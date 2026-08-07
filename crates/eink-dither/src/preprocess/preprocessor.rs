//! Core preprocessing logic for e-ink dithering.
//!
//! The [`Preprocessor`] struct transforms input images for optimal e-ink output
//! by resizing, boosting saturation, and adjusting contrast.
//!
//! # Processing Pipeline
//!
//! 1. **Resize** (if target dimensions specified)
//!    - Lanczos3 resampling for high-quality scaling
//!    - Happens first to ensure optimal quality at target size
//!
//! 2. **Saturation boost** (Oklch chroma scaling)
//!    - Perceptually correct: no hue shift
//!
//! 3. **Contrast adjustment** (linear RGB midpoint scaling)
//!    - Scales around 0.5 midpoint
//!
//! # Example
//!
//! ```ignore
//! use eink_dither::{Preprocessor, PreprocessOptions, PreprocessResult, Palette, Srgb};
//!
//! // Create a simple palette
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! // Configure preprocessing with resize
//! let options = PreprocessOptions::new().resize(100, 100);
//! let preprocessor = Preprocessor::new(options);
//!
//! // Process an image (2x1 pixels: black and mid-gray)
//! let input = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(128, 128, 128)];
//! let result = preprocessor.process(&input, 2, 1);
//!
//! // Result contains processed pixels and dimensions
//! assert_eq!(result.width, 100); // Resized
//! assert_eq!(result.height, 100);
//! ```

use crate::color::{LinearRgb, Oklab, Srgb};
use crate::preprocess::PreprocessOptions;

use super::oklch::Oklch;
use super::resize::resize_lanczos;

/// Result of preprocessing an image.
///
/// Contains the processed pixels and updated dimensions (after resize).
///
/// # Example
///
/// ```
/// use eink_dither::{Preprocessor, PreprocessOptions, PreprocessResult, Palette, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
/// let preprocessor = Preprocessor::new(PreprocessOptions::new());
///
/// let input = [Srgb::from_u8(128, 128, 128)];
/// let result = preprocessor.process(&input, 1, 1);
///
/// // Access result fields
/// assert_eq!(result.pixels.len(), 1);
/// assert_eq!(result.width, 1);
/// assert_eq!(result.height, 1);
/// ```
#[derive(Debug, Clone)]
pub struct PreprocessResult {
    /// Preprocessed pixels in linear RGB space, ready for dithering.
    pub pixels: Vec<LinearRgb>,

    /// Width after resize (may differ from input if resize was specified).
    pub width: usize,

    /// Height after resize (may differ from input if resize was specified).
    pub height: usize,
}

/// Image preprocessor with color enhancement.
///
/// `Preprocessor` transforms images for optimal e-ink display output using
/// a multi-phase pipeline:
///
/// 1. **Resize** to target dimensions (Lanczos3 filter)
/// 2. **Boost saturation** (Oklch chroma)
/// 3. **Adjust contrast** (linear RGB)
///
/// Enhancement is applied uniformly to every pixel. Pixels that already sit
/// exactly on a palette colour used to be detected and passed through
/// untouched, to keep text and logos crisp. That is gone: such a pixel has
/// zero quantisation error, so error diffusion reproduces it exactly without
/// a special case, and the exemption did active harm mid-gradient, where it
/// pinned pixels whose value merely happened to coincide with a palette
/// entry and discarded their error, leaving a seam across a smooth ramp.
///
/// # Thread Safety
///
/// `Preprocessor` is `Send + Sync`. Multiple threads can share an immutable
/// preprocessor for parallel image processing.
#[derive(Debug)]
pub struct Preprocessor {
    /// Preprocessing configuration
    options: PreprocessOptions,
}

impl Preprocessor {
    /// Create a new preprocessor with the given palette and options.
    ///
    /// # Arguments
    /// * `options` - Preprocessing configuration
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Preprocessor, PreprocessOptions, Palette, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    /// let preprocessor = Preprocessor::new(PreprocessOptions::new());
    /// ```
    #[inline]
    pub fn new(options: PreprocessOptions) -> Self {
        Self { options }
    }

