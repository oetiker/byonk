//! Dithering algorithm for e-ink displays.
//!
//! Provides palette-aware error diffusion dithering that works for both
//! grayscale and color e-ink panels. The caller supplies an RGB palette
//! and receives per-pixel palette indices.

use crate::rendering::blue_noise::BLUE_NOISE_64;

/// Default noise strength for blue noise modulation (0-128 scale).
/// Value of 14 works well for UI content with clean edges.
const DEFAULT_NOISE_STRENGTH: i16 = 14;

/// Palette-aware blue-noise-modulated error diffusion dithering.
///
/// This algorithm improves upon standard Floyd-Steinberg by:
/// 1. Using serpentine (boustrophedon) scanning to reduce directional artifacts
/// 2. Modulating the quantization threshold with blue noise to break up patterns
/// 3. Computing error from the un-noised value to preserve energy balance
/// 4. Preserving pixels already at exact palette colors (no dithering for solid fills)
///
/// Works uniformly for greyscale and color palettes — a 4-grey palette
/// `[(0,0,0), (85,85,85), (170,170,170), (255,255,255)]` produces equivalent
/// results to a dedicated greyscale ditherer.
///
/// # Arguments
/// * `rgba_data` - Input RGBA pixels (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `palette` - Target palette as RGB tuples
/// * `noise_strength` - Optional noise strength (0-128), None = use default (14)
///
/// # Returns
/// Vec of palette indices (0..palette.len()-1), one per pixel.
pub fn palette_dither(
    rgba_data: &[u8],
    width: u32,
    height: u32,
    palette: &[(u8, u8, u8)],
    noise_strength: Option<i16>,
) -> Vec<u8> {
    // Detect greyscale palette: all entries have R==G==B.
    // For greyscale targets, convert source pixels to luminance first
    // so that colors map to perceptually correct grey levels (BT.709).
    let is_grey = palette.iter().all(|&(r, g, b)| r == g && g == b);

    if is_grey {
        let grey_data = rgba_to_greyscale(rgba_data);
        color_dither(&grey_data, width, height, palette, noise_strength)
    } else {
        color_dither(rgba_data, width, height, palette, noise_strength)
    }
}

/// BT.709 luminance from RGB (fixed-point: result 0-255).
#[inline]
fn luminance(r: i32, g: i32, b: i32) -> i32 {
    // BT.709: Y = 0.2126*R + 0.7152*G + 0.0722*B
    // Fixed-point with 16-bit precision: 13933 + 46871 + 4732 = 65536
    (r * 13933 + g * 46871 + b * 4732 + 32768) >> 16
}

/// Convert RGBA pixels to greyscale RGBA using BT.709 luminance.
/// Alpha is preserved; RGB channels are replaced with the luminance value.
fn rgba_to_greyscale(rgba_data: &[u8]) -> Vec<u8> {
    rgba_data
        .chunks(4)
        .flat_map(|pixel| {
            let y = luminance(pixel[0] as i32, pixel[1] as i32, pixel[2] as i32) as u8;
            [y, y, y, pixel[3]]
        })
        .collect()
}

