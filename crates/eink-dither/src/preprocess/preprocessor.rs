//! Core preprocessing logic for e-ink dithering.
//!
//! The [`Preprocessor`] struct transforms input images for optimal e-ink output
//! by resizing, detecting exact palette matches, boosting saturation, and adjusting contrast.
//!
//! # Processing Pipeline
//!
//! 1. **Resize** (if target dimensions specified)
//!    - Lanczos3 resampling for high-quality scaling
//!    - Happens first to ensure optimal quality at target size
//!
//! 2. **Exact match detection** (on resized pixels)
//!    - Pixels matching palette colors are flagged for preservation
//!    - Detection uses Srgb bytes, not transformed values
//!    - Note: resize may destroy exact matches - this is expected for photos
//!
//! 3. **Saturation boost** (Oklch chroma scaling)
//!    - Perceptually correct: no hue shift
//!    - Only applied to non-matching pixels
//!
//! 4. **Contrast adjustment** (linear RGB midpoint scaling)
//!    - Scales around 0.5 midpoint
//!    - Only applied to non-matching pixels
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
//! let options = PreprocessOptions::photo().resize(100, 100);
//! let preprocessor = Preprocessor::new(&palette, options);
//!
//! // Process an image (2x1 pixels: black and mid-gray)
//! let input = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(128, 128, 128)];
//! let result = preprocessor.process(&input, 2, 1);
//!
//! // Result contains processed pixels, dimensions, and exact matches
//! assert_eq!(result.width, 100); // Resized
//! assert_eq!(result.height, 100);
//! ```

use crate::color::{LinearRgb, Oklab, Srgb};
use crate::palette::Palette;
use crate::preprocess::PreprocessOptions;

use super::oklch::Oklch;
use super::resize::resize_lanczos;

/// Result of preprocessing an image.
///
/// Contains the processed pixels, updated dimensions (after resize), and
/// exact match information for dithering optimization.
///
/// # Example
///
/// ```
/// use eink_dither::{Preprocessor, PreprocessOptions, PreprocessResult, Palette, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
/// let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());
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

    /// Exact match map for each pixel.
    ///
    /// `Some(palette_idx)` if the pixel exactly matched a palette color
    /// (before enhancement), `None` otherwise.
    ///
    /// Dithering algorithms can skip error diffusion for exact matches
    /// to preserve crisp edges in text and UI elements.
    pub exact_matches: Vec<Option<u8>>,
}

/// Image preprocessor with exact match detection and color enhancement.
///
/// `Preprocessor` transforms images for optimal e-ink display output using
/// a multi-phase pipeline:
///
/// 1. **Resize** to target dimensions (Lanczos3 filter)
/// 2. **Detect exact matches** against actual palette colors
/// 3. **Boost saturation** for non-matching pixels (Oklch chroma)
/// 4. **Adjust contrast** for non-matching pixels (linear RGB)
///
/// # Two-Phase Detection Strategy
///
/// Exact match detection uses **actual colors** (what the display shows),
/// not official colors. This is critical when displays have color calibration
/// differences from their specifications.
///
/// Matching pixels are flagged but NOT enhanced - they pass through unchanged
/// to preserve crisp edges in text, logos, and UI elements.
///
/// # Photo vs Graphics Intent
///
/// Use presets for common scenarios:
/// - [`PreprocessOptions::photo()`]: Enhances photos with saturation/contrast boost
/// - [`PreprocessOptions::graphics()`]: No enhancement for logos/text/UI
///
/// The preprocessor holds a reference to the palette and cannot outlive it.
///
/// # Lifetime
///
/// The `'a` lifetime ties the preprocessor to its palette reference.
/// The preprocessor cannot outlive the palette it references.
///
/// # Thread Safety
///
/// `Preprocessor` is `Send + Sync` if the palette is. Multiple threads can
/// share an immutable preprocessor for parallel image processing.
#[derive(Debug)]
pub struct Preprocessor<'a> {
    /// Reference to palette for exact match detection
    palette: &'a Palette,
    /// Preprocessing configuration
    options: PreprocessOptions,
}

impl<'a> Preprocessor<'a> {
    /// Create a new preprocessor with the given palette and options.
    ///
    /// # Arguments
    /// * `palette` - Palette for exact match detection
    /// * `options` - Preprocessing configuration
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Preprocessor, PreprocessOptions, Palette, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    /// let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());
    /// ```
    #[inline]
    pub fn new(palette: &'a Palette, options: PreprocessOptions) -> Self {
        Self { palette, options }
    }

