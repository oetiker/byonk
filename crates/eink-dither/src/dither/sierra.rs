//! Sierra family error diffusion dithering algorithms.
//!
//! The Sierra family includes three variants offering different speed/quality
//! tradeoffs: Sierra (full), Sierra Two-Row, and Sierra Lite.

use crate::color::LinearRgb;
use crate::palette::Palette;

use super::{dither_with_kernel, Dither, DitherOptions, SIERRA, SIERRA_LITE, SIERRA_TWO_ROW};

/// Sierra (full) error diffusion dithering.
///
/// Also known as Sierra-3, this algorithm distributes 100% of quantization
/// error to 10 neighbors over 3 rows. Similar to JJN but with different
/// coefficients.
///
/// # Algorithm
///
/// The Sierra kernel distributes error to 10 neighbors:
///
/// ```text
///            X   5   3
///    2   4   5   4   2
///        2   3   2
/// ```
///
/// Total: 32/32 = 100% error propagation.
///
/// # Characteristics
///
/// - **High quality**: Large kernel for smooth results
/// - **Max dy = 2**: Requires 3-row error buffer
/// - **10 neighbors**: Less than JJN's 12 but similar quality
///
/// # Example
///
/// ```ignore
/// use eink_dither::{Sierra, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = Sierra.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct Sierra;

impl Dither for Sierra {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel(image, width, height, palette, &SIERRA, options)
    }
}

/// Sierra Two-Row error diffusion dithering.
///
/// Also known as Sierra-2, this is a faster variant of Sierra that uses
/// only 2 rows instead of 3, distributing to 7 neighbors.
///
/// # Algorithm
///
/// The Sierra Two-Row kernel distributes error to 7 neighbors:
///
/// ```text
///            X   4   3
///    1   2   3   2   1
/// ```
///
/// Total: 16/16 = 100% error propagation.
///
/// # Characteristics
///
/// - **Faster than full Sierra**: Only 2 rows, 7 neighbors
/// - **Max dy = 1**: Requires only 2-row error buffer
/// - **Good quality/speed balance**: Faster than JJN/Sierra, better than Floyd-Steinberg
///
/// # Example
///
/// ```ignore
/// use eink_dither::{SierraTwoRow, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = SierraTwoRow.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct SierraTwoRow;

impl Dither for SierraTwoRow {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel(image, width, height, palette, &SIERRA_TWO_ROW, options)
    }
}

/// Sierra Lite error diffusion dithering.
///
/// The fastest Sierra variant, using a minimal 2x2 kernel with only 3
/// neighbors. Good for speed-critical applications.
///
/// # Algorithm
///
/// The Sierra Lite kernel distributes error to 3 neighbors:
///
/// ```text
///    X   2
///    1   1
/// ```
///
/// Total: 4/4 = 100% error propagation.
///
/// # Characteristics
///
/// - **Fastest Sierra variant**: Only 3 neighbors
/// - **Max dy = 1**: Requires only 2-row error buffer
/// - **Acceptable quality**: Similar to Floyd-Steinberg
///
/// # When to Use
///
/// - When speed is critical
/// - For real-time or interactive applications
/// - When quality differences aren't important
///
/// # Example
///
/// ```ignore
/// use eink_dither::{SierraLite, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = SierraLite.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct SierraLite;

impl Dither for SierraLite {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel(image, width, height, palette, &SIERRA_LITE, options)
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

    // ========================================================================
    // Sierra (full) tests
    // ========================================================================

