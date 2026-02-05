//! Atkinson error diffusion dithering algorithm.
//!
//! Atkinson dithering distributes only 75% of the quantization error (6/8),
//! which prevents color bleeding with small palettes. Originally developed
//! by Bill Atkinson for the Apple Macintosh, it's ideal for e-ink displays.

use crate::color::LinearRgb;
use crate::palette::Palette;

use super::{dither_with_kernel, Dither, DitherOptions, ATKINSON};

/// Atkinson error diffusion dithering.
///
/// Atkinson dithering is the recommended algorithm for e-ink displays with
/// small color palettes (typically 7-16 colors). By propagating only 75% of
/// the quantization error, it avoids the "bleeding" artifacts that occur with
/// 100% propagation algorithms like Floyd-Steinberg.
///
/// # Algorithm
///
/// The Atkinson kernel distributes error to 6 neighbors:
///
/// ```text
///        X   1   1
///    1   1   1
///        1
/// ```
///
/// Each neighbor receives 1/8 of the error, for a total of 6/8 = 75%.
///
/// # Features
///
/// - **Serpentine scanning**: Alternates row direction to eliminate
///   directional artifacts (enabled by default)
/// - **Exact match preservation**: Pixels matching palette colors exactly
///   skip dithering entirely, keeping text crisp
/// - **Error clamping**: Prevents blooming by limiting accumulated error
///
/// # Example
///
/// ```ignore
/// use eink_dither::{Atkinson, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = Atkinson.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct Atkinson;

impl Dither for Atkinson {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel(image, width, height, palette, &ATKINSON, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Srgb;

    fn make_bw_palette() -> Palette {
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        Palette::new(&colors, None).unwrap()
    }

    fn make_rgb_palette() -> Palette {
        let colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(0, 255, 0),     // green
            Srgb::from_u8(0, 0, 255),     // blue
        ];
        Palette::new(&colors, None).unwrap()
    }

    #[test]
    fn test_exact_match_preserved() {
        // Palette: black, white
        let palette = make_bw_palette();
        let options = DitherOptions::new().preserve_exact_matches(true);

        // Input: single black pixel (exact match to palette index 0)
        let black_linear = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let image = vec![black_linear];

        let result = Atkinson.dither(&image, 1, 1, &palette, &options);
        assert_eq!(result[0], 0, "Black pixel should map to index 0");
    }

    #[test]
    fn test_exact_match_white() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // Input: single white pixel
        let white_linear = LinearRgb::from(Srgb::from_u8(255, 255, 255));
        let image = vec![white_linear];

