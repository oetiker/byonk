//! Floyd-Steinberg error diffusion with blue noise kernel weight jitter.
//!
//! This variant of Floyd-Steinberg injects per-pixel randomization into the
//! error diffusion weights using a blue noise threshold matrix. The jitter
//! breaks the deterministic weight distribution that causes "worm" artifacts
//! (correlated directional patterns in shadows and highlights).
//!
//! # How It Works
//!
//! Standard Floyd-Steinberg distributes error with fixed weights:
//!
//! ```text
//!        X   7
//!    3   5   1
//! ```
//!
//! This variant rotates weight between the "right" (7/16) and "below" (5/16)
//! directions using blue noise:
//!
//! ```text
//! noise = BLUE_NOISE_64[y % 64][x % 64]     // 0..255
//! alpha = (noise - 128) / 256                // -0.5..+0.5
//! right_weight  = 7.0 - alpha * jitter       // varies around 7
//! below_weight  = 5.0 + alpha * jitter       // varies around 5
//! diagonals     = unchanged (3, 1)           // stable anchors
//! ```
//!
//! Total error propagation is always 100% (sum = 16/16). The blue noise
//! matrix ensures the jitter has high-frequency spatial distribution,
//! making the weight variation imperceptible while effectively breaking
//! directional correlations.
//!
//! # References
//!
//! - Zhou & Fang (2003): threshold modulation for artifact reduction
//! - Ostromoukhov (2001): variable-coefficient error diffusion

use super::blue_noise_matrix::BLUE_NOISE_64;
use super::{clamp_channel, find_exact_match, Dither, DitherOptions, ErrorBuffer};
use crate::color::{LinearRgb, Oklab};
use crate::palette::Palette;

/// Floyd-Steinberg error diffusion with blue noise kernel weight jitter.
///
/// Produces smooth gradients like standard Floyd-Steinberg but with
/// significantly reduced "worm" artifacts. Each pixel's error distribution
/// weights are slightly rotated using a blue noise value, breaking the
/// deterministic patterns that cause directional correlations.
///
/// # When to Use
///
/// - Photographs and natural images with smooth gradients
/// - When standard Floyd-Steinberg shows worm artifacts
/// - When Atkinson's 75% propagation loses too much detail
///
/// # Example
///
/// ```ignore
/// use eink_dither::{FloydSteinbergNoise, Dither, DitherOptions, Palette, LinearRgb};
///
/// let palette = Palette::new(&colors, None).unwrap();
/// let options = DitherOptions::new();
/// let indices = FloydSteinbergNoise.dither(&pixels, width, height, &palette, &options);
/// ```
pub struct FloydSteinbergNoise;

/// Default jitter scale (used by tests that reference the constant).
#[cfg(test)]
const JITTER_SCALE: f32 = 2.0;

impl Dither for FloydSteinbergNoise {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        let mut output = vec![0u8; width * height];

        // Pre-detect exact matches for entire image
        let exact_matches: Vec<Option<u8>> = if options.preserve_exact_matches {
            image
                .iter()
                .map(|&pixel| find_exact_match(pixel, palette))
                .collect()
        } else {
            vec![None; width * height]
        };

        let threshold_sq = options.chroma_clamp * options.chroma_clamp;

        // Floyd-Steinberg kernel reaches 1 row ahead
        let mut error_buf = ErrorBuffer::new(width, 2);

