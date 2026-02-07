use crate::error::RenderError;
use crate::models::DisplaySpec;
use eink_dither::{
    DitherAlgorithm, EinkDitherer, Palette as EinkPalette, RenderingIntent, Srgb as EinkSrgb,
};

/// Optional dithering parameter overrides (dev mode tuning).
#[derive(Debug, Default)]
pub struct DitherTuning {
    pub serpentine: Option<bool>,
    pub error_clamp: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub exact_absorb_error: Option<bool>,
}
use resvg::usvg::{self, Transform};
use std::io::Cursor;
use std::sync::Arc;
use tiny_skia::Pixmap;

/// Renders SVG to PNG with palette-aware dithering for e-ink displays.
///
/// All rendering goes through a single palette-based path. The PNG output
/// format is chosen automatically:
/// - Pure grey palette with ≤4 entries → grayscale color type 0, 2-bit
/// - Pure grey palette with 5-16 entries → grayscale color type 0, 4-bit
/// - Color palette → indexed color type 3 with PLTE chunk
pub struct SvgRenderer {
    /// Font database for text rendering
    fontdb: Arc<fontdb::Database>,
}

impl SvgRenderer {
    /// Create a new SVG renderer with fonts loaded from the provided data
    pub fn with_fonts(fonts: Vec<(String, std::borrow::Cow<'static, [u8]>)>) -> Self {
        let mut fontdb = fontdb::Database::new();

        for (name, data) in fonts {
            fontdb.load_font_data(data.into_owned());
            tracing::debug!(font = %name, "Loaded font");
        }

        // Load system fonts as fallback
        fontdb.load_system_fonts();

        tracing::info!(
            font_count = fontdb.len(),
            "Loaded fonts for SVG text rendering"
        );

        let families: std::collections::HashSet<_> = fontdb
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .collect();
        tracing::debug!(families = ?families, "Available font families");

        Self {
            fontdb: Arc::new(fontdb),
        }
    }

    /// Access the font database faces
    pub fn font_faces(&self) -> impl Iterator<Item = &fontdb::FaceInfo> {
        self.fontdb.faces()
    }

    /// Create a new SVG renderer with no custom fonts (system fonts only)
    pub fn new() -> Self {
        Self::with_fonts(Vec::new())
    }

    /// Render SVG to PNG using the given color palette.
    ///
    /// The output format is chosen automatically based on the palette content.
    /// The `dither` parameter selects the dithering algorithm:
    /// - `"photo"` / `"atkinson"` - Atkinson error diffusion (best color accuracy)
    /// - `"floyd-steinberg"` - Floyd-Steinberg with blue noise jitter (smooth gradients)
    /// - `"jarvis-judice-ninke"` - JJN error diffusion (wide kernel, least oscillation)
    /// - `"sierra"` - Sierra full error diffusion (wide kernel)
    /// - `"sierra-two-row"` - Sierra two-row error diffusion
    /// - `"sierra-lite"` - Sierra Lite error diffusion (minimal kernel)
    /// - `"graphics"` (default) - Blue noise ordered dithering
    /// - `"simplex"` - Barycentric ordered dithering (up to 4-color blending)
    ///
    /// When `actual` measured colors are provided, the ditherer uses them to model
    /// what the panel really displays. When `use_actual` is true, the PNG output
    /// uses measured colors (for dev mode preview); otherwise official colors are used.
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_palette_png(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
        actual: Option<&[(u8, u8, u8)]>,
        use_actual: bool,
        dither: Option<&str>,
        preserve_exact: bool,
        tuning: Option<&DitherTuning>,
    ) -> Result<Vec<u8>, RenderError> {
        let pixmap = self.rasterize_svg(svg_data, spec)?;

        // Build eink-dither palette with dedup (eink-dither rejects duplicates)
        let (eink_palette, output_palette) = build_eink_palette(palette, actual, use_actual)?;

        // Determine rendering intent and algorithm
        // All paths use Photo intent (saturation/contrast preprocessing + error diffusion)
        let algorithm = match dither {
            Some(s) if s.eq_ignore_ascii_case("floyd-steinberg") => {
                DitherAlgorithm::FloydSteinbergNoise
            }
            Some(s) if s.eq_ignore_ascii_case("jarvis-judice-ninke") => {
                DitherAlgorithm::JarvisJudiceNinkeNoise
            }
            Some(s) if s.eq_ignore_ascii_case("sierra") => DitherAlgorithm::SierraNoise,
            Some(s) if s.eq_ignore_ascii_case("sierra-two-row") => {
                DitherAlgorithm::SierraTwoRowNoise
            }
            Some(s) if s.eq_ignore_ascii_case("sierra-lite") => DitherAlgorithm::SierraLiteNoise,
            _ => DitherAlgorithm::Auto, // Atkinson
        };
        let intent = RenderingIntent::Photo;

        // Convert RGBA pixmap to eink-dither Srgb pixels
        let pixels = rgba_to_eink_srgb(pixmap.data());

        // Dither using eink-dither
        let mut ditherer = EinkDitherer::new(eink_palette, intent)
            .algorithm(algorithm)
            .preserve_exact_matches(preserve_exact);
        if let Some(t) = tuning {
            if let Some(s) = t.serpentine {
                ditherer = ditherer.serpentine(s);
            }
            if let Some(ec) = t.error_clamp {
                ditherer = ditherer.error_clamp(ec);
            }
            if let Some(cc) = t.chroma_clamp {
                ditherer = ditherer.chroma_clamp(cc);
            }
            if let Some(ns) = t.noise_scale {
                ditherer = ditherer.noise_scale(ns);
            }
            if let Some(ae) = t.exact_absorb_error {
                ditherer = ditherer.exact_absorb_error(ae);
            }
        }
        let result = ditherer.dither(&pixels, spec.width as usize, spec.height as usize);

        // eink-dither indices are into the deduped palette, which matches output_palette
        let indices: Vec<u8> = result.indices().to_vec();

        // Use output_palette for PNG encoding (measured colors in dev mode, official otherwise)
        let out = &output_palette;

        // Choose PNG format and pack pixel data.
        // When use_actual=true, always use indexed PNG so measured colors appear in PLTE.
        let (color_type, bit_depth, plte, packed) = if is_grey_palette(out) && !use_actual {
            if out.len() <= 4 {
                let mapped = map_grey_indices(&indices, out, 3);
                (
                    png::ColorType::Grayscale,
                    png::BitDepth::Two,
                    None,
                    pack_nbits(&mapped, spec.width, 2),
                )
            } else {
                let mapped = map_grey_indices(&indices, out, 15);
                (
                    png::ColorType::Grayscale,
                    png::BitDepth::Four,
                    None,
                    pack_nbits(&mapped, spec.width, 4),
                )
            }
        } else {
            let (depth, bits) = match out.len() {
                0..=2 => (png::BitDepth::One, 1),
                3..=4 => (png::BitDepth::Two, 2),
                5..=16 => (png::BitDepth::Four, 4),
                _ => (png::BitDepth::Eight, 8),
            };
            let plte: Vec<u8> = out.iter().flat_map(|&(r, g, b)| [r, g, b]).collect();
            let packed = if bits == 8 {
                indices
            } else {
                pack_nbits(&indices, spec.width, bits)
            };
            (png::ColorType::Indexed, depth, Some(plte), packed)
        };

        // Encode PNG (fast settings — oxipng will re-compress optimally)
        let png_bytes = encode_png(spec, color_type, bit_depth, plte.as_deref(), &packed)?;

        // Re-compress with oxipng (zopfli + adaptive filter selection)
        let optimized = oxipng::optimize_from_memory(
            &png_bytes,
            &oxipng::Options {
                strip: oxipng::StripChunks::Safe,
                optimize_alpha: false,
                ..Default::default()
            },
        )
        .unwrap_or(png_bytes);
        spec.validate_size(optimized.len())?;
        Ok(optimized)
    }

    /// Parse and rasterize SVG to an RGBA pixmap
    fn rasterize_svg(&self, svg_data: &[u8], spec: DisplaySpec) -> Result<Pixmap, RenderError> {
        let options = usvg::Options {
            fontdb: self.fontdb.clone(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_data(svg_data, &options)
            .map_err(|e| RenderError::SvgParse(e.to_string()))?;

        let svg_size = tree.size();
        let scale_x = spec.width as f32 / svg_size.width();
        let scale_y = spec.height as f32 / svg_size.height();
        let scale = scale_x.min(scale_y);

        let scaled_width = svg_size.width() * scale;
        let scaled_height = svg_size.height() * scale;
        let offset_x = (spec.width as f32 - scaled_width) / 2.0;
        let offset_y = (spec.height as f32 - scaled_height) / 2.0;

        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::WHITE);

        let transform = Transform::from_scale(scale, scale).post_translate(offset_x, offset_y);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap)
    }
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert RGBA pixel data to eink-dither Srgb, alpha-compositing against white.
fn rgba_to_eink_srgb(rgba_data: &[u8]) -> Vec<EinkSrgb> {
    rgba_data
        .chunks_exact(4)
        .map(|pixel| {
            let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
            if a == 255 {
                EinkSrgb::from_u8(r, g, b)
            } else if a == 0 {
                EinkSrgb::from_u8(255, 255, 255)
            } else {
                // Alpha composite against white
                let af = a as u16;
                let cr = ((r as u16 * af + 255 * (255 - af)) / 255) as u8;
                let cg = ((g as u16 * af + 255 * (255 - af)) / 255) as u8;
                let cb = ((b as u16 * af + 255 * (255 - af)) / 255) as u8;
                EinkSrgb::from_u8(cr, cg, cb)
            }
        })
        .collect()
}

/// Build an eink-dither palette from byonk's (u8,u8,u8) palette, deduplicating
/// colors since eink-dither rejects palettes with duplicate entries.
///
/// When `actual` measured colors are provided, they're passed to eink-dither so
/// dithering targets what the panel really displays. B&W forcing: if an official
/// color is exactly black or white, the measured value is forced to match.
///
/// Returns (eink_palette, output_palette) where output_palette uses
/// measured colors when `use_actual` is true, otherwise official colors.
type RgbTuple = (u8, u8, u8);

fn build_eink_palette(
    palette: &[RgbTuple],
    actual: Option<&[RgbTuple]>,
    use_actual: bool,
) -> Result<(EinkPalette, Vec<RgbTuple>), RenderError> {
    // Build actual colors with B&W forcing
    let actual_with_bw: Option<Vec<(u8, u8, u8)>> = actual.map(|a| {
        a.iter()
            .enumerate()
            .map(|(i, &(ar, ag, ab))| {
                if i < palette.len() {
                    let (or, og, ob) = palette[i];
                    // Force measured to match if official is pure black or white
                    if or == 0 && og == 0 && ob == 0 {
                        (0, 0, 0)
                    } else if or == 255 && og == 255 && ob == 255 {
                        (255, 255, 255)
                    } else {
                        (ar, ag, ab)
                    }
                } else {
                    (ar, ag, ab)
                }
            })
            .collect()
    });

    let mut unique_official: Vec<EinkSrgb> = Vec::new();
    let mut unique_actual: Vec<EinkSrgb> = Vec::new();
    // Track which original indices survived dedup, for building output_palette
    let mut kept_indices: Vec<usize> = Vec::new();

    for (orig_idx, &(r, g, b)) in palette.iter().enumerate() {
        let color = EinkSrgb::from_u8(r, g, b);
        let bytes = color.to_bytes();
        if !unique_official.iter().any(|c| c.to_bytes() == bytes) {
            kept_indices.push(orig_idx);
            unique_official.push(color);
            // Track corresponding actual color
            if let Some(ref abw) = actual_with_bw {
                if orig_idx < abw.len() {
                    let (ar, ag, ab) = abw[orig_idx];
                    unique_actual.push(EinkSrgb::from_u8(ar, ag, ab));
                }
            }
        }
    }

    let eink_actual = if !unique_actual.is_empty() && unique_actual.len() == unique_official.len() {
        Some(unique_actual.as_slice())
    } else {
        None
    };

    let eink_palette = EinkPalette::new(&unique_official, eink_actual)
        .map_err(|e| RenderError::Dither(format!("palette error: {e}")))?;

    // Build output palette: raw measured colors for dev preview, official for production.
    // Note: we use `actual` (without B&W forcing) for output so users see real panel colors.
    let output_palette: Vec<(u8, u8, u8)> = if use_actual {
        if let Some(a) = actual {
            kept_indices
                .iter()
                .map(|&i| if i < a.len() { a[i] } else { palette[i] })
                .collect()
        } else {
            kept_indices.iter().map(|&i| palette[i]).collect()
        }
    } else {
        kept_indices.iter().map(|&i| palette[i]).collect()
    };

    Ok((eink_palette, output_palette))
}

/// Check if a palette consists entirely of grey values (R == G == B).
fn is_grey_palette(palette: &[(u8, u8, u8)]) -> bool {
    palette.iter().all(|&(r, g, b)| r == g && g == b)
}

/// Map palette indices to native grayscale values (0..max_val).
fn map_grey_indices(indices: &[u8], palette: &[(u8, u8, u8)], max_val: u32) -> Vec<u8> {
    let max_level = (palette.len() - 1) as u32;
    let lut: Vec<u8> = (0..palette.len())
        .map(|i| ((i as u32 * max_val + max_level / 2) / max_level).min(max_val) as u8)
        .collect();
    indices.iter().map(|&idx| lut[idx as usize]).collect()
}

/// Encode packed pixel data as a PNG.
fn encode_png(
    spec: DisplaySpec,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    plte: Option<&[u8]>,
    packed: &[u8],
) -> Result<Vec<u8>, RenderError> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut buf, spec.width, spec.height);
        encoder.set_color(color_type);
        encoder.set_depth(bit_depth);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::FilterType::NoFilter);
        if let Some(plte) = plte {
            encoder.set_palette(plte);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|e| RenderError::PngEncode(e.to_string()))?;
        writer
            .write_image_data(packed)
            .map_err(|e| RenderError::PngEncode(e.to_string()))?;
    }
    Ok(buf.into_inner())
}

