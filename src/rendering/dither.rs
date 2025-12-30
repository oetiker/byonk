//! Dithering algorithms for e-ink displays.
//!
//! Provides error diffusion dithering to reduce bit depth while maintaining
//! visual quality on grayscale e-ink screens.

use crate::rendering::blue_noise::BLUE_NOISE_64;

/// Default noise strength for blue noise modulation (0-128 scale).
/// Value of 14 works well for UI content with clean edges.
const DEFAULT_NOISE_STRENGTH: i16 = 14;

/// Blue-noise-modulated error diffusion dithering.
///
/// This algorithm improves upon standard Floyd-Steinberg by:
/// 1. Using serpentine (boustrophedon) scanning to reduce directional artifacts
/// 2. Modulating the quantization threshold with blue noise to break up patterns
/// 3. Computing error from the un-noised value to preserve energy balance
///
/// For 2-bit (4 grayscale levels), the output values are:
/// - Level 0: 0 (black)
/// - Level 1: 85
/// - Level 2: 170
/// - Level 3: 255 (white)
///
/// # Arguments
/// * `gray_data` - Input grayscale pixels (0-255)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `levels` - Number of output levels (e.g., 4 for 2-bit)
/// * `noise_strength` - Optional noise strength (0-128), None = use default (14)
///
/// # Returns
/// Dithered grayscale data with values quantized to `levels` discrete values.
pub fn blue_noise_dither(
    gray_data: &[u8],
    width: u32,
    height: u32,
    levels: u8,
    noise_strength: Option<i16>,
) -> Vec<u8> {
    let noise_strength = noise_strength.unwrap_or(DEFAULT_NOISE_STRENGTH);
    let step = 255 / (levels - 1) as i16;

    // Working buffer with i16 to handle negative error values
    let mut buffer: Vec<i16> = gray_data.iter().map(|&v| v as i16).collect();
    let w = width as usize;
    let h = height as usize;

    for y in 0..h {
        let going_right = y % 2 == 0;

        if going_right {
            for x in 0..w {
                process_pixel(x, y, &mut buffer, w, h, levels, step, noise_strength, true);
            }
        } else {
            for x in (0..w).rev() {
                process_pixel(x, y, &mut buffer, w, h, levels, step, noise_strength, false);
            }
        }
    }

    // Convert back to u8
    buffer.iter().map(|&v| v.clamp(0, 255) as u8).collect()
}

/// Process a single pixel with blue-noise-modulated error diffusion.
#[inline]
#[allow(clippy::too_many_arguments)]
fn process_pixel(
    x: usize,
    y: usize,
    buffer: &mut [i16],
    w: usize,
    h: usize,
    levels: u8,
    step: i16,
    noise_strength: i16,
    going_right: bool,
) {
    let idx = y * w + x;
    let old_val = buffer[idx];

    // Blue noise modulation of threshold
    // Noise is centered around 0 (-128 to +127)
    let noise = BLUE_NOISE_64[y % 64][x % 64] as i16 - 128;
    let noise_offset = (noise * noise_strength) / 128;

    // Quantize with noise-modulated value
    let noised_val = (old_val + noise_offset).clamp(0, 255);
    let new_val = find_closest_level(noised_val as u8, levels, step as u8) as i16;

    // CRITICAL: Error is computed from UN-noised value
    // This preserves energy balance while the noise only affects quantization
    let error = old_val - new_val;

    // Store quantized result
    buffer[idx] = new_val;

    // Distribute error using Floyd-Steinberg coefficients
    // Pattern depends on scan direction:
    //
    // Left-to-right:      Right-to-left:
    //     X   7/16        7/16   X
    // 3/16 5/16 1/16      1/16 5/16 3/16

    if going_right {
        // Forward direction
        if x + 1 < w {
            buffer[idx + 1] += error * 7 / 16;
        }
        if y + 1 < h {
            if x > 0 {
                buffer[idx + w - 1] += error * 3 / 16;
            }
            buffer[idx + w] += error * 5 / 16;
            if x + 1 < w {
                buffer[idx + w + 1] += error / 16;
            }
        }
    } else {
        // Reverse direction - mirror the pattern
        if x > 0 {
            buffer[idx - 1] += error * 7 / 16;
        }
        if y + 1 < h {
            if x + 1 < w {
                buffer[idx + w + 1] += error * 3 / 16;
            }
            buffer[idx + w] += error * 5 / 16;
            if x > 0 {
                buffer[idx + w - 1] += error / 16;
            }
        }
    }
}

/// Find the closest palette level for a given pixel value.
#[inline]
fn find_closest_level(value: u8, levels: u8, step: u8) -> u8 {
    // Round to nearest level
    let level = ((value as u16 + step as u16 / 2) / step as u16).min((levels - 1) as u16);
    (level as u8) * step
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for blue_noise_dither

    #[test]
    fn test_blue_noise_pure_black() {
        let data = vec![0u8; 100];
        let result = blue_noise_dither(&data, 10, 10, 4, None);
        // Pure black should remain black
        assert!(result.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_blue_noise_pure_white() {
        let data = vec![255u8; 100];
        let result = blue_noise_dither(&data, 10, 10, 4, None);
        // Pure white should remain white
        assert!(result.iter().all(|&v| v == 255));
    }

    #[test]
    fn test_blue_noise_outputs_valid_levels() {
        // Create a gradient
        let data: Vec<u8> = (0..100).map(|i| (i * 255 / 100) as u8).collect();
        let result = blue_noise_dither(&data, 10, 10, 4, None);

        // All outputs should be one of the 4 valid levels
        let valid_levels = [0u8, 85, 170, 255];
        for pixel in result {
            assert!(valid_levels.contains(&pixel), "Invalid level: {pixel}");
        }
    }

    #[test]
    fn test_blue_noise_serpentine_coverage() {
        // Test that serpentine scanning processes all pixels
        let data = vec![128u8; 64 * 64]; // 64x64 mid-gray
        let result = blue_noise_dither(&data, 64, 64, 4, None);

        // Should have some dithering pattern (not all same value)
        let unique_vals: std::collections::HashSet<_> = result.iter().collect();
        assert!(unique_vals.len() > 1, "Dithering should produce variation");
    }

    #[test]
    fn test_noise_strength_zero() {
        // With noise_strength=0, should behave like standard dithering
        let data: Vec<u8> = (0..100).map(|i| (i * 255 / 100) as u8).collect();
        let result = blue_noise_dither(&data, 10, 10, 4, Some(0));

        let valid_levels = [0u8, 85, 170, 255];
        for pixel in result {
            assert!(valid_levels.contains(&pixel), "Invalid level: {pixel}");
        }
    }

    #[test]
    fn test_custom_noise_strength() {
        let data = vec![128u8; 100];
        // Should not panic with various noise strengths
        let _ = blue_noise_dither(&data, 10, 10, 4, Some(0));
        let _ = blue_noise_dither(&data, 10, 10, 4, Some(14));
        let _ = blue_noise_dither(&data, 10, 10, 4, Some(64));
        let _ = blue_noise_dither(&data, 10, 10, 4, Some(128));
    }
}
