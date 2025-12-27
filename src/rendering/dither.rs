/// Floyd-Steinberg dithering to reduce bit depth for e-ink displays.
///
/// For 2-bit (4 grayscale levels), the output values are:
/// - Level 0: 0 (black)
/// - Level 1: 85
/// - Level 2: 170
/// - Level 3: 255 (white)
pub fn floyd_steinberg(gray_data: &[u8], width: u32, height: u32, levels: u8) -> Vec<u8> {
    let step = 255.0 / (levels - 1) as f32;

    // Working buffer (need floats for error diffusion)
    let mut buffer: Vec<f32> = gray_data.iter().map(|&v| v as f32).collect();
    let w = width as usize;
    let h = height as usize;

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let old_pixel = buffer[idx];

            // Quantize to nearest level
            let level = (old_pixel / step).round().clamp(0.0, (levels - 1) as f32);
            let new_pixel = level * step;
            buffer[idx] = new_pixel;

            let error = old_pixel - new_pixel;

            // Distribute error to neighbors (Floyd-Steinberg pattern)
            //     X   7/16
            // 3/16 5/16 1/16
            if x + 1 < w {
                buffer[idx + 1] += error * 7.0 / 16.0;
            }
            if y + 1 < h {
                if x > 0 {
                    buffer[idx + w - 1] += error * 3.0 / 16.0;
                }
                buffer[idx + w] += error * 5.0 / 16.0;
                if x + 1 < w {
                    buffer[idx + w + 1] += error * 1.0 / 16.0;
                }
            }
        }
    }

    buffer
        .iter()
        .map(|&v| v.clamp(0.0, 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dither_pure_black() {
        let data = vec![0u8; 100];
        let result = floyd_steinberg(&data, 10, 10, 4);
        // Pure black should remain black
        assert!(result.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_dither_pure_white() {
        let data = vec![255u8; 100];
        let result = floyd_steinberg(&data, 10, 10, 4);
        // Pure white should remain white
        assert!(result.iter().all(|&v| v == 255));
    }

    #[test]
    fn test_dither_outputs_valid_levels() {
        // Create a gradient
        let data: Vec<u8> = (0..100).map(|i| (i * 255 / 100) as u8).collect();
        let result = floyd_steinberg(&data, 10, 10, 4);

        // All outputs should be one of the 4 valid levels
        let valid_levels = [0u8, 85, 170, 255];
        for pixel in result {
            assert!(valid_levels.contains(&pixel), "Invalid level: {pixel}");
        }
    }
}
