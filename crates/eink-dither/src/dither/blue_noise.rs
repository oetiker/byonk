//! Blue noise ordered dithering algorithm.
//!
//! Blue noise dithering is an ordered dithering technique that uses a
//! precomputed threshold matrix with blue noise spectral properties.
//! Unlike error diffusion algorithms, it processes each pixel independently,
//! making it inherently parallelizable.
//!
//! # Comparison with Error Diffusion
//!
//! | Aspect | Blue Noise | Error Diffusion |
//! |--------|------------|-----------------|
//! | Pattern | Organic, aperiodic | Varies by algorithm |
//! | Parallelizable | Yes (per-pixel) | No (neighbor dependencies) |
//! | Best for | Graphics, icons | Photos, gradients |
//! | Artifacts | None visible | Potential bleeding |
//!
//! # Comparison with Bayer Dithering
//!
//! Blue noise produces more natural-looking patterns than Bayer matrices
//! because its frequency spectrum lacks low frequencies. Bayer matrices
//! create visible cross-hatch patterns; blue noise creates organic texture.
//!
//! # Algorithm
//!
//! For each pixel:
//! 1. Check for exact palette match (if enabled)
//! 2. Find the two nearest palette colors
//! 3. Compute blend factor from color distances
//! 4. Use blue noise threshold to select between the two colors
//!
//! Note: `serpentine` and `error_clamp` options are ignored since this
//! algorithm has no error diffusion and no scan direction.

use super::blue_noise_matrix::BLUE_NOISE_64;
use super::{find_exact_match, Dither, DitherOptions};
use crate::color::{LinearRgb, Oklab};
use crate::palette::Palette;

/// Blue noise ordered dithering.
///
/// Blue noise dithering produces organic, natural-looking patterns without
/// the visible grid artifacts of Bayer dithering. Each pixel is processed
/// independently, making this algorithm fully parallelizable.
///
/// # When to Use
///
/// - Graphics, icons, and UI elements
/// - When parallel processing is needed
/// - When you want organic texture over smooth gradients
///
/// # Ignored Options
///
/// Since this is a per-pixel algorithm with no error propagation:
/// - `serpentine`: Ignored (no scan direction)
/// - `error_clamp`: Ignored (no error accumulation)
///
/// # Example
///
/// ```ignore
/// use eink_dither::{BlueNoiseDither, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = BlueNoiseDither.dither(&pixels, width, height, &palette, &options);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct BlueNoiseDither;

/// Find the second nearest palette color, excluding a specified index.
///
/// # Arguments
///
/// * `color` - The color to match in Oklab space
/// * `palette` - The palette to search
/// * `exclude` - The index to exclude (typically the nearest color)
/// * `pixel_chroma` - Precomputed chroma magnitude of the input pixel
///
/// # Returns
///
/// `(index, distance)` of the second nearest color.
/// If the palette has only one color, returns `(0, f32::MAX)`.
fn find_second_nearest(
    color: Oklab,
    palette: &Palette,
    exclude: usize,
    pixel_chroma: f32,
) -> (usize, f32) {
    // Handle single-color palette edge case
    if palette.len() == 1 {
        return (0, f32::MAX);
    }

    let mut best_idx = 0;
    let mut best_dist = f32::MAX;

    for i in 0..palette.len() {
        if i == exclude {
            continue;
        }

        let dist = palette.distance(color, palette.actual_oklab(i), pixel_chroma, i);
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }

    (best_idx, best_dist)
}

impl Dither for BlueNoiseDither {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        let mut output = vec![0u8; width * height];

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let pixel = image[idx];

                // Check for exact palette match first (if enabled)
                if options.preserve_exact_matches {
                    if let Some(palette_idx) = find_exact_match(pixel, palette) {
                        output[idx] = palette_idx;
                        continue;
                    }
                }

                // Convert to Oklab for perceptual matching
                let oklab = Oklab::from(pixel);
                let pixel_chroma = (oklab.a * oklab.a + oklab.b * oklab.b).sqrt();

                // Find the two closest palette colors
                let (idx1, raw_dist1) = palette.find_nearest(oklab);
                let (idx2, raw_dist2) = find_second_nearest(oklab, palette, idx1, pixel_chroma);

                // Convert to linear distances for blend factor.
                // Euclidean returns squared distances; HyAB returns linear distances.
                let dist1 = if palette.is_euclidean() {
                    raw_dist1.sqrt()
                } else {
                    raw_dist1
                };
                let dist2 = if palette.is_euclidean() {
                    raw_dist2.sqrt()
                } else {
                    raw_dist2
                };
                let total_dist = dist1 + dist2;

