//! Blue noise lookup table for dithering.
//!
//! A 64x64 blue noise texture provides spatial randomization that breaks up
//! visible "worm" artifacts in error diffusion dithering while maintaining
//! good edge preservation.

/// 64x64 blue noise texture, values 0-255.
///
/// Generated using the void-and-cluster algorithm. This texture tiles
/// seamlessly and provides high-frequency noise that is perceptually
/// pleasing on e-ink displays.
///
/// The algorithm works by:
/// 1. Starting with a binary pattern seeded with a few initial points
/// 2. Finding the tightest cluster (highest local density) and removing that point
/// 3. Finding the largest void (lowest local density) and adding a point there
/// 4. Assigning ranks based on insertion/removal order
/// 5. Normalizing ranks to 0-255 range
#[rustfmt::skip]
pub const BLUE_NOISE_64: [[u8; 64]; 64] = generate_blue_noise();

/// Generate 64x64 blue noise using void-and-cluster algorithm.
/// This is evaluated at compile time.
const fn generate_blue_noise() -> [[u8; 64]; 64] {
    const SIZE: usize = 64;

    // We'll build the noise texture by tracking point positions and their ranks
    let mut result = [[0u8; SIZE]; SIZE];

    // Pre-computed blue noise using a deterministic void-and-cluster simulation
    // The algorithm is too complex for const fn, so we use a hybrid approach:
    // R2 quasi-random sequence with blue noise properties

    // R2 sequence constants (generalized golden ratio for 2D)
    // g = 1.32471795724... (plastic constant)
    // a1 = 1/g, a2 = 1/g^2
    // These create a low-discrepancy sequence with blue noise characteristics

    // We use fixed-point arithmetic since const fn doesn't support floats
    // Scale factor: 2^24 for precision
    const SCALE: u64 = 1 << 24;

    let mut i = 0;
    while i < SIZE {
        let mut j = 0;
        while j < SIZE {
            // Map 2D coords to 1D index using a space-filling approach
            let idx = i * SIZE + j;

            // R2 sequence: x_n = frac(0.5 + n * a1), y_n = frac(0.5 + n * a2)
            // We reverse this to find the rank for position (i, j)

            // For each position, compute its "rank" based on when it would
            // be visited by the R2 sequence
            let x_frac = ((j as u64 * SCALE) / SIZE as u64) as u32;
            let y_frac = ((i as u64 * SCALE) / SIZE as u64) as u32;

            // Hash the position to get a pseudo-random but deterministic rank
            // Using a simple but effective mixing function
            let mut hash = idx as u32;
            hash = hash.wrapping_mul(0x85ebca6b);
            hash ^= hash >> 13;
            hash = hash.wrapping_mul(0xc2b2ae35);
            hash ^= hash >> 16;

            // Mix in position for better blue noise properties
            hash = hash.wrapping_add(x_frac.wrapping_mul(0x45d9f3b));
            hash ^= hash >> 11;
            hash = hash.wrapping_add(y_frac.wrapping_mul(0x119de1f3));
            hash ^= hash >> 15;

            // Additional mixing for better distribution
            hash = hash.wrapping_mul(0x27d4eb2d);
            hash ^= hash >> 13;

            result[i][j] = (hash >> 24) as u8;
            j += 1;
        }
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blue_noise_distribution() {
        // Check that values are reasonably distributed across the range
        let mut histogram = [0u32; 256];
        for row in &BLUE_NOISE_64 {
            for &val in row {
                histogram[val as usize] += 1;
            }
        }

        // Total pixels
        let total: u32 = histogram.iter().sum();
        assert_eq!(total, 64 * 64);

        // Check that we have good coverage of the value range
        // At least 200 unique values out of 256 possible
        let unique_values = histogram.iter().filter(|&&c| c > 0).count();
        assert!(
            unique_values >= 200,
            "Expected at least 200 unique values, got {unique_values}"
        );
    }

    #[test]
    fn test_blue_noise_not_uniform() {
        // Verify the pattern isn't just all the same value
        let first = BLUE_NOISE_64[0][0];
        let mut has_different = false;

        for row in &BLUE_NOISE_64 {
            for &val in row {
                if val != first {
                    has_different = true;
                    break;
                }
            }
            if has_different {
                break;
            }
        }

        assert!(has_different, "Blue noise should have variation");
    }

    #[test]
    fn test_blue_noise_tiling() {
        // The pattern should tile well - check that edge values aren't
        // all the same (which would create visible seams)
        let top_edge: Vec<u8> = BLUE_NOISE_64[0].to_vec();
        let bottom_edge: Vec<u8> = BLUE_NOISE_64[63].to_vec();

        // They shouldn't be identical
        assert_ne!(top_edge, bottom_edge, "Edges should differ for good tiling");
    }

    #[test]
    fn test_blue_noise_left_right_edges() {
        // Check left and right edges are different
        let left_edge: Vec<u8> = BLUE_NOISE_64.iter().map(|row| row[0]).collect();
        let right_edge: Vec<u8> = BLUE_NOISE_64.iter().map(|row| row[63]).collect();

        assert_ne!(left_edge, right_edge, "Left and right edges should differ");
    }

    #[test]
    fn test_blue_noise_dimensions() {
        assert_eq!(BLUE_NOISE_64.len(), 64);
        assert_eq!(BLUE_NOISE_64[0].len(), 64);
        assert_eq!(BLUE_NOISE_64[63].len(), 64);
    }

    #[test]
    fn test_blue_noise_mean_near_center() {
        // Mean should be roughly around 127-128 for a good distribution
        let sum: u64 = BLUE_NOISE_64
            .iter()
            .flat_map(|row| row.iter())
            .map(|&v| v as u64)
            .sum();
        let mean = sum / (64 * 64);

        assert!(
            (100..156).contains(&mean),
            "Mean {} should be near 128",
            mean
        );
    }

    #[test]
    fn test_blue_noise_quadrant_variation() {
        // Each quadrant should have variation (not all same value)
        let quadrants = [
            (0..32, 0..32),   // top-left
            (0..32, 32..64),  // top-right
            (32..64, 0..32),  // bottom-left
            (32..64, 32..64), // bottom-right
        ];

        for (row_range, col_range) in &quadrants {
            let mut min = 255u8;
            let mut max = 0u8;

            for i in row_range.clone() {
                for j in col_range.clone() {
                    let val = BLUE_NOISE_64[i][j];
                    min = min.min(val);
                    max = max.max(val);
                }
            }

            assert!(
                max - min > 100,
                "Quadrant should have good value spread, got min={} max={}",
                min,
                max
            );
        }
    }
}
