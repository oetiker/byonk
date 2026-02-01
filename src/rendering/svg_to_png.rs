use crate::error::RenderError;
use crate::models::DisplaySpec;
use crate::rendering::dither::palette_dither;
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

    /// Create a new SVG renderer with no custom fonts (system fonts only)
    pub fn new() -> Self {
        Self::with_fonts(Vec::new())
    }

    /// Render SVG to PNG using the given color palette.
    ///
    /// The output format is chosen automatically based on the palette content.
    pub fn render_to_palette_png(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
    ) -> Result<Vec<u8>, RenderError> {
        let pixmap = self.rasterize_svg(svg_data, spec)?;

        // Dither RGBA data to palette indices
        let indices = palette_dither(pixmap.data(), spec.width, spec.height, palette, None);

        // Choose optimal PNG encoding based on palette
        if is_grey_palette(palette) {
            let levels = palette.len() as u8;
            if levels <= 4 {
                self.encode_grey_2bit(&indices, spec, palette)
            } else {
                self.encode_grey_4bit(&indices, spec, palette)
            }
        } else {
            self.encode_indexed_png(&indices, spec, palette)
        }
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

    /// Encode as 2-bit native grayscale PNG (color type 0).
    ///
    /// Palette indices are mapped to grey values for the PNG data.
    fn encode_grey_2bit(
        &self,
        indices: &[u8],
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
    ) -> Result<Vec<u8>, RenderError> {
        let mut buf = Cursor::new(Vec::new());

        {
            let mut encoder = png::Encoder::new(&mut buf, spec.width, spec.height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Two);
            encoder.set_compression(png::Compression::Best);
            encoder.set_filter(png::FilterType::Paeth);

            let mut writer = encoder
                .write_header()
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;

            // Map palette indices to 2-bit grey values (0-3)
            let packed = pack_2bit_grey(indices, spec.width, palette);

            writer
                .write_image_data(&packed)
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;
        }

        let png_bytes = buf.into_inner();
        spec.validate_size(png_bytes.len())?;
        Ok(png_bytes)
    }

    /// Encode as 4-bit native grayscale PNG (color type 0).
    fn encode_grey_4bit(
        &self,
        indices: &[u8],
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
    ) -> Result<Vec<u8>, RenderError> {
        let mut buf = Cursor::new(Vec::new());

        {
            let mut encoder = png::Encoder::new(&mut buf, spec.width, spec.height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Four);
            encoder.set_compression(png::Compression::Best);
            encoder.set_filter(png::FilterType::Paeth);

            let mut writer = encoder
                .write_header()
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;

            let packed = pack_4bit_grey(indices, spec.width, palette);

            writer
                .write_image_data(&packed)
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;
        }

        let png_bytes = buf.into_inner();
        spec.validate_size(png_bytes.len())?;
        Ok(png_bytes)
    }

    /// Encode as indexed PNG (color type 3) with PLTE chunk.
    fn encode_indexed_png(
        &self,
        indices: &[u8],
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
    ) -> Result<Vec<u8>, RenderError> {
        let mut buf = Cursor::new(Vec::new());

        {
            let mut encoder = png::Encoder::new(&mut buf, spec.width, spec.height);
            encoder.set_color(png::ColorType::Indexed);

            let bit_depth = if palette.len() <= 2 {
                png::BitDepth::One
            } else if palette.len() <= 4 {
                png::BitDepth::Two
            } else if palette.len() <= 16 {
                png::BitDepth::Four
            } else {
                png::BitDepth::Eight
            };
            encoder.set_depth(bit_depth);
            encoder.set_compression(png::Compression::Best);
            encoder.set_filter(png::FilterType::Paeth);

            let plte: Vec<u8> = palette.iter().flat_map(|&(r, g, b)| [r, g, b]).collect();
            encoder.set_palette(plte);

            let mut writer = encoder
                .write_header()
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;

            let packed = match bit_depth {
                png::BitDepth::One => pack_nbits(indices, spec.width, 1),
                png::BitDepth::Two => pack_nbits(indices, spec.width, 2),
                png::BitDepth::Four => pack_nbits(indices, spec.width, 4),
                _ => indices.to_vec(),
            };

            writer
                .write_image_data(&packed)
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;
        }

        let png_bytes = buf.into_inner();
        spec.validate_size(png_bytes.len())?;
        Ok(png_bytes)
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

/// Check if a palette consists entirely of grey values (R == G == B),
/// sorted from dark to light.
fn is_grey_palette(palette: &[(u8, u8, u8)]) -> bool {
    palette.iter().all(|&(r, g, b)| r == g && g == b)
}

/// Pack palette indices into 2-bit grayscale PNG data.
///
/// Each palette index is mapped to a 2-bit grey value (0-3) derived from
/// the palette entry's red channel (palette must be grey).
fn pack_2bit_grey(indices: &[u8], width: u32, palette: &[(u8, u8, u8)]) -> Vec<u8> {
    // Build index → 2-bit grey value lookup
    let max_level = (palette.len() - 1) as u32;
    let grey_lut: Vec<u8> = palette
        .iter()
        .enumerate()
        .map(|(i, _)| {
            // Map palette index to 0-3 range
            ((i as u32 * 3 + max_level / 2) / max_level).min(3) as u8
        })
        .collect();

    let bytes_per_row = (width as usize).div_ceil(4);
    let height = indices.len() / width as usize;
    let mut packed = Vec::with_capacity(bytes_per_row * height);

    for row in indices.chunks(width as usize) {
        let mut byte = 0u8;
        let mut bit_pos = 6;

        for (i, &idx) in row.iter().enumerate() {
            let value = grey_lut[idx as usize];
            byte |= value << bit_pos;

            if bit_pos == 0 || i == row.len() - 1 {
                packed.push(byte);
                byte = 0;
                bit_pos = 6;
            } else {
                bit_pos -= 2;
            }
        }
    }

    packed
}

/// Pack palette indices into 4-bit grayscale PNG data.
fn pack_4bit_grey(indices: &[u8], width: u32, palette: &[(u8, u8, u8)]) -> Vec<u8> {
    let max_level = (palette.len() - 1) as u32;
    let grey_lut: Vec<u8> = palette
        .iter()
        .enumerate()
        .map(|(i, _)| ((i as u32 * 15 + max_level / 2) / max_level).min(15) as u8)
        .collect();

    let bytes_per_row = (width as usize).div_ceil(2);
    let height = indices.len() / width as usize;
    let mut packed = Vec::with_capacity(bytes_per_row * height);

    for row in indices.chunks(width as usize) {
        let mut high = true;
        let mut byte = 0u8;

        for (i, &idx) in row.iter().enumerate() {
            let value = grey_lut[idx as usize];
            if high {
                byte = value << 4;
                high = false;
            } else {
                byte |= value;
                packed.push(byte);
                byte = 0;
                high = true;
            }

            if i == row.len() - 1 && !high {
                packed.push(byte);
            }
        }
    }

    packed
}

/// Pack palette indices into N-bit indexed PNG data (1, 2, or 4 bits per pixel).
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
            .render_to_palette_png(svg.as_bytes(), spec, &palette)
            .unwrap();
        std::fs::write("/tmp/byonk-bitmap-font-test2.png", &png).unwrap();
        println!(
            "Wrote /tmp/byonk-bitmap-font-test2.png ({} bytes)",
            png.len()
        );
    }
}