                // If total distance is near zero, both colors are effectively the same
                // Use the nearest color
                if total_dist < 1e-10 {
                    output[idx] = idx1 as u8;
                    continue;
                }

                // Blend factor: how much of idx2 to use
                // When pixel is exactly at idx1: dist1=0, blend=0 (use idx1)
                // When pixel is exactly at idx2: dist2=0, blend=1 (use idx2)
                // When equidistant: blend=0.5
                let blend = dist1 / total_dist;

                // Get threshold from blue noise matrix (tiled with modulo)
                let threshold = BLUE_NOISE_64[y % 64][x % 64] as f32 / 255.0;

                // Select color based on threshold
                // threshold < (1 - blend) means use idx1
                // When blend is low (pixel closer to idx1), (1-blend) is high,
                // so we're more likely to pick idx1
                output[idx] = if threshold < (1.0 - blend) {
                    idx1 as u8
                } else {
                    idx2 as u8
                };
            }
        }

        output
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

    fn make_7_color_palette() -> Palette {
        let colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(0, 255, 0),     // green
            Srgb::from_u8(0, 0, 255),     // blue
            Srgb::from_u8(255, 255, 0),   // yellow
            Srgb::from_u8(255, 128, 0),   // orange
        ];
        Palette::new(&colors, None).unwrap()
    }

    #[test]
    fn test_basic_dithering() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // Create 4x4 mid-gray image
        let mid_gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![mid_gray; 16];

        let result = BlueNoiseDither.dither(&image, 4, 4, &palette, &options);

        // Should produce a mix of black (0) and white (1)
        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();

        assert!(
            blacks > 0 && whites > 0,
            "Mid-gray should dither to mix: {} blacks, {} whites",
            blacks,
            whites
        );
    }

    #[test]
    fn test_exact_match_preserved() {
        let palette = make_bw_palette();
        let options = DitherOptions::new().preserve_exact_matches(true);

        // Create image with exact black pixels
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let image = vec![black; 16];

        let result = BlueNoiseDither.dither(&image, 4, 4, &palette, &options);

        // All pixels should be black (index 0)
        for &idx in &result {
            assert_eq!(idx, 0, "Exact black should map to index 0");
        }
    }

    #[test]
    fn test_exact_match_disabled() {
        let palette = make_bw_palette();
        let options = DitherOptions::new().preserve_exact_matches(false);

        // Create image with exact black pixels
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let image = vec![black; 16];

        let result = BlueNoiseDither.dither(&image, 4, 4, &palette, &options);

        // Even with exact match disabled, pure black should still match black
        // because it's the nearest color. The difference is the code path.
        // All pixels should still be black (index 0) since black is nearest
        for &idx in &result {
            assert_eq!(idx, 0, "Pure black should still map to index 0");
        }
    }

    #[test]
    fn test_single_color_palette() {
        let colors = [Srgb::from_u8(128, 128, 128)];
        let palette = Palette::new(&colors, None).unwrap();
        let options = DitherOptions::new();

        // Any input should map to index 0
        let image: Vec<LinearRgb> = (0..16)
            .map(|i| {
                let v = i as f32 / 15.0;
                LinearRgb::new(v, v, v)
            })
            .collect();

        let result = BlueNoiseDither.dither(&image, 4, 4, &palette, &options);

        for &idx in &result {
            assert_eq!(idx, 0, "Single-color palette should always return 0");
        }
    }

    #[test]
    fn test_output_in_palette_range() {
        let palette = make_7_color_palette();
        let options = DitherOptions::new();

        // Create varied color image
        let image: Vec<LinearRgb> = (0..100)
            .map(|i| {
                let r = ((i * 7) % 256) as f32 / 255.0;
                let g = ((i * 13) % 256) as f32 / 255.0;
                let b = ((i * 23) % 256) as f32 / 255.0;
                LinearRgb::from(Srgb::new(r, g, b))
            })
            .collect();

        let result = BlueNoiseDither.dither(&image, 10, 10, &palette, &options);

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
    fn test_deterministic() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        let mid_gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![mid_gray; 64];

        let result1 = BlueNoiseDither.dither(&image, 8, 8, &palette, &options);
        let result2 = BlueNoiseDither.dither(&image, 8, 8, &palette, &options);

        assert_eq!(result1, result2, "Same input should produce same output");
    }

    #[test]
    fn test_find_second_nearest() {
        let palette = make_bw_palette();

        // Mid-gray should have black and white as nearest/second-nearest
        let gray = Oklab::from(LinearRgb::new(0.5, 0.5, 0.5));
        let pixel_chroma = (gray.a * gray.a + gray.b * gray.b).sqrt();

        // If nearest is black (0), second should be white (1)
        let (idx, dist) = find_second_nearest(gray, &palette, 0, pixel_chroma);
        assert_eq!(idx, 1, "Second nearest excluding black should be white");
        assert!(dist < f32::MAX, "Distance should be finite");

        // If nearest is white (1), second should be black (0)
        let (idx, _) = find_second_nearest(gray, &palette, 1, pixel_chroma);
        assert_eq!(idx, 0, "Second nearest excluding white should be black");
    }

    #[test]
    fn test_find_second_nearest_single_color() {
        let colors = [Srgb::from_u8(128, 128, 128)];
        let palette = Palette::new(&colors, None).unwrap();

        let gray = Oklab::from(LinearRgb::new(0.5, 0.5, 0.5));
        let pixel_chroma = (gray.a * gray.a + gray.b * gray.b).sqrt();
        let (idx, dist) = find_second_nearest(gray, &palette, 0, pixel_chroma);

        assert_eq!(idx, 0, "Single-color palette should return index 0");
        assert_eq!(dist, f32::MAX, "Distance should be MAX for single color");
    }

    #[test]
    fn test_pure_colors_map_correctly() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // Pure black
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let result = BlueNoiseDither.dither(&[black], 1, 1, &palette, &options);
        assert_eq!(result[0], 0, "Pure black should map to index 0");

        // Pure white
        let white = LinearRgb::from(Srgb::from_u8(255, 255, 255));
        let result = BlueNoiseDither.dither(&[white], 1, 1, &palette, &options);
        assert_eq!(result[0], 1, "Pure white should map to index 1");
    }

    #[test]
    fn test_gradient_produces_pattern() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        // Create a gradient from black to white
        let image: Vec<LinearRgb> = (0..64)
            .map(|i| {
                let v = i as f32 / 63.0;
                LinearRgb::new(v, v, v)
            })
            .collect();

        let result = BlueNoiseDither.dither(&image, 8, 8, &palette, &options);

        // First pixel (black) should be index 0
        assert_eq!(result[0], 0, "Black end should be black");

        // Last pixel (white) should be index 1
        assert_eq!(result[63], 1, "White end should be white");

        // Should have both black and white throughout
        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();

        assert!(
            blacks > 10 && whites > 10,
            "Gradient should have mix: {} blacks, {} whites",
            blacks,
            whites
        );
    }

    #[test]
    fn test_empty_image() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let image: Vec<LinearRgb> = vec![];

        let result = BlueNoiseDither.dither(&image, 0, 0, &palette, &options);
        assert!(result.is_empty(), "Empty input should produce empty output");
    }

    #[test]
    fn test_grey_gradient_no_chromatic_noise() {
        use crate::DistanceMetric;

        // 6-color palette with HyAB + chroma coupling to prevent chromatic contamination
        let colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(255, 0, 0),     // 2: red
            Srgb::from_u8(0, 255, 0),     // 3: green
            Srgb::from_u8(0, 0, 255),     // 4: blue
            Srgb::from_u8(255, 255, 0),   // 5: yellow
        ];
        let palette = Palette::new(&colors, None)
            .unwrap()
            .with_distance_metric(DistanceMetric::HyAB {
                kl: 2.0,
                kc: 1.0,
                kchroma: 2.0,
            });
        let options = DitherOptions::new();

        // Create a grey gradient (64x64)
        let image: Vec<LinearRgb> = (0..64 * 64)
            .map(|i| {
                let v = (i % 64) as f32 / 63.0;
                LinearRgb::from(Srgb::new(v, v, v))
            })
            .collect();

        let result = BlueNoiseDither.dither(&image, 64, 64, &palette, &options);

        // With chroma coupling, ALL grey pixels must map to achromatic palette
        // entries (black or white). The chroma penalty prevents any grey from
        // matching a chromatic color.
        for (i, &idx) in result.iter().enumerate() {
            assert!(
                idx <= 1,
                "Grey pixel at position {} mapped to chromatic index {} (expected 0 or 1)",
                i, idx
            );
        }
    }
}