    #[test]
    fn test_sierra_basic() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // 4x4 mid-gray image
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 16];

        let result = Sierra.dither(&image, 4, 4, &palette, &options);

        assert_eq!(result.len(), 16);
        let black_count = result.iter().filter(|&&x| x == 0).count();
        let white_count = result.iter().filter(|&&x| x == 1).count();
        assert!(black_count > 0 && white_count > 0);
    }

    #[test]
    fn test_sierra_10_entries() {
        // Sierra has 10 kernel entries and divisor=32
        assert_eq!(SIERRA.entries.len(), 10);
        assert_eq!(SIERRA.divisor, 32);
        assert_eq!(SIERRA.max_dy, 2);
    }

    #[test]
    fn test_sierra_high_quality() {
        // Sierra produces 100% error propagation like Floyd-Steinberg
        let palette = create_test_palette();
        let options = DitherOptions::new().serpentine(false);

        let width = 10;
        let height = 10;
        let gray_value = 0.35_f32;
        let image: Vec<LinearRgb> = (0..width * height)
            .map(|_| LinearRgb::new(gray_value, gray_value, gray_value))
            .collect();

        let result = Sierra.dither(&image, width, height, &palette, &options);

        let white_count = result.iter().filter(|&&x| x == 1).count();
        let white_ratio = white_count as f32 / (width * height) as f32;

        assert!(
            (white_ratio - gray_value).abs() < 0.15,
            "Expected ~{} white ratio, got {}",
            gray_value,
            white_ratio
        );
    }

    #[test]
    fn test_sierra_exact_black_white() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let white = LinearRgb::new(1.0, 1.0, 1.0);

        let result_black = Sierra.dither(&vec![black; 16], 4, 4, &palette, &options);
        let result_white = Sierra.dither(&vec![white; 16], 4, 4, &palette, &options);

        assert!(result_black.iter().all(|&x| x == 0));
        assert!(result_white.iter().all(|&x| x == 1));
    }

    // ========================================================================
    // Sierra Two-Row tests
    // ========================================================================

    #[test]
    fn test_sierra_two_row_basic() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 16];

        let result = SierraTwoRow.dither(&image, 4, 4, &palette, &options);

        assert_eq!(result.len(), 16);
        let black_count = result.iter().filter(|&&x| x == 0).count();
        let white_count = result.iter().filter(|&&x| x == 1).count();
        assert!(black_count > 0 && white_count > 0);
    }

    #[test]
    fn test_sierra_two_row_max_dy_1() {
        // Sierra Two-Row only reaches 1 row ahead
        assert_eq!(SIERRA_TWO_ROW.max_dy, 1);
        assert_eq!(SIERRA_TWO_ROW.entries.len(), 7);
        assert_eq!(SIERRA_TWO_ROW.divisor, 16);
    }

    #[test]
    fn test_sierra_two_row_faster_variant() {
        // Verify it works with smaller images (only needs 2-row buffer)
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 4]; // 2x2 image

        let result = SierraTwoRow.dither(&image, 2, 2, &palette, &options);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_sierra_two_row_exact_colors() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let white = LinearRgb::new(1.0, 1.0, 1.0);

        let result_black = SierraTwoRow.dither(&vec![black; 16], 4, 4, &palette, &options);
        let result_white = SierraTwoRow.dither(&vec![white; 16], 4, 4, &palette, &options);

        assert!(result_black.iter().all(|&x| x == 0));
        assert!(result_white.iter().all(|&x| x == 1));
    }

    // ========================================================================
    // Sierra Lite tests
    // ========================================================================

    #[test]
    fn test_sierra_lite_basic() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 16];

        let result = SierraLite.dither(&image, 4, 4, &palette, &options);

        assert_eq!(result.len(), 16);
        let black_count = result.iter().filter(|&&x| x == 0).count();
        let white_count = result.iter().filter(|&&x| x == 1).count();
        assert!(black_count > 0 && white_count > 0);
    }

    #[test]
    fn test_sierra_lite_3_entries() {
        // Sierra Lite is the minimal variant with only 3 entries
        assert_eq!(SIERRA_LITE.entries.len(), 3);
        assert_eq!(SIERRA_LITE.divisor, 4);
        assert_eq!(SIERRA_LITE.max_dy, 1);
    }

    #[test]
    fn test_sierra_lite_fastest_variant() {
        // Sierra Lite should work on tiny images
        let palette = create_test_palette();
        let options = DitherOptions::new();

        // Even 1x1 should work
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let result = SierraLite.dither(&[gray], 1, 1, &palette, &options);
        assert_eq!(result.len(), 1);

        // 2x2 should work
        let result = SierraLite.dither(&vec![gray; 4], 2, 2, &palette, &options);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_sierra_lite_exact_colors() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let white = LinearRgb::new(1.0, 1.0, 1.0);

        let result_black = SierraLite.dither(&vec![black; 16], 4, 4, &palette, &options);
        let result_white = SierraLite.dither(&vec![white; 16], 4, 4, &palette, &options);

        assert!(result_black.iter().all(|&x| x == 0));
        assert!(result_white.iter().all(|&x| x == 1));
    }

    // ========================================================================
    // Cross-variant comparison tests
    // ========================================================================

    #[test]
    fn test_sierra_family_all_valid_output() {
        let palette = create_test_palette();
        let options = DitherOptions::new();

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 64]; // 8x8

        let result_full = Sierra.dither(&image, 8, 8, &palette, &options);
        let result_two = SierraTwoRow.dither(&image, 8, 8, &palette, &options);
        let result_lite = SierraLite.dither(&image, 8, 8, &palette, &options);

        // All should produce valid output
        assert_eq!(result_full.len(), 64);
        assert_eq!(result_two.len(), 64);
        assert_eq!(result_lite.len(), 64);

        // All values should be valid indices
        assert!(result_full.iter().all(|&x| x <= 1));
        assert!(result_two.iter().all(|&x| x <= 1));
        assert!(result_lite.iter().all(|&x| x <= 1));
    }

    #[test]
    fn test_sierra_preserves_exact_matches() {
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(128, 128, 128),
            Srgb::from_u8(255, 255, 255),
        ];
        let palette = Palette::new(&colors, None).unwrap();
        let options = DitherOptions::new().preserve_exact_matches(true);

        let exact_gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![exact_gray; 16];

        // All Sierra variants should preserve exact matches
        let result_full = Sierra.dither(&image, 4, 4, &palette, &options);
        let result_two = SierraTwoRow.dither(&image, 4, 4, &palette, &options);
        let result_lite = SierraLite.dither(&image, 4, 4, &palette, &options);

        assert!(result_full.iter().all(|&x| x == 1));
        assert!(result_two.iter().all(|&x| x == 1));
        assert!(result_lite.iter().all(|&x| x == 1));
    }
}