/// Palette dithering in RGB space with blue-noise-modulated error diffusion.
/// Works for both greyscale and color palettes — for greyscale palettes,
/// the caller pre-converts pixels to luminance via `rgba_to_greyscale`.
fn color_dither(
    rgba_data: &[u8],
    width: u32,
    height: u32,
    palette: &[(u8, u8, u8)],
    noise_strength: Option<i16>,
) -> Vec<u8> {
    let noise_strength = noise_strength.unwrap_or(DEFAULT_NOISE_STRENGTH);
    let w = width as usize;
    let h = height as usize;

    // Convert RGBA to RGB, alpha-compositing against white.
    // Also detect pixels that exactly match a palette entry.
    let mut buf_r: Vec<i16> = Vec::with_capacity(w * h);
    let mut buf_g: Vec<i16> = Vec::with_capacity(w * h);
    let mut buf_b: Vec<i16> = Vec::with_capacity(w * h);
    let mut exact_pixel: Vec<Option<u8>> = Vec::with_capacity(w * h);

    for pixel in rgba_data.chunks(4) {
        let (r, g, b, a) = (
            pixel[0] as i16,
            pixel[1] as i16,
            pixel[2] as i16,
            pixel[3] as i16,
        );
        let (cr, cg, cb) = if a == 0 {
            (255i16, 255, 255)
        } else if a == 255 {
            (r, g, b)
        } else {
            // Alpha composite against white
            (
                (r * a + 255 * (255 - a)) / 255,
                (g * a + 255 * (255 - a)) / 255,
                (b * a + 255 * (255 - a)) / 255,
            )
        };

        // Check if this pixel exactly matches a palette entry
        let exact = palette
            .iter()
            .position(|&(pr, pg, pb)| cr == pr as i16 && cg == pg as i16 && cb == pb as i16)
            .map(|i| i as u8);

        buf_r.push(cr);
        buf_g.push(cg);
        buf_b.push(cb);
        exact_pixel.push(exact);
    }

    let mut indices = vec![0u8; w * h];

    for y in 0..h {
        let going_right = y % 2 == 0;
        let xs: Box<dyn Iterator<Item = usize>> = if going_right {
            Box::new(0..w)
        } else {
            Box::new((0..w).rev())
        };

        for x in xs {
            let idx = y * w + x;

            // Preserve pixels that exactly match a palette color
            if let Some(pi) = exact_pixel[idx] {
                indices[idx] = pi;
                continue; // No error diffusion for exact palette matches
            }

            let or = buf_r[idx];
            let og = buf_g[idx];
            let ob = buf_b[idx];

            // Blue noise modulation
            let noise = BLUE_NOISE_64[y % 64][x % 64] as i16 - 128;
            let offset = (noise * noise_strength) / 128;

            let nr = (or + offset).clamp(0, 255);
            let ng = (og + offset).clamp(0, 255);
            let nb = (ob + offset).clamp(0, 255);

            // Find nearest palette color by Euclidean distance
            let best_idx = nearest_palette_entry(nr, ng, nb, palette);
            indices[idx] = best_idx;

            // Error from un-noised value (preserves energy)
            // Use i32 throughout to avoid overflow on accumulated errors.
            let (pr, pg, pb) = palette[best_idx as usize];
            let err_r = or as i32 - pr as i32;
            let err_g = og as i32 - pg as i32;
            let err_b = ob as i32 - pb as i32;

            // Floyd-Steinberg error distribution
            // Don't distribute into pixels at exact palette colors.
            let diffuse = |buf_r: &mut [i16],
                           buf_g: &mut [i16],
                           buf_b: &mut [i16],
                           bi: usize,
                           weight: i32| {
                buf_r[bi] = (buf_r[bi] as i32 + err_r * weight / 16).clamp(-512, 767) as i16;
                buf_g[bi] = (buf_g[bi] as i32 + err_g * weight / 16).clamp(-512, 767) as i16;
                buf_b[bi] = (buf_b[bi] as i32 + err_b * weight / 16).clamp(-512, 767) as i16;
            };

            if going_right {
                if x + 1 < w && exact_pixel[idx + 1].is_none() {
                    diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + 1, 7);
                }
                if y + 1 < h {
                    if x > 0 && exact_pixel[idx + w - 1].is_none() {
                        diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + w - 1, 3);
                    }
                    if exact_pixel[idx + w].is_none() {
                        diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + w, 5);
                    }
                    if x + 1 < w && exact_pixel[idx + w + 1].is_none() {
                        diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + w + 1, 1);
                    }
                }
            } else {
                if x > 0 && exact_pixel[idx - 1].is_none() {
                    diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx - 1, 7);
                }
                if y + 1 < h {
                    if x + 1 < w && exact_pixel[idx + w + 1].is_none() {
                        diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + w + 1, 3);
                    }
                    if exact_pixel[idx + w].is_none() {
                        diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + w, 5);
                    }
                    if x > 0 && exact_pixel[idx + w - 1].is_none() {
                        diffuse(&mut buf_r, &mut buf_g, &mut buf_b, idx + w - 1, 1);
                    }
                }
            }
        }
    }

    indices
}