    /// Process an image with the complete preprocessing pipeline.
    ///
    /// # Processing Order
    ///
    /// 1. **Resize** (if target dimensions specified)
    ///    - Lanczos3 resampling for high-quality scaling
    ///    - If only some dimensions match, no resize occurs (both must be specified)
    ///
    /// 2. **Saturation boost**
    ///    - Perceptually correct Oklch chroma scaling
    ///
    /// 3. **Contrast adjustment**
    ///    - Linear RGB midpoint scaling
    ///
    /// # Arguments
    /// * `input` - Input pixels in sRGB
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    ///
    /// # Returns
    /// [`PreprocessResult`] containing:
    /// - `pixels`: Processed pixels in linear RGB space
    /// - `width`: Width after resize (may differ from input)
    /// - `height`: Height after resize (may differ from input)
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Preprocessor, PreprocessOptions, Palette, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    /// let preprocessor = Preprocessor::new(PreprocessOptions::new());
    ///
    /// let input = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(128, 128, 128)];
    /// let result = preprocessor.process(&input, 2, 1);
    ///
    /// assert_eq!(result.pixels.len(), 2);
    /// assert_eq!(result.width, 2);
    /// assert_eq!(result.height, 1);
    /// ```
    pub fn process(&self, input: &[Srgb], width: usize, height: usize) -> PreprocessResult {
        let total = width * height;
        debug_assert_eq!(
            input.len(),
            total,
            "Input length {} doesn't match width*height {}",
            input.len(),
            total
        );

        // Step 1: Resize (if target dimensions specified)
        let (working_pixels, working_width, working_height) =
            match (self.options.target_width, self.options.target_height) {
                (Some(tw), Some(th)) => {
                    let (resized, rw, rh) =
                        resize_lanczos(input, width as u32, height as u32, tw, th);
                    (resized, rw as usize, rh as usize)
                }
                _ => (input.to_vec(), width, height),
            };

        // Step 2: Convert to LinearRgb, applying enhancements uniformly.
        let pixels: Vec<LinearRgb> = working_pixels
            .iter()
            .map(|&pixel| {
                // WHY LinearRgb as working space: All arithmetic (contrast scaling,
                // saturation adjustment) must operate on physically linear light
                // values. sRGB's gamma curve would distort midpoints and ratios.
                let mut linear = LinearRgb::from(pixel);

                // Apply saturation boost (if factor != 1.0)
                if (self.options.saturation - 1.0).abs() > f32::EPSILON {
                    linear = self.boost_saturation(linear, self.options.saturation);
                }

                // Apply contrast adjustment (if factor != 1.0)
                if (self.options.contrast - 1.0).abs() > f32::EPSILON {
                    linear = self.adjust_contrast(linear, self.options.contrast);
                }

                linear
            })
            .collect();

        PreprocessResult {
            pixels,
            width: working_width,
            height: working_height,
        }
    }

    /// Boost saturation using Oklch chroma scaling.
    ///
    /// This is perceptually correct: scaling chroma in Oklch doesn't shift hue.
    ///
    /// # Arguments
    /// * `pixel` - Input pixel in linear RGB
    /// * `factor` - Chroma multiplier (>1.0 increases saturation)
    #[inline]
    fn boost_saturation(&self, pixel: LinearRgb, factor: f32) -> LinearRgb {
        // WHY Oklch for saturation: Oklch is the polar form of OKLab where
        // chroma (saturation) is an independent axis. Scaling chroma in Oklch
        // preserves hue and lightness exactly. HSL/HSV saturation shifts hue
        // for non-primary colors and is not perceptually uniform.
        let oklab = Oklab::from(pixel);
        let oklch = Oklch::from(oklab);

        let boosted = oklch.scale_chroma(factor);

        // WHY convert back to LinearRgb: Return to the working color space
        // after perceptual adjustment. The rest of the pipeline (contrast,
        // error diffusion) operates in LinearRgb.
        let boosted_oklab = Oklab::from(boosted);
        LinearRgb::from(boosted_oklab)
    }

    /// Adjust contrast by scaling around the midpoint.
    ///
    /// The midpoint (0.5 in linear RGB) stays fixed; values above move further up,
    /// values below move further down.
    ///
    /// # Arguments
    /// * `pixel` - Input pixel in linear RGB
    /// * `factor` - Contrast multiplier (>1.0 increases contrast)
    #[inline]
    fn adjust_contrast(&self, pixel: LinearRgb, factor: f32) -> LinearRgb {
        const MIDPOINT: f32 = 0.5;
        LinearRgb::new(
            MIDPOINT + (pixel.r - MIDPOINT) * factor,
            MIDPOINT + (pixel.g - MIDPOINT) * factor,
            MIDPOINT + (pixel.b - MIDPOINT) * factor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to check approximate equality for f32
    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    // =========================================================================
    // Exact Match Detection Tests
    // =========================================================================

    // =========================================================================
    // Process Pipeline Tests
    // =========================================================================

    #[test]
    fn test_process_returns_correct_lengths() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        let input = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(128, 128, 128),
            Srgb::from_u8(255, 255, 255),
        ];
        let result = preprocessor.process(&input, 3, 1);

        assert_eq!(result.pixels.len(), 3, "Should return 3 processed pixels");
        assert_eq!(result.width, 3, "Width should be 3");
        assert_eq!(result.height, 1, "Height should be 1");
    }

