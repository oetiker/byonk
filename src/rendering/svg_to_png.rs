use crate::error::RenderError;
use crate::models::DisplaySpec;
use eink_dither::{
    DitherAlgorithm, EinkDitherer, GamutMapper, GamutOptions, Palette as EinkPalette,
    Srgb as EinkSrgb,
};

/// Optional per-render tuning overrides (dev mode, script, device config).
///
/// `gamut` belongs here rather than in a separate parameter because gamut
/// mapping runs in the same stage as dithering, against the same palette, and
/// is configured from the same places in the priority chain.
#[derive(Debug, Default)]
pub struct DitherTuning {
    pub serpentine: Option<bool>,
    pub error_clamp: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub strength: Option<f32>,
    /// Gamut mapping knobs. `None` uses [`GamutOptions::default`]; mapping
    /// still only happens where the document marks a continuous-tone region.
    pub gamut: Option<GamutOptions>,
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
    /// - `"atkinson-hybrid"` - Atkinson hybrid (100% achromatic / 75% chromatic)
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
        tuning: Option<&DitherTuning>,
    ) -> Result<Vec<u8>, RenderError> {
        let pixmap = self.rasterize_svg(svg_data, spec)?;

        // Build eink-dither palette with dedup (eink-dither rejects duplicates)
        let (eink_palette, output_palette) = build_eink_palette(palette, actual, use_actual)?;

        // Determine algorithm
        let algorithm = match dither {
            Some(s) if s.eq_ignore_ascii_case("atkinson-hybrid") => DitherAlgorithm::AtkinsonHybrid,
            Some(s) if s.eq_ignore_ascii_case("floyd-steinberg") => DitherAlgorithm::FloydSteinberg,
            Some(s) if s.eq_ignore_ascii_case("jarvis-judice-ninke") => {
                DitherAlgorithm::JarvisJudiceNinke
            }
            Some(s) if s.eq_ignore_ascii_case("sierra") => DitherAlgorithm::Sierra,
            Some(s) if s.eq_ignore_ascii_case("sierra-two-row") => DitherAlgorithm::SierraTwoRow,
            Some(s) if s.eq_ignore_ascii_case("sierra-lite") => DitherAlgorithm::SierraLite,
            Some(s) if s.eq_ignore_ascii_case("stucki") => DitherAlgorithm::Stucki,
            Some(s) if s.eq_ignore_ascii_case("burkes") => DitherAlgorithm::Burkes,
            _ => DitherAlgorithm::Atkinson,
        };

        // Convert RGBA pixmap to eink-dither Srgb pixels
        let mut pixels = rgba_to_eink_srgb(pixmap.data());

        // The tone mask has three consumers: gamut mapping acts INSIDE marked
        // regions; outside them the colour model is nominal and exact matches
        // are pinned; and no error crosses between the two. So it is rasterized
        // whenever the document carries markup, and only the mapping is skipped
        // when amount is zero. An unmarked document skips the second
        // rasterization entirely: every pixel is structure.
        let tone_mask: Option<Vec<bool>> = if crate::rendering::tone_mask::has_tone_markup(svg_data)
        {
            let mask = self.rasterize_tone_mask(svg_data, spec)?;
            if mask.len() != pixels.len() {
                // Cannot happen: both rasterize to `spec`. Loud rather than
                // silently skipped.
                return Err(RenderError::Dither(format!(
                    "tone mask length {} does not match frame {}",
                    mask.len(),
                    pixels.len()
                )));
            }
            Some(mask)
        } else {
            None
        };

        if let Some(mask) = tone_mask.as_ref() {
            let gamut_opts = tuning.and_then(|t| t.gamut).unwrap_or_default();
            if gamut_opts.amount != 0.0 {
                let marked = mask.iter().filter(|m| **m).count();
                tracing::debug!(
                    marked_pixels = marked,
                    total_pixels = pixels.len(),
                    knee = gamut_opts.knee,
                    amount = gamut_opts.amount,
                    max_compression = gamut_opts.max_compression,
                    "applying gamut mapping to continuous-tone regions"
                );
                GamutMapper::new(&eink_palette).map_frame(&mut pixels, mask, gamut_opts);
            }
        }

        // Passed through unchanged: this IS the tone mask, not its inverse.
        // A document with no markup is all-structure.
        let continuous: Vec<bool> = match tone_mask {
            Some(mask) => mask,
            None => vec![false; pixels.len()],
        };

        // Dither using eink-dither
        let mut ditherer = EinkDitherer::new(eink_palette).algorithm(algorithm);
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
            if let Some(st) = t.strength {
                ditherer = ditherer.strength(st);
            }
        }
        let result = ditherer.dither_with_regions(
            &pixels,
            spec.width as usize,
            spec.height as usize,
            Some(&continuous),
        );

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
        let optimized = optimize_png(png_bytes);
        spec.validate_size(optimized.len())?;
        Ok(optimized)
    }

    /// Render SVG straight to a full-color 8-bit RGB PNG, with no e-ink
    /// dithering or palette restriction applied — the pre-dither preview
    /// authoring diagnostics compare the palette-restricted device output
    /// against. Reuses the same rasterize/encode helpers
    /// `render_to_palette_png` uses; skips the `eink-dither` step entirely,
    /// so this never duplicates the dithering logic.
    pub fn render_to_raw_png(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
    ) -> Result<Vec<u8>, RenderError> {
        let pixmap = self.rasterize_svg(svg_data, spec)?;
        let rgb: Vec<u8> = rgba_to_eink_srgb(pixmap.data())
            .into_iter()
            .flat_map(|c| c.to_bytes())
            .collect();

        let png_bytes = encode_png(spec, png::ColorType::Rgb, png::BitDepth::Eight, None, &rgb)?;
        Ok(optimize_png(png_bytes))
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
        let transform = Self::fit_transform(svg_size.width(), svg_size.height(), spec);

        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::WHITE);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap)
    }

    /// The scale-and-centre transform that fits an SVG of `svg_w` x `svg_h`
    /// into `spec`.
    ///
    /// Shared by the frame and the tone mask: if these two ever disagreed the
    /// mask would be offset from the pixels it selects. Takes plain floats
    /// rather than a size type so it does not depend on which crate
    /// `usvg::Tree::size` currently returns.
    fn fit_transform(svg_w: f32, svg_h: f32, spec: DisplaySpec) -> Transform {
        let scale = (spec.width as f32 / svg_w).min(spec.height as f32 / svg_h);
        let offset_x = (spec.width as f32 - svg_w * scale) / 2.0;
        let offset_y = (spec.height as f32 - svg_h * scale) / 2.0;
        Transform::from_scale(scale, scale).post_translate(offset_x, offset_y)
    }

    /// Rasterize the tone mask for `svg_data`.
    ///
    /// The mask document is the original with paint forced to white inside
    /// `data-byonk-tone="continuous"` subtrees and black elsewhere, drawn over
    /// a black background so unpainted area reads as unmarked. Edge
    /// antialiasing produces greys; threshold at 0.5.
    ///
    /// Failure is a hard error, deliberately. The mask comes from a document
    /// that just rasterized successfully, by the same renderer, with only
    /// paint values changed — so the realistic failure paths are all our own
    /// bugs in the rewriter. Silently rendering something materially different
    /// while reporting success is the failure mode that costs hours.
    fn rasterize_tone_mask(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
    ) -> Result<Vec<bool>, RenderError> {
        let mask_svg = crate::rendering::tone_mask::build_mask_svg(svg_data)
            .map_err(|e| RenderError::SvgParse(format!("tone mask: {e}")))?;

        let options = usvg::Options {
            fontdb: self.fontdb.clone(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_data(&mask_svg, &options)
            .map_err(|e| RenderError::SvgParse(format!("tone mask: {e}")))?;

        let svg_size = tree.size();
        let transform = Self::fit_transform(svg_size.width(), svg_size.height(), spec);

        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::BLACK);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap
            .data()
            .chunks_exact(4)
            .map(|px| {
                // Premultiplied RGBA over an opaque black fill: the green
                // channel alone separates white from black cleanly.
                px[1] >= 128
            })
            .collect())
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
        // Never fail a device render over calibration — but do not lose it
        // silently either. After the length check in
        // `api::display::resolve_measured_colors` this should be unreachable
        // from the script path; if it fires, something upstream disagrees
        // about palette length (e.g. dedup removed a duplicate official
        // colour without a matching measured entry).
        if actual.is_some() {
            tracing::warn!(
                official = unique_official.len(),
                measured = unique_actual.len(),
                "measured colours dropped: length disagrees with the deduplicated \
                 official palette; dithering will target the official colours"
            );
        }
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

/// Re-compress a PNG with oxipng (zopfli + adaptive filter selection).
/// Falls back to the input bytes unchanged if optimization fails. Shared by
/// `render_to_palette_png` (dithered device output) and `render_to_raw_png`
/// (pre-dither preview) — both encode fast/uncompressed via `encode_png`
/// and rely on this pass for the real compression.
fn optimize_png(png_bytes: Vec<u8>) -> Vec<u8> {
    oxipng::optimize_from_memory(
        &png_bytes,
        &oxipng::Options {
            strip: oxipng::StripChunks::Safe,
            optimize_alpha: false,
            ..Default::default()
        },
    )
    .unwrap_or(png_bytes)
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
    fn build_eink_palette_drops_mismatched_actual_but_still_builds() {
        // Three official colours, two measured: the measured list is dropped
        // (never fail a device render) — but this must not be silent.
        let official = vec![(0, 0, 0), (255, 255, 255), (255, 0, 0)];
        let actual = vec![(10, 10, 10), (232, 230, 224)];
        let (_palette, output) =
            build_eink_palette(&official, Some(&actual), false).expect("must still build");
        assert_eq!(output, official);
    }

    #[test]
    fn build_eink_palette_keeps_matched_actual() {
        let official = vec![(0, 0, 0), (255, 255, 255), (255, 0, 0)];
        let actual = vec![(10, 10, 10), (232, 230, 224), (168, 58, 48)];
        let (palette, output) =
            build_eink_palette(&official, Some(&actual), true).expect("must build");
        // use_actual = true draws the output in the measured colours, except
        // that pure black/white are forced to match — the existing B&W rule
        // applies to the dither palette, while `output` uses raw measured.
        assert_eq!(output, actual);

        // The regression this test exists to catch: matched measured
        // colours must actually reach the `EinkPalette` that dithering
        // matches against, not just the display-facing `output` tuple
        // (which is computed independently of `eink_actual`). Assert on
        // `palette.actual(idx)` directly — the B&W-forced entries (0, 1)
        // collapse to pure black/white, while entry 2 carries the raw
        // measured red through unchanged.
        assert_eq!(palette.actual(0), EinkSrgb::from_u8(0, 0, 0));
        assert_eq!(palette.actual(1), EinkSrgb::from_u8(255, 255, 255));
        assert_eq!(palette.actual(2), EinkSrgb::from_u8(168, 58, 48));
    }

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
            .render_to_palette_png(svg.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();
        std::fs::write("/tmp/byonk-bitmap-font-test2.png", &png).unwrap();
        println!(
            "Wrote /tmp/byonk-bitmap-font-test2.png ({} bytes)",
            png.len()
        );
    }

    #[test]
    fn tone_mask_marks_only_the_marked_region() {
        let renderer = SvgRenderer::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="50" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let mask = renderer
            .rasterize_tone_mask(svg.as_bytes(), spec)
            .expect("mask must rasterize");

        assert_eq!(mask.len(), 100 * 100);
        assert!(mask[50 * 100 + 25], "left half must be marked");
        assert!(!mask[50 * 100 + 75], "right half must not be marked");
    }

    #[test]
    fn tone_mask_respects_occlusion_by_unmarked_shapes() {
        let renderer = SvgRenderer::new();
        // A marked photo area with an unmarked label drawn over its middle.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="100" height="100" fill="#336699"/>
            </g>
            <rect x="40" y="40" width="20" height="20" fill="#000000"/>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let mask = renderer.rasterize_tone_mask(svg.as_bytes(), spec).unwrap();
        assert!(mask[10 * 100 + 10], "photo area must be marked");
        assert!(
            !mask[50 * 100 + 50],
            "the occluding unmarked rect must punch through the mask"
        );
    }

    /// A camelCase attribute must survive the rewrite. XML attribute names are
    /// case-sensitive, so emitting `viewbox` silently drops the coordinate
    /// system and the mask rasterizes empty — while every test whose viewBox
    /// matches its width/height still passes.
    #[test]
    fn tone_mask_preserves_camelcase_attributes() {
        let renderer = SvgRenderer::new();
        // viewBox deliberately differs from width/height.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="400" height="400">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="50" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        let mask_doc = crate::rendering::tone_mask::build_mask_svg(svg.as_bytes()).unwrap();
        let text = String::from_utf8_lossy(&mask_doc);
        assert!(
            text.contains("viewBox"),
            "viewBox must keep its case: {text}"
        );

        let spec = DisplaySpec::from_dimensions(400, 400).unwrap();
        let mask = renderer.rasterize_tone_mask(svg.as_bytes(), spec).unwrap();
        let row = 200usize;
        let last = (0..400).rev().find(|&x| mask[row * 400 + x]);
        assert_eq!(
            last,
            Some(199),
            "the marked half must scale with the viewBox"
        );
    }

    /// The mask must not invent a stroke. This is the only kind of test that
    /// can catch it: the rewriter's own tests assert on the mask *document*,
    /// and an added `stroke` is only visible once the document is rasterized.
    ///
    /// Measured before the fix: case A marked 50..=150 and case B 40..=159.
    #[test]
    fn tone_mask_edge_does_not_spill_past_an_unstroked_shape() {
        let renderer = SvgRenderer::new();
        let spec = DisplaySpec::from_dimensions(200, 200).unwrap();
        let span = |svg: &str| {
            let mask = renderer.rasterize_tone_mask(svg.as_bytes(), spec).unwrap();
            let row = 100usize;
            let first = (0..200).find(|&x| mask[row * 200 + x]).unwrap();
            let last = (0..200).rev().find(|&x| mask[row * 200 + x]).unwrap();
            (first, last)
        };

        // A: a plain unstroked shape. SVG's initial stroke is `none`, so the
        // mask edge must land exactly on the geometry.
        let plain = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="50" y="50" width="100" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        assert_eq!(span(plain), (50, 149), "an unstroked shape must not widen");

        // B: `stroke` is a paint property, so the stylesheet's `stroke: none`
        // is stripped — while `stroke-width` survives as geometry. Inventing a
        // stroke here would widen the mask by half of it, unboundedly.
        let css_width = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <style>.p { stroke: none; stroke-width: 20; }</style>
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect class="p" x="50" y="50" width="100" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        assert_eq!(
            span(css_width),
            (50, 149),
            "a stripped stroke must not resurrect via stroke-width"
        );

        // C: the control. A shape that genuinely IS stroked must still mark its
        // stroke, or the fix would just be deleting strokes.
        let stroked = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="50" y="50" width="100" height="100" fill="#336699" stroke="#336699" stroke-width="20"/>
            </g>
          </svg>"##;
        assert_eq!(
            span(stroked),
            (40, 159),
            "a real stroke must still be marked"
        );
    }

    /// The opt-in guarantee: an unmarked document must render byte-identically
    /// whether or not the gamut knobs are present.
    #[test]
    fn unmarked_document_renders_byte_identically() {
        let renderer = SvgRenderer::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <rect x="10" y="10" width="60" height="60" fill="#c06020"/>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let palette = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
        ];

        let plain = renderer
            .render_to_palette_png(svg.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();

        let tuning = DitherTuning {
            gamut: Some(eink_dither::GamutOptions::default()),
            ..Default::default()
        };
        let with_knobs = renderer
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                Some(&tuning),
            )
            .unwrap();

        assert_eq!(plain, with_knobs, "unmarked document must be unaffected");
    }

    /// A marked vivid region must actually change.
    #[test]
    fn marked_region_is_altered_by_mapping() {
        let renderer = SvgRenderer::new();
        let marked = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="100" height="100" fill="#ff00aa"/>
            </g>
          </svg>"##;
        let unmarked = marked.replace(r#" data-byonk-tone="continuous""#, "");
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let palette = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
        ];

        let a = renderer
            .render_to_palette_png(marked.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();
        let b = renderer
            .render_to_palette_png(unmarked.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();

        assert_ne!(a, b, "marking a vivid region must change the output");
    }

    // ---- Tone-mask fixtures shared by the pinning/containment tests ----

    /// The panel's six nominal inks, in the order byonk ships them.
    fn six_ink_official() -> Vec<(u8, u8, u8)> {
        vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (255, 255, 0),
            (0, 0, 255),
            (0, 255, 0),
        ]
    }

    /// A 32x32 document: a saturated field with one 1 px vertical line of
    /// `line` at x = 15, optionally marked continuous.
    ///
    /// `#FF00AA` is far from every ink in `six_ink_official`, so it diffuses
    /// hard into the line — measured below: an *unpinnable* line here keeps
    /// only 12.5% of its colour.
    fn line_in_hostile_field(line: &str, marked: bool) -> String {
        let attr = if marked {
            r#" data-byonk-tone="continuous""#
        } else {
            ""
        };
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
<rect x="0" y="0" width="32" height="32" fill="#FF00AA"/>
<rect{attr} x="15" y="0" width="1" height="32" fill="{line}"/>
</svg>"##
        )
    }

    /// Fraction of the 1 px line in `line_in_hostile_field` that came out
    /// pure black.
    fn line_black_share(svg: &str) -> f64 {
        let spec = DisplaySpec {
            width: 32,
            height: 32,
            max_size_bytes: 200_000,
        };
        let png = SvgRenderer::new()
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &six_ink_official(),
                None,
                false,
                None,
                None,
            )
            .expect("render failed");
        let img = image::load_from_memory(&png)
            .expect("decode failed")
            .to_rgb8();
        let black = (0..32u32)
            .filter(|&y| img.get_pixel(15, y).0 == [0, 0, 0])
            .count();
        black as f64 / 32.0
    }

    /// Structural content keeps its pure inks (ruling 18's pinning), and a
    /// marked region is shielded from the field's error (ruling 23's
    /// containment).
    ///
    /// Three renders of the same geometry — a 1 px line through a field that
    /// diffuses hard into it — differing only in the line's fill (`#000000`
    /// vs `#010101`, one 8-bit step apart and visually identical) and in
    /// whether the line is marked:
    ///
    /// - **pure, unmarked**: pinnable, so it must survive.
    /// - **near-black, unmarked**: the control. Same geometry, same field,
    ///   but `(1,1,1)` is not a palette entry so it cannot be pinned. It must
    ///   still be eroded — that is what proves the pure line *needed*
    ///   rescuing and that pinning, not the geometry, is what rescued it.
    /// - **near-black, marked**: differs from the control only by the mark.
    ///   Still unpinnable, so its survival can only come from error
    ///   containment at the region boundary.
    ///
    /// Note the marked arm is NOT a "must be eroded" control: containment
    /// protects a marked region from outside error too. See the task report.
    #[test]
    fn structural_pure_ink_is_pinned_and_marked_regions_are_shielded() {
        let pure_unmarked = line_black_share(&line_in_hostile_field("#000000", false));
        let near_unmarked = line_black_share(&line_in_hostile_field("#010101", false));
        let near_marked = line_black_share(&line_in_hostile_field("#010101", true));

        assert!(
            near_unmarked < 0.5,
            "the unpinnable control line kept {:.1}% of its black, so this field does not \
             erode anything and the test cannot attribute the pure line's survival to \
             pinning — report it",
            near_unmarked * 100.0
        );
        assert!(
            pure_unmarked > 0.99,
            "only {:.1}% of the unmarked pure-black line stayed black (the unpinnable \
             control in the same geometry kept {:.1}%) — it is not being pinned",
            pure_unmarked * 100.0,
            near_unmarked * 100.0
        );
        assert!(
            near_marked > 0.99,
            "the marked line kept only {:.1}% of its black against the unmarked control's \
             {:.1}% — error is still crossing into the marked region",
            near_marked * 100.0,
            near_unmarked * 100.0
        );
    }

    /// Unmarked content is matched against the NOMINAL inks; marked content
    /// against the MEASURED ones (ruling 22).
    ///
    /// The fill is `#00FE00` — one 8-bit step off the nominal green — so it is
    /// **not** a palette entry and can never be pinned. Whatever difference the
    /// two arms show is therefore the colour model and nothing else.
    ///
    /// The marked arm sets `gamut.amount = 0.0` so that gamut mapping, the
    /// mask's other consumer, cannot contribute to the difference either.
    #[test]
    fn unmarked_content_is_matched_against_nominal_inks() {
        let official = six_ink_official();
        // byonk's shipped measured inks for this panel.
        let measured: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (0xB5, 0x03, 0x03),
            (0xFF, 0xEE, 0x00),
            (0x20, 0x54, 0x97),
            (0x0D, 0x87, 0x6B),
        ];
        assert_ne!(
            official[5], measured[5],
            "the two colour models are identical for green, so this test cannot see the switch"
        );

        let green_share = |marked: bool| -> f64 {
            let attr = if marked {
                r#" data-byonk-tone="continuous""#
            } else {
                ""
            };
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
<rect{attr} x="0" y="0" width="32" height="32" fill="#00FE00"/>
</svg>"##
            );
            let tuning = DitherTuning {
                gamut: Some(GamutOptions {
                    amount: 0.0,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let spec = DisplaySpec {
                width: 32,
                height: 32,
                max_size_bytes: 200_000,
            };
            let png = SvgRenderer::new()
                .render_to_palette_png(
                    svg.as_bytes(),
                    spec,
                    &official,
                    Some(&measured),
                    false,
                    None,
                    Some(&tuning),
                )
                .expect("render failed");
            let img = image::load_from_memory(&png)
                .expect("decode failed")
                .to_rgb8();
            let green = img.pixels().filter(|p| p.0 == [0, 255, 0]).count();
            green as f64 / (32.0 * 32.0)
        };

        let unmarked = green_share(false);
        let marked = green_share(true);

        assert!(
            marked < 0.10,
            "the marked arm came out {:.1}% green, so measured matching is not sending this \
             fill to yellow and the comparison proves nothing — report it",
            marked * 100.0
        );
        assert!(
            unmarked > 0.99,
            "the unmarked arm came out only {:.1}% green against the marked arm's {:.1}%: \
             unmarked content is not being matched against the nominal inks",
            unmarked * 100.0,
            marked * 100.0
        );
    }

    /// Diagnostic (ruling 20: non-asserting; the printed output is the
    /// deliverable): the content-adaptation factor `R` for a **real screen**,
    /// derived from the pixels its tone mask actually marks.
    ///
    /// `crates/eink-dither/tests/gamut_adaptation_diag.rs` prints `R` for a
    /// synthetic `rho` field only. It cannot do this one: `eink-dither` has no
    /// dependencies at all and so cannot parse or rasterize an SVG. This lives
    /// in `byonk`, where `resvg`/`usvg` and `rasterize_tone_mask` are.
    ///
    /// Run with:
    ///     cargo test -p byonk --lib tone_screen_adaptation_factor -- --ignored --nocapture
    #[test]
    #[ignore = "diagnostic; prints R for the tone calibration screen"]
    fn tone_screen_adaptation_factor() {
        use crate::assets::AssetLoader;
        use crate::services::screen_repo_cache::ScreenRepoCache;
        use crate::services::screen_repo_manager::ScreenRepoManager;
        use crate::services::{ContentPipeline, DeviceContext, RenderService};
        use eink_dither::gamut::adapt::{adaptation_factor, MIN_DISCARD, PERCENTILE};

        // The screen as a device sees it: reterminal_e1002, 800x480.
        let spec = DisplaySpec::from_dimensions(800, 480).unwrap();
        let official: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (255, 255, 0),
            (0, 0, 255),
            (0, 255, 0),
        ];
        let actual: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (0xB5, 0x03, 0x03),
            (0xFF, 0xEE, 0x00),
            (0x20, 0x54, 0x97),
            (0x0D, 0x87, 0x6B),
        ];

        let loader = Arc::new(AssetLoader::new(None, None, None));
        let shared: crate::server::SharedConfig = Arc::new(arc_swap::ArcSwap::from(Arc::new(
            crate::models::AppConfig::default(),
        )));
        let render_service = Arc::new(RenderService::new(&loader).unwrap());
        let cache_root = std::env::temp_dir().join(format!(
            "byonk_tone_diag_cache_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let pm = ScreenRepoManager::new(
            loader.clone(),
            shared.clone(),
            ScreenRepoCache::new(cache_root.clone()),
            Default::default(),
            None,
            None,
        );
        pm.rebuild_loader();
        let pipeline = ContentPipeline::new(shared, loader, render_service, pm).unwrap();

        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:01".to_string(),
            model: Some("og".to_string()),
            width: Some(spec.width),
            height: Some(spec.height),
            colors: Some(
                official
                    .iter()
                    .map(|&(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"))
                    .collect(),
            ),
            colors_actual: Some(
                actual
                    .iter()
                    .map(|&(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"))
                    .collect(),
            ),
            ..Default::default()
        };

        let script = pipeline
            .run_screen_by_name(
                "byonk-builtin/calibration/tone",
                Default::default(),
                Some(ctx.clone()),
            )
            .expect("tone screen runs");
        let svg = pipeline
            .render_svg_from_script(&script, Some(&ctx))
            .expect("tone screen templates");

        let renderer = SvgRenderer::new();
        let pixmap = renderer.rasterize_svg(svg.as_bytes(), spec).unwrap();
        let pixels = rgba_to_eink_srgb(pixmap.data());
        let mask = renderer.rasterize_tone_mask(svg.as_bytes(), spec).unwrap();
        assert_eq!(mask.len(), pixels.len());

        let (eink_palette, _) = build_eink_palette(&official, Some(&actual), false).unwrap();
        let mapper = GamutMapper::new(&eink_palette);
        let opts = GamutOptions::default();

        let mut rhos: Vec<f32> = pixels
            .iter()
            .zip(mask.iter())
            .filter(|(_, m)| **m)
            .map(|(p, _)| mapper.rho(*p))
            .collect();

        let marked = rhos.len();
        let mut sorted = rhos.clone();
        sorted.sort_by(f32::total_cmp);
        let q = |f: f32| sorted[((sorted.len() - 1) as f32 * f) as usize];
        println!(
            "tone screen {}x{}: {marked} marked pixels of {} ({:.1}%)",
            spec.width,
            spec.height,
            pixels.len(),
            marked as f32 * 100.0 / pixels.len() as f32
        );
        println!(
            "  rho over the marked set: p50={:.4} p90={:.4} p99={:.4} max={:.4}",
            q(0.5),
            q(0.9),
            q(0.99),
            q(1.0)
        );
        let discard = MIN_DISCARD
            .max((marked as f32 * (1.0 - PERCENTILE)).ceil() as usize)
            .min(marked.saturating_sub(1));
        println!("  PERCENTILE={PERCENTILE} MIN_DISCARD={MIN_DISCARD} -> discarding {discard}");
        let r = adaptation_factor(&mut rhos, opts.max_compression);
        println!(
            "ADAPTATION FACTOR R = {r:.4}   (max_compression cap = {})",
            opts.max_compression
        );

        let _ = std::fs::remove_dir_all(&cache_root);
    }
}
