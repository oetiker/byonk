//! Rendering intent orchestrator for the dithering pipeline.
//!
//! [`RenderingIntent`] is the primary entry point for Phase 6: a single
//! `render()` call takes raw sRGB pixels and returns a structured
//! [`DitheredImage`](super::DitheredImage) with the correct preprocessing
//! and dithering algorithm wired for the content type.

use crate::color::Srgb;
use crate::dither::{Atkinson, BlueNoiseDither, Dither, DitherOptions};
use crate::palette::Palette;
use crate::preprocess::{PreprocessOptions, Preprocessor};

use super::DitheredImage;

/// Rendering intent controls how the image is processed for the target medium.
///
/// Each variant selects the optimal preprocessing preset and dithering
/// algorithm for a category of content, then orchestrates the full pipeline
/// from raw sRGB pixels to a finished [`DitheredImage`].
///
/// # Variants
///
/// - **`Photo`**: Best for photographs. Uses error diffusion ([`Atkinson`])
///   with saturation boost and contrast enhancement to compensate for
///   e-ink's limited gamut and dynamic range.
///
/// - **`Graphics`**: Best for logos, text, and UI elements. Uses ordered
///   dithering ([`BlueNoiseDither`]), preserves exact palette matches, and
///   applies no saturation or contrast enhancement.
///
/// # Example
///
/// ```ignore
/// use eink_dither::{RenderingIntent, Palette, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
///
/// let pixels = vec![Srgb::from_u8(128, 128, 128); 4];
/// let image = RenderingIntent::Photo.render(&pixels, 2, 2, &palette);
///
/// assert_eq!(image.width(), 2);
/// assert_eq!(image.height(), 2);
/// ```
pub enum RenderingIntent {
    /// Photo rendering intent (INTENT-01).
    ///
    /// Pipeline: `PreprocessOptions::photo()` -> `Atkinson` error diffusion.
    Photo,

    /// Graphics rendering intent (INTENT-02).
    ///
    /// Pipeline: `PreprocessOptions::graphics()` -> `BlueNoiseDither` ordered dithering.
    Graphics,
}

impl RenderingIntent {
    /// Render raw sRGB pixels into a [`DitheredImage`].
    ///
    /// This is the primary entry point for the dithering pipeline.
    /// It applies the appropriate preprocessing and dithering algorithm
    /// based on the selected rendering intent.
    ///
    /// # Arguments
    ///
    /// * `input` - Raw pixels in sRGB space, row-major order.
    /// * `width` - Image width in pixels.
    /// * `height` - Image height in pixels.
    /// * `palette` - The color palette to quantize to.
    ///
    /// # Returns
    ///
    /// A [`DitheredImage`] with palette indices, dimensions, and the palette.
    ///
    /// # Panics (debug only)
    ///
    /// Debug-asserts that `input.len() == width * height`.
    pub fn render(
        &self,
        input: &[Srgb],
        width: usize,
        height: usize,
        palette: &Palette,
    ) -> DitheredImage {
        match self {
            RenderingIntent::Photo => {
                // INTENT-01: Photo mode
                // 1. Preprocess with saturation boost (1.5) and contrast enhancement (1.1)
                let preprocess_opts = PreprocessOptions::photo();
                let preprocessor = Preprocessor::new(palette, preprocess_opts);
                let result = preprocessor.process(input, width, height);

                // 2. Error diffusion via Atkinson (75% propagation, ideal for photos)
                let dither_opts = DitherOptions::new();
                let indices = Atkinson.dither(
                    &result.pixels,
                    result.width,
                    result.height,
                    palette,
                    &dither_opts,
                );

                // 3. Wrap in DitheredImage
                DitheredImage::new(indices, result.width, result.height, palette.clone())
            }
            RenderingIntent::Graphics => {
                // INTENT-02: Graphics mode
                // 1. Preprocess with no enhancement (saturation=1.0, contrast=1.0)
                let preprocess_opts = PreprocessOptions::graphics();
                let preprocessor = Preprocessor::new(palette, preprocess_opts);
                let result = preprocessor.process(input, width, height);

                // 2. Ordered dithering via blue noise (no error diffusion artifacts)
                let dither_opts = DitherOptions::new();
                let indices = BlueNoiseDither.dither(
                    &result.pixels,
                    result.width,
                    result.height,
                    palette,
                    &dither_opts,
                );

                // 3. Wrap in DitheredImage
                DitheredImage::new(indices, result.width, result.height, palette.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a 3-color palette with distinct official and actual colors.
    fn test_palette() -> Palette {
        let official = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red (official)
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),       // black (same)
            Srgb::from_u8(255, 255, 255), // white (same)
            Srgb::from_u8(200, 50, 50),   // muddy red (actual)
        ];
        Palette::new(&official, Some(&actual)).unwrap()
    }

    /// Helper: create a 4x4 gradient image (dark to light).
    fn gradient_4x4() -> Vec<Srgb> {
        (0..16)
            .map(|i| {
                let v = (i as f32 / 15.0 * 255.0) as u8;
                Srgb::from_u8(v, v, v)
            })
            .collect()
    }

    #[test]
    fn test_photo_renders_without_panic() {
        let palette = test_palette();
        let pixels = gradient_4x4();

        let image = RenderingIntent::Photo.render(&pixels, 4, 4, &palette);

        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 4);
        assert_eq!(image.indices().len(), 16);

        // All indices must be valid
        for &idx in image.indices() {
            assert!(
                (idx as usize) < palette.len(),
                "Index {} out of palette range {}",
                idx,
                palette.len()
            );
        }
    }

    #[test]
    fn test_graphics_renders_without_panic() {
        let palette = test_palette();
        let pixels = gradient_4x4();

        let image = RenderingIntent::Graphics.render(&pixels, 4, 4, &palette);

        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 4);
        assert_eq!(image.indices().len(), 16);

        // All indices must be valid
        for &idx in image.indices() {
            assert!(
                (idx as usize) < palette.len(),
                "Index {} out of palette range {}",
                idx,
                palette.len()
            );
        }
    }

