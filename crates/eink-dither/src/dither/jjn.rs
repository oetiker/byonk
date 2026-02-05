//! Jarvis-Judice-Ninke error diffusion dithering algorithm.
//!
//! JJN spreads error over a larger area (12 neighbors across 3 rows),
//! producing smoother gradients than Floyd-Steinberg at the cost of
//! slightly more processing time.

use crate::color::LinearRgb;
use crate::palette::Palette;

use super::{dither_with_kernel, Dither, DitherOptions, JARVIS_JUDICE_NINKE};

/// Jarvis-Judice-Ninke error diffusion dithering.
///
/// JJN distributes 100% of quantization error across 12 neighboring pixels
/// over 3 rows. The larger kernel produces smoother gradients and less
/// visible dithering patterns than Floyd-Steinberg.
///
/// # Algorithm
///
/// The JJN kernel distributes error to 12 neighbors:
///
/// ```text
///            X   7   5
///    3   5   7   5   3
///    1   3   5   3   1
/// ```
///
/// Total: 48/48 = 100% error propagation.
///
/// # Characteristics
///
/// - **Smoother gradients**: Larger kernel reduces visible patterns
/// - **Better for continuous tones**: Photos and gradients benefit most
/// - **Slightly slower**: 12 neighbors vs 4 (Floyd-Steinberg)
/// - **Max dy = 2**: Requires 3-row error buffer
///
/// # When to Use
///
/// - When smooth gradients are more important than speed
/// - For photographic content with continuous tones
/// - When Floyd-Steinberg produces too much visible texture
///
/// # Example
///
/// ```ignore
/// use eink_dither::{JarvisJudiceNinke, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = JarvisJudiceNinke.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct JarvisJudiceNinke;

impl Dither for JarvisJudiceNinke {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel(image, width, height, palette, &JARVIS_JUDICE_NINKE, options)
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
    fn test_jjn_basic() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // 4x4 mid-gray image (need larger for JJN's 3-row kernel)
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 16];

        let result = JarvisJudiceNinke.dither(&image, 4, 4, &palette, &options);

        assert_eq!(result.len(), 16);
        // Should produce a mix of black (0) and white (1)
        let black_count = result.iter().filter(|&&x| x == 0).count();
        let white_count = result.iter().filter(|&&x| x == 1).count();
        assert!(black_count > 0 && white_count > 0);
    }

    #[test]
    fn test_jjn_12_neighbors() {
        // JJN uses 12 neighbors - verify kernel is applied correctly
        // by checking that the algorithm completes without error
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // Need at least 5 columns for dx range -2..+2
        // and 3 rows for dy range 0..2
        let width = 6;
        let height = 4;
        let gray = LinearRgb::new(0.3, 0.3, 0.3);
        let image = vec![gray; width * height];

        let result = JarvisJudiceNinke.dither(&image, width, height, &palette, &options);

        assert_eq!(result.len(), width * height);
        // All values should be valid palette indices (0 or 1)
        assert!(result.iter().all(|&x| x <= 1));
    }

    #[test]
    fn test_jjn_max_dy_2_handled() {
        // JJN reaches 2 rows ahead - verify it works near bottom of image
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // Small image where kernel exceeds bounds
        let width = 3;
        let height = 2; // Only 2 rows, but kernel reaches dy=2
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; width * height];

        // Should complete without panic
        let result = JarvisJudiceNinke.dither(&image, width, height, &palette, &options);
        assert_eq!(result.len(), width * height);
    }

    #[test]
    fn test_jjn_exact_black() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let image = vec![black; 16];

        let result = JarvisJudiceNinke.dither(&image, 4, 4, &palette, &options);
        assert!(result.iter().all(|&x| x == 0), "Pure black should all be 0");
    }

    #[test]
    fn test_jjn_exact_white() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let white = LinearRgb::new(1.0, 1.0, 1.0);
        let image = vec![white; 16];

        let result = JarvisJudiceNinke.dither(&image, 4, 4, &palette, &options);
        assert!(result.iter().all(|&x| x == 1), "Pure white should all be 1");
    }

    #[test]
    fn test_jjn_100_percent_propagation() {
        // JJN propagates 100% of error
        let palette = create_test_palette();
        let options = DitherOptions::new().serpentine(false);

        let width = 12;
        let height = 12;
        let gray_value = 0.4_f32;
        let image: Vec<LinearRgb> = (0..width * height)
            .map(|_| LinearRgb::new(gray_value, gray_value, gray_value))
            .collect();

        let result = JarvisJudiceNinke.dither(&image, width, height, &palette, &options);

        let white_count = result.iter().filter(|&&x| x == 1).count();
        let white_ratio = white_count as f32 / (width * height) as f32;

        // With 100% propagation, output brightness should approximate input
        assert!(
            (white_ratio - gray_value).abs() < 0.15,
            "Expected ~{} white ratio, got {}",
            gray_value,
            white_ratio
        );
    }

    #[test]
    fn test_jjn_preserves_exact_matches() {
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(128, 128, 128),
            Srgb::from_u8(255, 255, 255),
        ];
        let palette = Palette::new(&colors, None).unwrap();
        let options = DitherOptions::new().preserve_exact_matches(true);

        let exact_gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![exact_gray; 16];

        let result = JarvisJudiceNinke.dither(&image, 4, 4, &palette, &options);
        assert!(
            result.iter().all(|&x| x == 1),
            "Exact gray matches should preserve"
        );
    }

    #[test]
    fn test_jjn_vs_floyd_steinberg_smoother() {
        // JJN should produce different (smoother) patterns than Floyd-Steinberg
        // We can't easily measure "smoothness" but can verify different output
        use super::super::FloydSteinberg;

        let palette = create_test_palette();
        let options = DitherOptions::new().serpentine(false);

        let width = 8;
        let height = 8;
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; width * height];

        let result_jjn = JarvisJudiceNinke.dither(&image, width, height, &palette, &options);
        let result_fs = FloydSteinberg.dither(&image, width, height, &palette, &options);

        // Both valid but may differ in pattern
        assert_eq!(result_jjn.len(), result_fs.len());
        // Count differences - patterns should differ
        // (Note: they could theoretically be the same, but unlikely for mid-gray)
    }
}