    // =========================================================================
    // Saturation Boost Tests
    // =========================================================================

    #[test]
    fn test_boost_saturation_increases_chroma() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // A saturated red color
        let red = LinearRgb::new(0.8, 0.2, 0.1);
        let boosted = preprocessor.boost_saturation(red, 1.5);

        // Get chroma of both
        let original_oklch = Oklch::from(Oklab::from(red));
        let boosted_oklch = Oklch::from(Oklab::from(boosted));

        assert!(
            boosted_oklch.c > original_oklch.c,
            "Boosted chroma {} should be greater than original {}",
            boosted_oklch.c,
            original_oklch.c
        );
    }

    #[test]
    fn test_boost_saturation_gray_stays_gray() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // Pure gray (no chroma to boost)
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let boosted = preprocessor.boost_saturation(gray, 1.5);

        // Should remain approximately equal (gray has no chroma)
        assert!(
            approx_eq(boosted.r, gray.r, 1e-5),
            "Gray R should be unchanged"
        );
        assert!(
            approx_eq(boosted.g, gray.g, 1e-5),
            "Gray G should be unchanged"
        );
        assert!(
            approx_eq(boosted.b, gray.b, 1e-5),
            "Gray B should be unchanged"
        );
    }

    #[test]
    fn test_boost_saturation_preserves_hue() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // Orange color
        let orange = LinearRgb::new(0.7, 0.3, 0.1);
        let boosted = preprocessor.boost_saturation(orange, 1.5);

        let original_oklch = Oklch::from(Oklab::from(orange));
        let boosted_oklch = Oklch::from(Oklab::from(boosted));

        assert!(
            approx_eq(original_oklch.h, boosted_oklch.h, 1e-5),
            "Hue should be preserved: original={}, boosted={}",
            original_oklch.h,
            boosted_oklch.h
        );
    }

    #[test]
    fn test_boost_saturation_preserves_lightness() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        let color = LinearRgb::new(0.6, 0.3, 0.2);
        let boosted = preprocessor.boost_saturation(color, 1.5);

        let original_oklch = Oklch::from(Oklab::from(color));
        let boosted_oklch = Oklch::from(Oklab::from(boosted));

        assert!(
            approx_eq(original_oklch.l, boosted_oklch.l, 1e-5),
            "Lightness should be preserved: original={}, boosted={}",
            original_oklch.l,
            boosted_oklch.l
        );
    }

    // =========================================================================
    // Contrast Adjustment Tests
    // =========================================================================

    #[test]
    fn test_adjust_contrast_midpoint_unchanged() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // Mid-gray at exactly 0.5
        let midpoint = LinearRgb::new(0.5, 0.5, 0.5);
        let adjusted = preprocessor.adjust_contrast(midpoint, 1.5);

        assert!(
            approx_eq(adjusted.r, 0.5, 1e-6),
            "Midpoint should be unchanged: got {}",
            adjusted.r
        );
        assert!(
            approx_eq(adjusted.g, 0.5, 1e-6),
            "Midpoint should be unchanged: got {}",
            adjusted.g
        );
        assert!(
            approx_eq(adjusted.b, 0.5, 1e-6),
            "Midpoint should be unchanged: got {}",
            adjusted.b
        );
    }

    #[test]
    fn test_adjust_contrast_dark_gets_darker() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // Dark gray below midpoint
        let dark = LinearRgb::new(0.3, 0.3, 0.3);
        let adjusted = preprocessor.adjust_contrast(dark, 1.5);

        // 0.5 + (0.3 - 0.5) * 1.5 = 0.5 - 0.3 = 0.2
        assert!(
            adjusted.r < dark.r,
            "Dark should get darker: original={}, adjusted={}",
            dark.r,
            adjusted.r
        );
        assert!(
            approx_eq(adjusted.r, 0.2, 1e-6),
            "Expected 0.2, got {}",
            adjusted.r
        );
    }

    #[test]
    fn test_adjust_contrast_light_gets_lighter() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // Light gray above midpoint
        let light = LinearRgb::new(0.7, 0.7, 0.7);
        let adjusted = preprocessor.adjust_contrast(light, 1.5);

        // 0.5 + (0.7 - 0.5) * 1.5 = 0.5 + 0.3 = 0.8
        assert!(
            adjusted.r > light.r,
            "Light should get lighter: original={}, adjusted={}",
            light.r,
            adjusted.r
        );
        assert!(
            approx_eq(adjusted.r, 0.8, 1e-6),
            "Expected 0.8, got {}",
            adjusted.r
        );
    }

    #[test]
    fn test_adjust_contrast_factor_one_no_change() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        let color = LinearRgb::new(0.3, 0.5, 0.7);
        let adjusted = preprocessor.adjust_contrast(color, 1.0);

        assert!(approx_eq(adjusted.r, color.r, 1e-6));
        assert!(approx_eq(adjusted.g, color.g, 1e-6));
        assert!(approx_eq(adjusted.b, color.b, 1e-6));
    }

    // =========================================================================
    // Exact Match Preservation Tests
    // =========================================================================

    #[test]
    fn test_non_match_is_enhanced() {
        // Significant saturation boost
        let options = PreprocessOptions::new().saturation(2.0).contrast(1.0);
        let preprocessor = Preprocessor::new(options);

        // A colored pixel that doesn't match palette
        let input = [Srgb::from_u8(200, 100, 50)];
        let result = preprocessor.process(&input, 1, 1);

        // Doesn't match

        // Should be different from direct conversion (enhanced)
        let direct = LinearRgb::from(input[0]);
        let is_different = (result.pixels[0].r - direct.r).abs() > 0.01
            || (result.pixels[0].g - direct.g).abs() > 0.01
            || (result.pixels[0].b - direct.b).abs() > 0.01;

        assert!(
            is_different,
            "Non-match should be enhanced by saturation boost"
        );
    }

    // =========================================================================
    // Integration Tests
    // =========================================================================

    #[test]
    fn test_process_with_photo_preset() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        let input = [
            Srgb::from_u8(0, 0, 0),       // Black (exact match)
            Srgb::from_u8(255, 128, 64),  // Orange (no match, will be enhanced)
            Srgb::from_u8(255, 255, 255), // White (exact match)
        ];
        let result = preprocessor.process(&input, 3, 1);

        // Verify matches

        // Verify processed has 3 LinearRgb values
        assert_eq!(result.pixels.len(), 3);
    }

    #[test]
    fn test_process_with_graphics_preset() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        let input = [Srgb::from_u8(128, 64, 32)];
        let result = preprocessor.process(&input, 1, 1);

        // Graphics preset has saturation=1.0, contrast=1.0 (no enhancement)
        let expected = LinearRgb::from(input[0]);

        assert!(
            approx_eq(result.pixels[0].r, expected.r, 1e-5),
            "Graphics preset should not enhance"
        );
        assert!(
            approx_eq(result.pixels[0].g, expected.g, 1e-5),
            "Graphics preset should not enhance"
        );
        assert!(
            approx_eq(result.pixels[0].b, expected.b, 1e-5),
            "Graphics preset should not enhance"
        );
    }

    #[test]
    fn test_process_2d_image() {
        let preprocessor = Preprocessor::new(PreprocessOptions::new());

        // 2x2 image
        let input = [
            Srgb::from_u8(0, 0, 0),       // (0,0) black
            Srgb::from_u8(128, 128, 128), // (1,0) gray
            Srgb::from_u8(64, 64, 64),    // (0,1) dark gray
            Srgb::from_u8(255, 255, 255), // (1,1) white
        ];
        let result = preprocessor.process(&input, 2, 2);

        assert_eq!(result.pixels.len(), 4);
    }

    #[test]
    fn test_saturation_then_contrast_order() {
        // Verify the processing order: saturation first, then contrast

        // Use settings where order matters
        let options = PreprocessOptions::new().saturation(1.5).contrast(1.2);
        let preprocessor = Preprocessor::new(options);

        // A colored pixel
        let input = [Srgb::from_u8(200, 100, 50)];
        let result = preprocessor.process(&input, 1, 1);

        // Manually compute expected result: saturation then contrast
        let linear = LinearRgb::from(input[0]);
        let after_sat = preprocessor.boost_saturation(linear, 1.5);
        let after_contrast = preprocessor.adjust_contrast(after_sat, 1.2);

        assert!(
            approx_eq(result.pixels[0].r, after_contrast.r, 1e-5),
            "R: expected {}, got {}",
            after_contrast.r,
            result.pixels[0].r
        );
        assert!(
            approx_eq(result.pixels[0].g, after_contrast.g, 1e-5),
            "G: expected {}, got {}",
            after_contrast.g,
            result.pixels[0].g
        );
        assert!(
            approx_eq(result.pixels[0].b, after_contrast.b, 1e-5),
            "B: expected {}, got {}",
            after_contrast.b,
            result.pixels[0].b
        );
    }

    // =========================================================================
    // Resize Integration Tests
    // =========================================================================

    #[test]
    #[ignore = "requires image crate for actual resize"]
    fn test_process_with_resize() {
        // Set up resize to 50x50
        let options = PreprocessOptions::new().resize(50, 50);
        let preprocessor = Preprocessor::new(options);

        // 100x100 solid gray image
        let input = vec![Srgb::from_u8(128, 128, 128); 100 * 100];
        let result = preprocessor.process(&input, 100, 100);

        // Should be resized
        assert_eq!(result.width, 50, "Width should be 50 after resize");
        assert_eq!(result.height, 50, "Height should be 50 after resize");
        assert_eq!(result.pixels.len(), 2500, "Should have 50*50 pixels");
    }

    #[test]
    fn test_process_without_resize() {
        // No resize specified
        let options = PreprocessOptions::new();
        let preprocessor = Preprocessor::new(options);

        let input = vec![Srgb::from_u8(128, 128, 128); 100 * 100];
        let result = preprocessor.process(&input, 100, 100);

        // Should keep original dimensions
        assert_eq!(result.width, 100, "Width should stay 100");
        assert_eq!(result.height, 100, "Height should stay 100");
        assert_eq!(result.pixels.len(), 10000, "Should have 100*100 pixels");
    }

    #[test]
    #[ignore = "requires image crate for actual resize"]
    fn test_resize_before_enhancement() {
        // Verify resize happens before saturation/contrast

        // Create a small image with a specific pattern
        let options = PreprocessOptions::new().resize(2, 2);
        let preprocessor = Preprocessor::new(options);

        // 4x4 gradient-like input
        let input = vec![
            Srgb::from_u8(0, 0, 0), // Top-left
            Srgb::from_u8(50, 50, 50),
            Srgb::from_u8(100, 100, 100),
            Srgb::from_u8(150, 150, 150), // Top-right
            Srgb::from_u8(50, 50, 50),
            Srgb::from_u8(100, 100, 100),
            Srgb::from_u8(150, 150, 150),
            Srgb::from_u8(200, 200, 200),
            Srgb::from_u8(100, 100, 100),
            Srgb::from_u8(150, 150, 150),
            Srgb::from_u8(200, 200, 200),
            Srgb::from_u8(250, 250, 250),
            Srgb::from_u8(150, 150, 150), // Bottom-left
            Srgb::from_u8(200, 200, 200),
            Srgb::from_u8(250, 250, 250),
            Srgb::from_u8(255, 255, 255), // Bottom-right (close to white)
        ];
        let result = preprocessor.process(&input, 4, 4);

        // Output should be 2x2 with applied enhancements
        assert_eq!(result.width, 2);
        assert_eq!(result.height, 2);
        assert_eq!(result.pixels.len(), 4);
    }

    #[test]
    #[ignore = "requires image crate for actual resize"]
    fn test_resize_full_pipeline_with_photo_preset() {
        let options = PreprocessOptions::new().resize(10, 10);
        let preprocessor = Preprocessor::new(options);

        // 50x50 image with mix of colors
        let mut input = Vec::with_capacity(50 * 50);
        for y in 0..50u32 {
            for x in 0..50u32 {
                let v = ((x + y) * 255 / 100) as u8;
                input.push(Srgb::from_u8(v, v, v));
            }
        }

        let result = preprocessor.process(&input, 50, 50);

        // Should be 10x10 after resize
        assert_eq!(result.width, 10);
        assert_eq!(result.height, 10);
        assert_eq!(result.pixels.len(), 100);
    }
}