/// Pack pixel values into N-bit PNG row data (1, 2, or 4 bits per pixel).
fn pack_nbits(indices: &[u8], width: u32, bits: u8) -> Vec<u8> {
    let pixels_per_byte = 8 / bits as usize;
    let bytes_per_row = (width as usize).div_ceil(pixels_per_byte);
    let height = indices.len() / width as usize;
    let mask = (1u8 << bits) - 1;
    let mut packed = Vec::with_capacity(bytes_per_row * height);

    for row in indices.chunks(width as usize) {
        let mut byte = 0u8;
        for (i, &idx) in row.iter().enumerate() {
            let shift = (8 - bits) - (i % pixels_per_byte) as u8 * bits;
            byte |= (idx & mask) << shift;

            if (i % pixels_per_byte) == pixels_per_byte - 1 || i == row.len() - 1 {
                packed.push(byte);
                byte = 0;
            }
        }
    }

    packed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_font_families() {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts);

        let mut x11_families: Vec<_> = renderer
            .fontdb
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .filter(|name| name.starts_with("X11"))
            .collect();
        x11_families.sort();
        x11_families.dedup();
        for fam in &x11_families {
            println!("fontdb family: {}", fam);
        }
        assert!(!x11_families.is_empty(), "No X11 font families found");
    }

    #[test]
    fn test_bitmap_strikes_exposed() {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts);

        // X11Helv should have bitmap strikes
        let x11_face = renderer
            .font_faces()
            .find(|f| f.families.first().map(|(n, _)| n.as_str()) == Some("X11Helv"))
            .expect("X11Helv face not found");

        assert!(
            !x11_face.bitmap_strikes.is_empty(),
            "X11Helv should have bitmap strikes"
        );
        // Strikes should be sorted
        for w in x11_face.bitmap_strikes.windows(2) {
            assert!(w[0] <= w[1], "bitmap_strikes should be sorted");
        }
        println!("X11Helv bitmap strikes: {:?}", x11_face.bitmap_strikes);
    }

    #[test]
    fn test_bitmap_font_render() {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts);

        // Check what fontdb knows about X11Helv
        for face in renderer.fontdb.faces() {
            if let Some((name, _)) = face.families.first() {
                if name == "X11Helv" {
                    println!(
                        "Face: {} | style={:?} weight={:?} | source={:?}",
                        name, face.style, face.weight, face.source
                    );
                }
            }
        }

        // Render with bitmap fonts — font-size selects the bitmap strike
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 200" width="800" height="200">
          <rect width="800" height="200" fill="white"/>
          <text x="20" y="30" font-family="X11Helv" font-size="8" fill="black">X11Helv 8px: Hello World</text>
          <text x="20" y="60" font-family="NONEXISTENT_FONT" font-size="14" fill="black">NONEXISTENT: Hello World</text>
          <text x="20" y="90" font-family="X11Helv" font-size="14" fill="black">X11Helv 14px: Hello World</text>
        </svg>"#;

        let spec = DisplaySpec::from_dimensions(800, 200).unwrap();
        let palette = vec![(0, 0, 0), (255, 255, 255)];
        let png = renderer
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                true,
                None,
            )
            .unwrap();
        std::fs::write("/tmp/byonk-bitmap-font-test2.png", &png).unwrap();
        println!(
            "Wrote /tmp/byonk-bitmap-font-test2.png ({} bytes)",
            png.len()
        );
    }
}