        let result = Atkinson.dither(&image, 1, 1, &palette, &options);
        assert_eq!(result[0], 1, "White pixel should map to index 1");
    }

    #[test]
    fn test_serpentine_alternates_direction() {
        // Create a small gradient image and compare with/without serpentine
        let palette = make_bw_palette();

        // 4x4 gradient (dark to light)
        let mut image = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let val = (y * 4 + x) as f32 / 15.0; // 0.0 to 1.0
                let srgb = Srgb::new(val, val, val);
                image.push(LinearRgb::from(srgb));
            }
        }

        let opts_serpentine = DitherOptions::new().serpentine(true);
        let opts_no_serpentine = DitherOptions::new().serpentine(false);

        let result_serp = Atkinson.dither(&image, 4, 4, &palette, &opts_serpentine);
        let result_no_serp = Atkinson.dither(&image, 4, 4, &palette, &opts_no_serpentine);

        // Results should be different (serpentine affects error diffusion pattern)
        // Not necessarily completely different, but with this gradient they should differ
        assert_ne!(
            result_serp, result_no_serp,
            "Serpentine should produce different pattern"
        );
    }

    #[test]
    fn test_error_clamping() {
        // Input color very far from any palette color - test that clamping prevents extreme values
        let palette = make_bw_palette();
        let options = DitherOptions::new().error_clamp(0.5);

        // Mid-gray row - will generate significant error with B&W palette
        // The clamping should prevent runaway error accumulation
        let mid_gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![mid_gray; 100]; // 100 pixels wide

        let result = Atkinson.dither(&image, 100, 1, &palette, &options);

        // Should produce a mix of black and white (dithered pattern)
        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();

        // Neither should dominate completely due to error diffusion
        assert!(blacks > 10, "Should have some black pixels: {}", blacks);
        assert!(whites > 10, "Should have some white pixels: {}", whites);
    }

    #[test]
    fn test_gradient_dithering() {
        // Black-to-white gradient with B&W palette
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // 10-pixel gradient from black to white
        let image: Vec<LinearRgb> = (0..10)
            .map(|i| {
                let srgb_val = i as f32 / 9.0;
                LinearRgb::from(Srgb::new(srgb_val, srgb_val, srgb_val))
            })
            .collect();

        let result = Atkinson.dither(&image, 10, 1, &palette, &options);

        // First pixel (black) should map to black
        assert_eq!(result[0], 0, "Pure black should map to black");

        // Last pixel (white) should map to white
        assert_eq!(result[9], 1, "Pure white should map to white");

        // Should have a reasonable distribution (not all one color)
        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();
        assert!(blacks >= 2, "Should have multiple black pixels");
        assert!(whites >= 2, "Should have multiple white pixels");
    }

    #[test]
    fn test_linear_rgb_diffusion() {
        // Test that mid-gray (sRGB 128) dithers correctly in linear RGB space
        // Linear mid-point (~0.214) is different from sRGB mid-point (0.5)
        let palette = make_bw_palette();
        let options = DitherOptions::new().serpentine(false);

        // sRGB 128/255 = 0.502 in sRGB space
        // Converts to ~0.214 in linear space
        // This is closer to black than white perceptually
        let mid_srgb = Srgb::from_u8(128, 128, 128);
        let mid_linear = LinearRgb::from(mid_srgb);

        // Verify the linear value is indeed closer to 0 than 0.5
        assert!(
            mid_linear.r < 0.25,
            "sRGB 128 should be ~0.214 in linear, got {}",
            mid_linear.r
        );

        // Dither an area of this gray - larger area for more stable statistics
        let size = 10;
        let image = vec![mid_linear; size * size];
        let result = Atkinson.dither(&image, size, size, &palette, &options);

        // Should produce a mix with tendency toward darker
        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();

        // The linear value (~0.214) means roughly 21% brightness
        // So we expect significantly more blacks than whites
        // Allow some tolerance due to Atkinson's 75% propagation and edge effects
        assert!(
            blacks > 0 && whites > 0,
            "Should produce a dithered pattern, not solid: {} blacks, {} whites",
            blacks,
            whites
        );

        // The ratio should favor black since linear ~0.214 is closer to black (0) than white (1)
        // With 100 pixels and ~21% brightness, we'd expect roughly 79 blacks, 21 whites
        // But Atkinson's error diffusion patterns vary - just check it's reasonably darker
        assert!(
            blacks > whites,
            "sRGB 128 gray (linear ~0.214) should have more blacks than whites: {} blacks, {} whites",
            blacks,
            whites
        );
    }

    #[test]
    fn test_exact_match_blocks_error() {
        // Image with exact match surrounded by non-matches
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // Create 3x3 image: gray everywhere except center is black (exact match)
        let gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0)); // Exact match

        #[rustfmt::skip]
        let image = vec![
            gray,  gray,  gray,
            gray,  black, gray,
            gray,  gray,  gray,
        ];

        let result = Atkinson.dither(&image, 3, 3, &palette, &options);

        // Center pixel (index 4) should be exactly black (index 0)
        assert_eq!(
            result[4], 0,
            "Center black pixel should be preserved as exact match"
        );
    }

    #[test]
    fn test_preserve_exact_matches_disabled() {
        let palette = make_bw_palette();
        let opts_preserve = DitherOptions::new().preserve_exact_matches(true);
        let opts_no_preserve = DitherOptions::new().preserve_exact_matches(false);

        // Exact black followed by gray
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![black, gray, gray, gray];

        let result_preserve = Atkinson.dither(&image, 4, 1, &palette, &opts_preserve);
        let result_no_preserve = Atkinson.dither(&image, 4, 1, &palette, &opts_no_preserve);

        // Both should output black for the first pixel
        assert_eq!(result_preserve[0], 0);
        assert_eq!(result_no_preserve[0], 0);

        // Results may differ slightly due to exact match handling differences
        // With preserve=true, the exact black doesn't propagate error
        // With preserve=false, error is propagated normally
    }

    #[test]
    fn test_multi_row_image() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // 4x4 image with vertical gradient (top=black, bottom=white)
        let image: Vec<LinearRgb> = (0..16)
            .map(|i| {
                let y = i / 4;
                let srgb_val = y as f32 / 3.0;
                LinearRgb::from(Srgb::new(srgb_val, srgb_val, srgb_val))
            })
            .collect();

        let result = Atkinson.dither(&image, 4, 4, &palette, &options);

        // Top row should be mostly black
        let top_row = &result[0..4];
        let top_blacks = top_row.iter().filter(|&&x| x == 0).count();
        assert!(top_blacks >= 3, "Top row should be mostly black");

        // Bottom row should be mostly white
        let bottom_row = &result[12..16];
        let bottom_whites = bottom_row.iter().filter(|&&x| x == 1).count();
        assert!(bottom_whites >= 3, "Bottom row should be mostly white");
    }

    #[test]
    fn test_color_image() {
        // Test with color palette
        let palette = make_rgb_palette();
        let options = DitherOptions::new();

        // Pure red pixel
        let red = LinearRgb::from(Srgb::from_u8(255, 0, 0));
        let image = vec![red];

        let result = Atkinson.dither(&image, 1, 1, &palette, &options);

        // Should match red (index 2)
        assert_eq!(result[0], 2, "Pure red should match red palette entry");
    }

    #[test]
    fn test_empty_image() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let image: Vec<LinearRgb> = vec![];

        let result = Atkinson.dither(&image, 0, 0, &palette, &options);
        assert!(result.is_empty(), "Empty input should produce empty output");
    }

    #[test]
    fn test_single_column_image() {
        // 1-pixel wide, 4 rows tall - tests row advancement
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        let image: Vec<LinearRgb> = vec![
            LinearRgb::from(Srgb::from_u8(0, 0, 0)),       // black
            LinearRgb::from(Srgb::from_u8(64, 64, 64)),    // dark gray
            LinearRgb::from(Srgb::from_u8(192, 192, 192)), // light gray
            LinearRgb::from(Srgb::from_u8(255, 255, 255)), // white
        ];

        let result = Atkinson.dither(&image, 1, 4, &palette, &options);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0], 0, "Black should stay black");
        assert_eq!(result[3], 1, "White should stay white");
    }

    #[test]
    fn test_output_indices_in_range() {
        let palette = make_rgb_palette();
        let options = DitherOptions::new();

        // Random-ish colors
        let image: Vec<LinearRgb> = (0..100)
            .map(|i| {
                let r = ((i * 7) % 256) as f32 / 255.0;
                let g = ((i * 13) % 256) as f32 / 255.0;
                let b = ((i * 23) % 256) as f32 / 255.0;
                LinearRgb::from(Srgb::new(r, g, b))
            })
            .collect();

        let result = Atkinson.dither(&image, 10, 10, &palette, &options);

        // All indices should be in valid range
        for (i, &idx) in result.iter().enumerate() {
            assert!(
                (idx as usize) < palette.len(),
                "Index {} at position {} exceeds palette size {}",
                idx,
                i,
                palette.len()
            );
        }
    }

    #[test]
    fn test_75_percent_error_propagation() {
        // Atkinson propagates only 75% of error (6/8), which should result in
        // less total error being spread compared to Floyd-Steinberg's 100%
        let palette = make_bw_palette();
        let options = DitherOptions::new().serpentine(false);

        // Use a larger area for statistical stability
        let width = 12;
        let height = 12;
        let gray_value = 0.5_f32; // 50% gray in linear space
        let image: Vec<LinearRgb> = (0..width * height)
            .map(|_| LinearRgb::new(gray_value, gray_value, gray_value))
            .collect();

        let result = Atkinson.dither(&image, width, height, &palette, &options);

        // Count white pixels
        let white_count = result.iter().filter(|&&x| x == 1).count();
        let white_ratio = white_count as f32 / (width * height) as f32;

        // With 75% propagation, output should approximate input brightness
        // but with Atkinson's characteristic "lost" error, may be slightly different
        assert!(
            (white_ratio - gray_value).abs() < 0.2,
            "Expected ~{} white ratio, got {}",
            gray_value,
            white_ratio
        );
    }
}