        for y in 0..height {
            let reverse = options.serpentine && y % 2 == 1;

            let x_range: Box<dyn Iterator<Item = usize>> = if reverse {
                Box::new((0..width).rev())
            } else {
                Box::new(0..width)
            };

            for x in x_range {
                let idx = y * width + x;

                // Exact palette match handling.
                if let Some(palette_idx) = exact_matches[idx] {
                    output[idx] = palette_idx;
                    if options.exact_absorb_error {
                        // Absorb: discard accumulated error
                        continue;
                    }
                    // Pass-through: compute and diffuse error normally
                    let accumulated = error_buf.get_accumulated(x);
                    let nearest_linear = palette.actual_linear(palette_idx as usize);
                    let error = [
                        clamp_channel(image[idx].r + accumulated[0], options.error_clamp)
                            - nearest_linear.r,
                        clamp_channel(image[idx].g + accumulated[1], options.error_clamp)
                            - nearest_linear.g,
                        clamp_channel(image[idx].b + accumulated[2], options.error_clamp)
                            - nearest_linear.b,
                    ];

                    diffuse_jittered(
                        &error,
                        x,
                        y,
                        width,
                        height,
                        reverse,
                        options.noise_scale,
                        &mut error_buf,
                    );
                    continue;
                }

                // Add accumulated error to input pixel
                let accumulated = error_buf.get_accumulated(x);
                let pixel = LinearRgb::new(
                    clamp_channel(image[idx].r + accumulated[0], options.error_clamp),
                    clamp_channel(image[idx].g + accumulated[1], options.error_clamp),
                    clamp_channel(image[idx].b + accumulated[2], options.error_clamp),
                );

                // Chroma of original pixel (for chromatic damping)
                let original_oklab = Oklab::from(image[idx]);
                let original_chroma_sq =
                    original_oklab.a * original_oklab.a + original_oklab.b * original_oklab.b;

                // Match in OKLab (perceptual)
                let oklab = Oklab::from(pixel);
                let (nearest_idx, _dist) = palette.find_nearest(oklab);
                output[idx] = nearest_idx as u8;

                // Error in Linear RGB (physical)
                let nearest_linear = palette.actual_linear(nearest_idx);
                let error = [
                    pixel.r - nearest_linear.r,
                    pixel.g - nearest_linear.g,
                    pixel.b - nearest_linear.b,
                ];

                // Optional chromatic error damping
                let damped_error = if options.chroma_clamp < f32::INFINITY {
                    let ratio_sq = (original_chroma_sq / threshold_sq).min(1.0);
                    let alpha = ratio_sq * ratio_sq;
                    let err_mean = (error[0] + error[1] + error[2]) * (1.0 / 3.0);
                    [
                        err_mean + alpha * (error[0] - err_mean),
                        err_mean + alpha * (error[1] - err_mean),
                        err_mean + alpha * (error[2] - err_mean),
                    ]
                } else {
                    error
                };

                // Diffuse with jittered weights
                diffuse_jittered(
                    &damped_error,
                    x,
                    y,
                    width,
                    height,
                    reverse,
                    options.noise_scale,
                    &mut error_buf,
                );
            }

            error_buf.advance_row();
        }

        output
    }
}

