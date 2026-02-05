//! Floyd-Steinberg error diffusion dithering algorithm.
//!
//! Floyd-Steinberg is the most widely known error diffusion algorithm.
//! It distributes 100% of the quantization error to 4 neighbors, producing
//! smooth gradients but potentially more color bleeding than Atkinson
//! with small palettes.

use crate::color::LinearRgb;
use crate::palette::Palette;

use super::{dither_with_kernel, Dither, DitherOptions, FLOYD_STEINBERG};

/// Floyd-Steinberg error diffusion dithering.
///
/// The classic error diffusion algorithm, distributing 100% of quantization
/// error to 4 neighboring pixels. While producing smooth results, it can
/// cause more color bleeding than Atkinson with small palettes (7-16 colors).
///
/// # Algorithm
///
/// The Floyd-Steinberg kernel distributes error to 4 neighbors:
///
/// ```text
///        X   7
///    3   5   1
/// ```
///
/// Weights: 7/16 right, 3/16 bottom-left, 5/16 bottom, 1/16 bottom-right.
/// Total: 16/16 = 100% error propagation.
///
/// # When to Use
///
/// - When you need full error preservation (e.g., larger palettes)
/// - For compatibility with classic dithering behavior
/// - When Atkinson produces too much contrast
///
/// For small e-ink palettes (7-16 colors), consider [`Atkinson`](super::Atkinson)
/// which propagates only 75% of error to reduce bleeding.
///
/// # Example
///
/// ```ignore
/// use eink_dither::{FloydSteinberg, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = FloydSteinberg.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct FloydSteinberg;

impl Dither for FloydSteinberg {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel(image, width, height, palette, &FLOYD_STEINBERG, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Srgb;

    fn create_test_palette() -> Palette {
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        Palette::new(&colors, None).unwrap()
    }

    #[test]
    fn test_floyd_steinberg_basic() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // 2x2 mid-gray image
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 4];

        let result = FloydSteinberg.dither(&image, 2, 2, &palette, &options);

        assert_eq!(result.len(), 4);
        // Should produce a mix of black (0) and white (1)
        let black_count = result.iter().filter(|&&x| x == 0).count();
        let white_count = result.iter().filter(|&&x| x == 1).count();
        // Mid-gray should produce roughly equal mix
        assert!(black_count > 0 && white_count > 0);
    }

    #[test]
    fn test_floyd_steinberg_100_percent_propagation() {
        // Floyd-Steinberg propagates 100% of error, which can be verified by
        // checking that the average brightness of output approximately matches input
        let palette = create_test_palette();
        let options = DitherOptions::new().serpentine(false);

        // Create a larger gradient image for statistical significance
        let width = 10;
        let height = 10;
        let gray_value = 0.3_f32; // 30% brightness
        let image: Vec<LinearRgb> = (0..width * height)
            .map(|_| LinearRgb::new(gray_value, gray_value, gray_value))
            .collect();

        let result = FloydSteinberg.dither(&image, width, height, &palette, &options);

        // Count white pixels (index 1)
        let white_count = result.iter().filter(|&&x| x == 1).count();
        let white_ratio = white_count as f32 / (width * height) as f32;

        // With 100% error propagation, output should roughly match input brightness
        // Allow generous tolerance due to small size
        assert!(
            (white_ratio - gray_value).abs() < 0.15,
            "Expected ~{} white ratio, got {}",
            gray_value,
            white_ratio
        );
    }

    #[test]
    fn test_floyd_steinberg_exact_black() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // Pure black should output all black
        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let image = vec![black; 4];

        let result = FloydSteinberg.dither(&image, 2, 2, &palette, &options);
        assert!(result.iter().all(|&x| x == 0), "Pure black should all be 0");
    }

    #[test]
    fn test_floyd_steinberg_exact_white() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // Pure white should output all white
        let white = LinearRgb::new(1.0, 1.0, 1.0);
        let image = vec![white; 4];

        let result = FloydSteinberg.dither(&image, 2, 2, &palette, &options);
        assert!(result.iter().all(|&x| x == 1), "Pure white should all be 1");
    }

    #[test]
    fn test_floyd_steinberg_preserves_exact_matches() {
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(128, 128, 128),
            Srgb::from_u8(255, 255, 255),
        ];
        let palette = Palette::new(&colors, None).unwrap();
        let options = DitherOptions::new().preserve_exact_matches(true);

        // Image with exact palette color
        let exact_gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![exact_gray; 4];

        let result = FloydSteinberg.dither(&image, 2, 2, &palette, &options);
        // All should map to gray (index 1)
        assert!(
            result.iter().all(|&x| x == 1),
            "Exact gray matches should preserve"
        );
    }

    #[test]
    fn test_floyd_steinberg_serpentine() {
        let palette = create_test_palette();

        // Compare serpentine vs non-serpentine on same input
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 16]; // 4x4 image

        let result_serp = FloydSteinberg.dither(
            &image,
            4,
            4,
            &palette,
            &DitherOptions::new().serpentine(true),
        );
        let result_flat = FloydSteinberg.dither(
            &image,
            4,
            4,
            &palette,
            &DitherOptions::new().serpentine(false),
        );

        // Both should produce valid output
        assert_eq!(result_serp.len(), 16);
        assert_eq!(result_flat.len(), 16);

        // Results may differ due to serpentine processing
        // (no assertion on equality - they're expected to differ)
    }
}
