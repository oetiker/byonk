use crate::error::RenderError;
use crate::models::DisplaySpec;
use crate::rendering::dither::blue_noise_dither;
use resvg::usvg::{self, Transform};
use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Arc;
use tiny_skia::Pixmap;

/// Renders SVG to grayscale PNG with dithering for e-ink displays
pub struct SvgRenderer {
    /// Number of grayscale levels (4 for 2-bit)
    pub levels: u8,
    /// Font database for text rendering
    fontdb: Arc<fontdb::Database>,
}

impl SvgRenderer {
    /// Create a new SVG renderer with fonts loaded from the provided data
    ///
    /// The `fonts` parameter is a list of (filename, data) tuples, typically
    /// from `AssetLoader::get_fonts()`.
    pub fn with_fonts(fonts: Vec<(String, Cow<'static, [u8]>)>) -> Self {
        let mut fontdb = fontdb::Database::new();

        // Load fonts from provided data (embedded or external)
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

        // Log available font families for debugging
        let families: std::collections::HashSet<_> = fontdb
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .collect();
        tracing::debug!(families = ?families, "Available font families");

        Self {
            levels: 4, // 2-bit = 4 levels
            fontdb: Arc::new(fontdb),
        }
    }

    /// Create a new SVG renderer with no custom fonts (system fonts only)
    pub fn new() -> Self {
        Self::with_fonts(Vec::new())
    }

    /// Render SVG to grayscale PNG with dithering
    pub fn render_to_png(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
    ) -> Result<Vec<u8>, RenderError> {
        // Parse SVG with font database for text support
        let options = usvg::Options {
            fontdb: self.fontdb.clone(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_data(svg_data, &options)
            .map_err(|e| RenderError::SvgParse(e.to_string()))?;

        // Calculate scale to fit display while maintaining aspect ratio
        let svg_size = tree.size();
        let scale_x = spec.width as f32 / svg_size.width();
        let scale_y = spec.height as f32 / svg_size.height();
        let scale = scale_x.min(scale_y);

        // Calculate offset to center the image
        let scaled_width = svg_size.width() * scale;
        let scaled_height = svg_size.height() * scale;
        let offset_x = (spec.width as f32 - scaled_width) / 2.0;
        let offset_y = (spec.height as f32 - scaled_height) / 2.0;

        // Create pixmap with white background
        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::WHITE);

        // Render with transform (scale and center)
        let transform = Transform::from_scale(scale, scale).post_translate(offset_x, offset_y);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        // Convert RGBA to grayscale
        let gray_data = self.to_grayscale(pixmap.data());

        // Apply blue-noise-modulated error diffusion for 2-bit output
        let dithered = blue_noise_dither(&gray_data, spec.width, spec.height, self.levels, None);

        // Encode to PNG
        self.encode_png(&dithered, spec)
    }

    /// Convert RGBA to grayscale using ITU-R BT.709 luma coefficients
    fn to_grayscale(&self, rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4)
            .map(|pixel| {
                let r = pixel[0] as u32;
                let g = pixel[1] as u32;
                let b = pixel[2] as u32;
                let a = pixel[3] as u32;

                // If fully transparent, treat as white (e-ink background)
                if a == 0 {
                    return 255;
                }

                // ITU-R BT.709 luma: Y = 0.2126*R + 0.7152*G + 0.0722*B
                // Using integer math: Y = (2126*R + 7152*G + 722*B) / 10000
                let luma = (2126 * r + 7152 * g + 722 * b) / 10000;

                // Alpha compositing against white background
                let white = 255u32;
                let composited = (luma * a + white * (255 - a)) / 255;

                composited as u8
            })
            .collect()
    }

    /// Encode grayscale data to 2-bit native grayscale PNG for e-ink displays.
    ///
    /// Uses PNG color type 0 (Grayscale) with 2-bit depth for optimal firmware
    /// decoding:
    /// - PNG 0 → black
    /// - PNG 1 → dark gray
    /// - PNG 2 → light gray
    /// - PNG 3 → white
    fn encode_png(&self, gray_data: &[u8], spec: DisplaySpec) -> Result<Vec<u8>, RenderError> {
        let mut buf = Cursor::new(Vec::new());

        {
            // Create PNG encoder with 2-bit native grayscale (not indexed)
            // This allows the TRMNL firmware to use the optimized direct path
            // instead of going through ReduceBpp with palette lookup
            let mut encoder = png::Encoder::new(&mut buf, spec.width, spec.height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Two);
            // Use maximum compression to reduce file size for memory-constrained devices
            encoder.set_compression(png::Compression::Best);
            encoder.set_filter(png::FilterType::Paeth);

            let mut writer = encoder
                .write_header()
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;

            // Convert 8-bit grayscale to 2-bit, packing 4 pixels per byte
            let packed_data = self.pack_2bit(gray_data, spec.width);

            writer
                .write_image_data(&packed_data)
                .map_err(|e| RenderError::PngEncode(e.to_string()))?;
        }

        let png_bytes = buf.into_inner();

        // Validate size
        spec.validate_size(png_bytes.len())?;

        Ok(png_bytes)
    }

    /// Pack 8-bit grayscale pixels into 2-bit grayscale format (4 pixels per byte)
    ///
    /// Maps grayscale values directly to 2-bit values:
    /// - Input 0 (black)   → PNG 0 (black)
    /// - Input 85 (dark)   → PNG 1 (dark gray)
    /// - Input 170 (light) → PNG 2 (light gray)
    /// - Input 255 (white) → PNG 3 (white)
    fn pack_2bit(&self, gray_data: &[u8], width: u32) -> Vec<u8> {
        // Each row needs to be byte-aligned
        let bytes_per_row = (width as usize).div_ceil(4);
        let height = gray_data.len() / width as usize;

        let mut packed = Vec::with_capacity(bytes_per_row * height);

        for row in gray_data.chunks(width as usize) {
            let mut byte = 0u8;
            let mut bit_pos = 6; // Start from high bits (2 bits per pixel)

            for (i, &pixel) in row.iter().enumerate() {
                // Map 8-bit grayscale to 2-bit value (0-3)
                // After dithering, pixels should already be quantized to 4 levels
                let value = match pixel {
                    0..=63 => 0,    // black
                    64..=127 => 1,  // dark gray
                    128..=191 => 2, // light gray
                    192..=255 => 3, // white
                };

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
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self::new()
    }
}