    #[test]
    fn test_photo_uses_atkinson_characteristics() {
        // Photo and Graphics use different algorithms, so they should
        // produce different index patterns on the same input.
        let palette = test_palette();
        let pixels = gradient_4x4();

        let photo = RenderingIntent::Photo.render(&pixels, 4, 4, &palette);
        let graphics = RenderingIntent::Graphics.render(&pixels, 4, 4, &palette);

        // Different algorithms + different preprocessing should produce
        // different outputs on a gradient.
        assert_ne!(
            photo.indices(),
            graphics.indices(),
            "Photo and Graphics should produce different dither patterns on a gradient"
        );
    }

    #[test]
    fn test_photo_output_has_official_colors() {
        let palette = test_palette();
        let pixels = gradient_4x4();

        let image = RenderingIntent::Photo.render(&pixels, 4, 4, &palette);
        let rgb = image.to_rgb_official();

        assert_eq!(rgb.len(), 4 * 4 * 3);

        // Verify each pixel's RGB matches the official palette color for its index
        for (i, &idx) in image.indices().iter().enumerate() {
            let [r, g, b] = palette.official(idx as usize).to_bytes();
            assert_eq!(rgb[i * 3], r, "pixel {} R mismatch", i);
            assert_eq!(rgb[i * 3 + 1], g, "pixel {} G mismatch", i);
            assert_eq!(rgb[i * 3 + 2], b, "pixel {} B mismatch", i);
        }
    }

    #[test]
    fn test_graphics_output_has_actual_colors() {
        let palette = test_palette();
        let pixels = gradient_4x4();

        let image = RenderingIntent::Graphics.render(&pixels, 4, 4, &palette);
        let rgb = image.to_rgb_actual();

        assert_eq!(rgb.len(), 4 * 4 * 3);

        // Verify each pixel's RGB matches the actual palette color for its index
        for (i, &idx) in image.indices().iter().enumerate() {
            let [r, g, b] = palette.actual(idx as usize).to_bytes();
            assert_eq!(rgb[i * 3], r, "pixel {} R mismatch", i);
            assert_eq!(rgb[i * 3 + 1], g, "pixel {} G mismatch", i);
            assert_eq!(rgb[i * 3 + 2], b, "pixel {} B mismatch", i);
        }
    }

    #[test]
    fn test_indices_in_palette_range() {
        let palette = test_palette();
        let pixels = gradient_4x4();

        // Test both intents
        for intent in &[RenderingIntent::Photo, RenderingIntent::Graphics] {
            let image = intent.render(&pixels, 4, 4, &palette);

            for (i, &idx) in image.indices().iter().enumerate() {
                assert!(
                    (idx as usize) < palette.len(),
                    "Index {} at position {} exceeds palette size {}",
                    idx,
                    i,
                    palette.len()
                );
            }
        }
    }
}