/// Diffuse error to neighbors using jittered Floyd-Steinberg weights.
///
/// Rotates weight between the "right" and "below" directions using
/// blue noise, keeping total propagation at 100%.
#[inline]
#[allow(clippy::too_many_arguments)]
fn diffuse_jittered(
    error: &[f32; 3],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    reverse: bool,
    jitter_scale: f32,
    error_buf: &mut ErrorBuffer,
) {
    // Blue noise jitter: rotate weight between right and below
    let noise = BLUE_NOISE_64[y % 64][x % 64];
    let alpha = (noise as f32 - 128.0) / 256.0; // -0.5..+0.5

    // Jittered weights (float, will be divided by 16)
    let w_right = 7.0 - alpha * jitter_scale;
    let w_below_left = 3.0_f32;
    let w_below = 5.0 + alpha * jitter_scale;
    let w_below_right = 1.0_f32;

    // Floyd-Steinberg entries: (dx, dy, weight)
    let entries: [(i32, usize, f32); 4] = [
        (1, 0, w_right),
        (-1, 1, w_below_left),
        (0, 1, w_below),
        (1, 1, w_below_right),
    ];

    for (dx, dy, weight) in entries {
        let effective_dx = if reverse { -dx } else { dx };
        let nx = x as i32 + effective_dx;

        if nx >= 0 && (nx as usize) < width {
            let ny = y + dy;
            if ny < height {
                let scaled_error = [
                    error[0] * weight / 16.0,
                    error[1] * weight / 16.0,
                    error[2] * weight / 16.0,
                ];
                error_buf.add_error(nx as usize, dy, scaled_error);
            }
        }
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

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 16];

        let result = FloydSteinbergNoise.dither(&image, 4, 4, &palette, &options);

        assert_eq!(result.len(), 16);
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
    fn test_pure_black() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let image = vec![black; 4];

        let result = FloydSteinbergNoise.dither(&image, 2, 2, &palette, &options);
        assert!(result.iter().all(|&x| x == 0), "Pure black should all be 0");
    }

    #[test]
    fn test_pure_white() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        let white = LinearRgb::new(1.0, 1.0, 1.0);
        let image = vec![white; 4];

        let result = FloydSteinbergNoise.dither(&image, 2, 2, &palette, &options);
        assert!(result.iter().all(|&x| x == 1), "Pure white should all be 1");
    }

    #[test]
    fn test_100_percent_propagation() {
        let palette = make_bw_palette();
        let options = DitherOptions::new().serpentine(false);

        let width = 64;
        let height = 64;
        let gray_value = 0.3_f32;
        let image: Vec<LinearRgb> = (0..width * height)
            .map(|_| LinearRgb::new(gray_value, gray_value, gray_value))
            .collect();

        let result = FloydSteinbergNoise.dither(&image, width, height, &palette, &options);

        let white_count = result.iter().filter(|&&x| x == 1).count();
        let white_ratio = white_count as f32 / (width * height) as f32;

        assert!(
            (white_ratio - gray_value).abs() < 0.05,
            "Expected ~{} white ratio, got {}",
            gray_value,
            white_ratio
        );
    }

    #[test]
    fn test_deterministic() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 64];

        let r1 = FloydSteinbergNoise.dither(&image, 8, 8, &palette, &options);
        let r2 = FloydSteinbergNoise.dither(&image, 8, 8, &palette, &options);
        assert_eq!(r1, r2, "Same input should produce same output");
    }

    #[test]
    fn test_differs_from_plain_floyd_steinberg() {
        use crate::dither::FloydSteinberg;

        let palette = make_bw_palette();
        let options = DitherOptions::new();

        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 256];

        let plain = FloydSteinberg.dither(&image, 16, 16, &palette, &options);
        let noise = FloydSteinbergNoise.dither(&image, 16, 16, &palette, &options);

        // Should produce different patterns (that's the whole point)
        assert_ne!(plain, noise, "Jittered FS should differ from plain FS");
    }

    #[test]
    fn test_output_in_palette_range() {
        let palette = make_7_color_palette();
        let options = DitherOptions::new();

        let image: Vec<LinearRgb> = (0..100)
            .map(|i| {
                let r = ((i * 7) % 256) as f32 / 255.0;
                let g = ((i * 13) % 256) as f32 / 255.0;
                let b = ((i * 23) % 256) as f32 / 255.0;
                LinearRgb::from(Srgb::new(r, g, b))
            })
            .collect();

        let result = FloydSteinbergNoise.dither(&image, 10, 10, &palette, &options);

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
    fn test_preserves_exact_matches() {
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(128, 128, 128),
            Srgb::from_u8(255, 255, 255),
        ];
        let palette = Palette::new(&colors, None).unwrap();
        let options = DitherOptions::new().preserve_exact_matches(true);

        let exact_gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![exact_gray; 4];

        let result = FloydSteinbergNoise.dither(&image, 2, 2, &palette, &options);
        assert!(
            result.iter().all(|&x| x == 1),
            "Exact gray matches should preserve"
        );
    }

    #[test]
    fn test_empty_image() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let image: Vec<LinearRgb> = vec![];

        let result = FloydSteinbergNoise.dither(&image, 0, 0, &palette, &options);
        assert!(result.is_empty(), "Empty input should produce empty output");
    }

    #[test]
    fn test_jitter_weights_sum_to_16() {
        // Verify the jitter formula preserves total weight = 16
        for noise_val in 0..=255u8 {
            let alpha = (noise_val as f32 - 128.0) / 256.0;
            let w_right = 7.0 - alpha * JITTER_SCALE;
            let w_below_left = 3.0_f32;
            let w_below = 5.0 + alpha * JITTER_SCALE;
            let w_below_right = 1.0_f32;
            let total = w_right + w_below_left + w_below + w_below_right;
            assert!(
                (total - 16.0).abs() < 1e-5,
                "Weights should sum to 16, got {} for noise={}",
                total,
                noise_val
            );
        }
    }

    #[test]
    fn test_jitter_weights_stay_positive() {
        // Verify no weight goes negative with our jitter scale
        for noise_val in 0..=255u8 {
            let alpha = (noise_val as f32 - 128.0) / 256.0;
            let w_right = 7.0 - alpha * JITTER_SCALE;
            let w_below = 5.0 + alpha * JITTER_SCALE;
            assert!(
                w_right > 0.0,
                "Right weight should be positive, got {} for noise={}",
                w_right,
                noise_val
            );
            assert!(
                w_below > 0.0,
                "Below weight should be positive, got {} for noise={}",
                w_below,
                noise_val
            );
        }
    }
}