/// Find the palette entry nearest to an RGB value by Euclidean distance.
#[inline]
fn nearest_palette_entry(r: i16, g: i16, b: i16, palette: &[(u8, u8, u8)]) -> u8 {
    let mut best_idx = 0u8;
    let mut best_dist = i32::MAX;
    for (pi, &(pr, pg, pb)) in palette.iter().enumerate() {
        let dr = r as i32 - pr as i32;
        let dg = g as i32 - pg as i32;
        let db = b as i32 - pb as i32;
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_dist {
            best_dist = dist;
            best_idx = pi as u8;
        }
    }
    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: make RGBA data from a single grey value
    fn grey_rgba(value: u8, count: usize) -> Vec<u8> {
        (0..count)
            .flat_map(|_| [value, value, value, 255])
            .collect()
    }

    /// Standard 4-grey palette
    const GREY4: [(u8, u8, u8); 4] = [(0, 0, 0), (85, 85, 85), (170, 170, 170), (255, 255, 255)];

    #[test]
    fn test_pure_black() {
        let data = grey_rgba(0, 100);
        let result = palette_dither(&data, 10, 10, &GREY4, None);
        assert!(result.iter().all(|&v| v == 0)); // index 0 = black
    }

    #[test]
    fn test_pure_white() {
        let data = grey_rgba(255, 100);
        let result = palette_dither(&data, 10, 10, &GREY4, None);
        assert!(result.iter().all(|&v| v == 3)); // index 3 = white
    }

    #[test]
    fn test_outputs_valid_indices() {
        // Gradient
        let data: Vec<u8> = (0..100)
            .flat_map(|i| {
                let v = (i * 255 / 100) as u8;
                [v, v, v, 255]
            })
            .collect();
        let result = palette_dither(&data, 10, 10, &GREY4, None);
        for idx in result {
            assert!(idx < 4, "Invalid palette index: {idx}");
        }
    }

    #[test]
    fn test_serpentine_coverage() {
        let data = grey_rgba(128, 64 * 64);
        let result = palette_dither(&data, 64, 64, &GREY4, None);
        let unique: std::collections::HashSet<_> = result.iter().collect();
        assert!(unique.len() > 1, "Dithering should produce variation");
    }

    #[test]
    fn test_exact_palette_preservation() {
        // Pixels at exact palette colors should be preserved
        let data: Vec<u8> = [0u8, 85, 170, 255]
            .iter()
            .flat_map(|&v| [v, v, v, 255])
            .collect();
        let result = palette_dither(&data, 4, 1, &GREY4, None);
        assert_eq!(result, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_color_palette() {
        // 4-color e-ink: black, white, red, yellow
        let palette = [(0, 0, 0), (255, 255, 255), (255, 0, 0), (255, 255, 0)];
        // Pure red pixel
        let data = vec![255, 0, 0, 255];
        let result = palette_dither(&data, 1, 1, &palette, None);
        assert_eq!(result[0], 2); // index 2 = red
    }

    #[test]
    fn test_noise_strength_zero() {
        let data: Vec<u8> = (0..100)
            .flat_map(|i| {
                let v = (i * 255 / 100) as u8;
                [v, v, v, 255]
            })
            .collect();
        let result = palette_dither(&data, 10, 10, &GREY4, Some(0));
        for idx in result {
            assert!(idx < 4, "Invalid palette index: {idx}");
        }
    }

    #[test]
    fn test_transparent_becomes_white() {
        // Fully transparent → white background → palette index for white
        let data = vec![0, 0, 0, 0]; // transparent
        let result = palette_dither(&data, 1, 1, &GREY4, None);
        assert_eq!(result[0], 3); // white
    }

    #[test]
    fn test_luminance_function() {
        // Pure white
        assert_eq!(luminance(255, 255, 255), 255);
        // Pure black
        assert_eq!(luminance(0, 0, 0), 0);
        // Pure red: BT.709 → ~54
        let red_lum = luminance(255, 0, 0);
        assert!(red_lum >= 50 && red_lum <= 58, "Red luminance: {red_lum}");
        // Pure green: BT.709 → ~182
        let green_lum = luminance(0, 255, 0);
        assert!(
            green_lum >= 178 && green_lum <= 186,
            "Green luminance: {green_lum}"
        );
        // Pure blue: BT.709 → ~18
        let blue_lum = luminance(0, 0, 255);
        assert!(
            blue_lum >= 15 && blue_lum <= 22,
            "Blue luminance: {blue_lum}"
        );
    }

    #[test]
    fn test_grey_palette_uses_luminance() {
        // Red pixel on grey palette: should map based on luminance (~54),
        // which is nearest to black (0) or dark grey (85).
        // With no noise, 54 is closer to 85 than to 0, so expect index 1 (dark grey).
        let data = vec![255, 0, 0, 255];
        let result = palette_dither(&data, 1, 1, &GREY4, Some(0));
        assert_eq!(
            result[0], 1,
            "Red on grey palette should be dark grey (luminance ~54 → 85)"
        );

        // Green pixel: luminance ~182, nearest to 170 (light grey, index 2)
        let data = vec![0, 255, 0, 255];
        let result = palette_dither(&data, 1, 1, &GREY4, Some(0));
        assert_eq!(
            result[0], 2,
            "Green on grey palette should be light grey (luminance ~182 → 170)"
        );

        // Blue pixel: luminance ~18, nearest to 0 (black, index 0)
        let data = vec![0, 0, 255, 255];
        let result = palette_dither(&data, 1, 1, &GREY4, Some(0));
        assert_eq!(
            result[0], 0,
            "Blue on grey palette should be black (luminance ~18 → 0)"
        );
    }

    #[test]
    fn test_grey_detection() {
        // Grey palette should be detected
        assert!(GREY4.iter().all(|&(r, g, b)| r == g && g == b));
        // Color palette should not
        let color_palette = [(0, 0, 0), (255, 255, 255), (255, 0, 0), (255, 255, 0)];
        assert!(!color_palette.iter().all(|&(r, g, b)| r == g && g == b));
    }
}