    /// Check if a pixel exactly matches any palette color (by Srgb bytes).
    ///
    /// Matches against the ACTUAL colors (what display shows), not official colors.
    /// This ensures we detect pixels that will appear identical on the display.
    ///
    /// # Arguments
    /// * `pixel` - The pixel to check
    ///
    /// # Returns
    /// `Some(index)` if pixel matches palette entry, `None` otherwise
    #[inline]
    pub fn find_exact_match_srgb(&self, pixel: Srgb) -> Option<u8> {
        let pixel_bytes = pixel.to_bytes();
        for i in 0..self.palette.len() {
            let palette_bytes = self.palette.actual(i).to_bytes();
            if pixel_bytes == palette_bytes {
                return Some(i as u8);
            }
        }
        None
    }

    /// Process an image with the complete preprocessing pipeline.
    ///
    /// # Processing Order
    ///
    /// 1. **Resize** (if target dimensions specified)
    ///    - Lanczos3 resampling for high-quality scaling
    ///    - If only some dimensions match, no resize occurs (both must be specified)
    ///
    /// 2. **Exact match detection** (on resized pixels)
    ///    - Detect pixels matching palette colors exactly
    ///    - Note: resize may destroy exact matches, which is expected for photos
    ///
    /// 3. **Saturation boost** (for non-matching pixels)
    ///    - Perceptually correct Oklch chroma scaling
    ///
    /// 4. **Contrast adjustment** (for non-matching pixels)
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
    /// - `exact_matches`: Map of exact palette matches
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Preprocessor, PreprocessOptions, Palette, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    /// let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());
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

        let working_total = working_width * working_height;

        // Step 2: Detect exact matches (on resized pixels)
        let exact_matches: Vec<Option<u8>> = if self.options.preserve_exact_matches {
            working_pixels
                .iter()
                .map(|&pixel| self.find_exact_match_srgb(pixel))
                .collect()
        } else {
            vec![None; working_total]
        };

        // Step 3 & 4: Convert to LinearRgb, applying enhancements to non-matches
        let pixels: Vec<LinearRgb> = working_pixels
            .iter()
            .zip(exact_matches.iter())
            .map(|(&pixel, &exact_match)| {
                // Convert sRGB to LinearRgb
                let mut linear = LinearRgb::from(pixel);

                // Skip enhancement for exact matches
                if exact_match.is_some() {
                    return linear;
                }

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
            exact_matches,
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
        // LinearRgb -> Oklab -> Oklch
        let oklab = Oklab::from(pixel);
        let oklch = Oklch::from(oklab);

        // Scale chroma
        let boosted = oklch.scale_chroma(factor);

        // Oklch -> Oklab -> LinearRgb
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

    /// Helper to create a simple black/white palette
    fn bw_palette() -> Palette {
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        Palette::new(&colors, None).unwrap()
    }

    /// Helper to check approximate equality for f32
    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    // =========================================================================
    // Exact Match Detection Tests
    // =========================================================================

    #[test]
    fn test_exact_match_detection_black() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        let black = Srgb::from_u8(0, 0, 0);
        assert_eq!(
            preprocessor.find_exact_match_srgb(black),
            Some(0),
            "Black should match palette index 0"
        );
    }

    #[test]
    fn test_exact_match_detection_white() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        let white = Srgb::from_u8(255, 255, 255);
        assert_eq!(
            preprocessor.find_exact_match_srgb(white),
            Some(1),
            "White should match palette index 1"
        );
    }

    #[test]
    fn test_exact_match_detection_gray_no_match() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        let gray = Srgb::from_u8(128, 128, 128);
        assert_eq!(
            preprocessor.find_exact_match_srgb(gray),
            None,
            "Mid-gray should not match any palette color"
        );
    }

    #[test]
    fn test_exact_match_detection_near_black() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        // 1 LSB off from black
        let near_black = Srgb::from_u8(1, 0, 0);
        assert_eq!(
            preprocessor.find_exact_match_srgb(near_black),
            None,
            "Near-black should not match (exact byte comparison)"
        );
    }

    #[test]
    fn test_exact_match_uses_actual_colors() {
        // Create palette where official != actual
        let official = [Srgb::from_u8(255, 0, 0)]; // Official red
        let actual = [Srgb::from_u8(200, 50, 50)]; // Actual muddy red
        let palette = Palette::new(&official, Some(&actual)).unwrap();

        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        // Input matches ACTUAL color (muddy red)
        let muddy_red = Srgb::from_u8(200, 50, 50);
        assert_eq!(
            preprocessor.find_exact_match_srgb(muddy_red),
            Some(0),
            "Should match actual color, not official"
        );

        // Input matches OFFICIAL color (bright red) - should NOT match
        let bright_red = Srgb::from_u8(255, 0, 0);
        assert_eq!(
            preprocessor.find_exact_match_srgb(bright_red),
            None,
            "Should not match official color when actual differs"
        );
    }

    // =========================================================================
    // Process Pipeline Tests
    // =========================================================================

    #[test]
    fn test_process_returns_correct_lengths() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        let input = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(128, 128, 128),
            Srgb::from_u8(255, 255, 255),
        ];
        let result = preprocessor.process(&input, 3, 1);

        assert_eq!(result.pixels.len(), 3, "Should return 3 processed pixels");
        assert_eq!(
            result.exact_matches.len(),
            3,
            "Should return 3 match entries"
        );
        assert_eq!(result.width, 3, "Width should be 3");
        assert_eq!(result.height, 1, "Height should be 1");
    }

    #[test]
    fn test_process_detects_exact_matches() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        let input = [
            Srgb::from_u8(0, 0, 0),       // Black - matches
            Srgb::from_u8(128, 128, 128), // Gray - no match
            Srgb::from_u8(255, 255, 255), // White - matches
        ];
        let result = preprocessor.process(&input, 3, 1);

        assert_eq!(
            result.exact_matches[0],
            Some(0),
            "Black should match index 0"
        );
        assert_eq!(result.exact_matches[1], None, "Gray should not match");
        assert_eq!(
            result.exact_matches[2],
            Some(1),
            "White should match index 1"
        );
    }

    #[test]
    fn test_process_respects_preserve_exact_matches_flag() {
        let palette = bw_palette();

        // With preservation disabled
        let options = PreprocessOptions::new().preserve_exact_matches(false);
        let preprocessor = Preprocessor::new(&palette, options);

        let input = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        let result = preprocessor.process(&input, 2, 1);

        // All should be None when preservation is disabled
        assert_eq!(
            result.exact_matches[0], None,
            "Should be None when preservation disabled"
        );
        assert_eq!(
            result.exact_matches[1], None,
            "Should be None when preservation disabled"
        );
    }

    // =========================================================================
    // Saturation Boost Tests
    // =========================================================================

    #[test]
    fn test_boost_saturation_increases_chroma() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

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
    fn test_exact_match_not_enhanced() {
        let palette = bw_palette();
        // High saturation and contrast that would change pixels
        let options = PreprocessOptions::new().saturation(2.0).contrast(2.0);
        let preprocessor = Preprocessor::new(&palette, options);

        // Process a colored pixel that matches palette (black)
        let input = [Srgb::from_u8(0, 0, 0)];
        let result = preprocessor.process(&input, 1, 1);

        // Black matches palette
        assert_eq!(result.exact_matches[0], Some(0));

        // Should be converted directly without enhancement
        let expected = LinearRgb::from(input[0]);
        assert!(
            approx_eq(result.pixels[0].r, expected.r, 1e-6),
            "Exact match should not be enhanced"
        );
        assert!(
            approx_eq(result.pixels[0].g, expected.g, 1e-6),
            "Exact match should not be enhanced"
        );
        assert!(
            approx_eq(result.pixels[0].b, expected.b, 1e-6),
            "Exact match should not be enhanced"
        );
    }

    #[test]
    fn test_non_match_is_enhanced() {
        let palette = bw_palette();
        // Significant saturation boost
        let options = PreprocessOptions::new().saturation(2.0).contrast(1.0);
        let preprocessor = Preprocessor::new(&palette, options);

        // A colored pixel that doesn't match palette
        let input = [Srgb::from_u8(200, 100, 50)];
        let result = preprocessor.process(&input, 1, 1);

        // Doesn't match
        assert_eq!(result.exact_matches[0], None);

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        let input = [
            Srgb::from_u8(0, 0, 0),       // Black (exact match)
            Srgb::from_u8(255, 128, 64),  // Orange (no match, will be enhanced)
            Srgb::from_u8(255, 255, 255), // White (exact match)
        ];
        let result = preprocessor.process(&input, 3, 1);

        // Verify matches
        assert_eq!(result.exact_matches[0], Some(0), "Black matches");
        assert_eq!(result.exact_matches[1], None, "Orange no match");
        assert_eq!(result.exact_matches[2], Some(1), "White matches");

        // Verify processed has 3 LinearRgb values
        assert_eq!(result.pixels.len(), 3);
    }

    #[test]
    fn test_process_with_graphics_preset() {
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::graphics());

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
        let palette = bw_palette();
        let preprocessor = Preprocessor::new(&palette, PreprocessOptions::photo());

        // 2x2 image
        let input = [
            Srgb::from_u8(0, 0, 0),       // (0,0) black
            Srgb::from_u8(128, 128, 128), // (1,0) gray
            Srgb::from_u8(64, 64, 64),    // (0,1) dark gray
            Srgb::from_u8(255, 255, 255), // (1,1) white
        ];
        let result = preprocessor.process(&input, 2, 2);

        assert_eq!(result.pixels.len(), 4);
        assert_eq!(result.exact_matches.len(), 4);

        // Check corner matches
        assert_eq!(result.exact_matches[0], Some(0), "Top-left black matches");
        assert_eq!(
            result.exact_matches[3],
            Some(1),
            "Bottom-right white matches"
        );
    }

    #[test]
    fn test_saturation_then_contrast_order() {
        // Verify the processing order: saturation first, then contrast
        let palette = bw_palette();

        // Use settings where order matters
        let options = PreprocessOptions::new().saturation(1.5).contrast(1.2);
        let preprocessor = Preprocessor::new(&palette, options);

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
        let palette = bw_palette();
        // Set up resize to 50x50
        let options = PreprocessOptions::graphics().resize(50, 50);
        let preprocessor = Preprocessor::new(&palette, options);

        // 100x100 solid gray image
        let input = vec![Srgb::from_u8(128, 128, 128); 100 * 100];
        let result = preprocessor.process(&input, 100, 100);

        // Should be resized
        assert_eq!(result.width, 50, "Width should be 50 after resize");
        assert_eq!(result.height, 50, "Height should be 50 after resize");
        assert_eq!(result.pixels.len(), 2500, "Should have 50*50 pixels");
        assert_eq!(
            result.exact_matches.len(),
            2500,
            "Should have 50*50 match entries"
        );
    }

    #[test]
    fn test_process_without_resize() {
        let palette = bw_palette();
        // No resize specified
        let options = PreprocessOptions::graphics();
        let preprocessor = Preprocessor::new(&palette, options);

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
        let palette = bw_palette();

        // Create a small image with a specific pattern
        let options = PreprocessOptions::photo().resize(2, 2);
        let preprocessor = Preprocessor::new(&palette, options);

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
        let palette = bw_palette();
        let options = PreprocessOptions::photo().resize(10, 10);
        let preprocessor = Preprocessor::new(&palette, options);

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
        assert_eq!(result.exact_matches.len(), 100);

        // All enhancements should be applied (photo preset has saturation 1.5, contrast 1.1)
        // Since this is a gradient, most pixels won't be exact matches
        let non_matches: Vec<_> = result
            .exact_matches
            .iter()
            .filter(|m| m.is_none())
            .collect();
        assert!(
            non_matches.len() > 0,
            "Some pixels should not be exact matches"
        );
    }

    #[test]
    #[ignore = "requires image crate for actual resize"]
    fn test_resize_preserves_palette_matches_in_graphics_mode() {
        // In graphics mode (no enhancement), solid color images should stay solid
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(128, 128, 128),
        ];
        let palette = Palette::new(&colors, None).unwrap();

        let options = PreprocessOptions::graphics().resize(5, 5);
        let preprocessor = Preprocessor::new(&palette, options);

        // Solid gray (matches palette)
        let input = vec![Srgb::from_u8(128, 128, 128); 10 * 10];
        let result = preprocessor.process(&input, 10, 10);

        // After resize, center should still match (Lanczos might affect edges)
        // Check center pixel
        let center_idx = 2 * 5 + 2; // (2, 2) in 5x5
        assert_eq!(
            result.exact_matches[center_idx],
            Some(2),
            "Center of solid color resized image should still match palette"
        );
    }
}
