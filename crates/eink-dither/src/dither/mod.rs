//! Error diffusion dithering algorithms.
//!
//! This module provides error diffusion dithering algorithms optimized for
//! e-ink displays with small color palettes (typically 7-16 colors).
//!
//! # Algorithms
//!
//! Multiple diffusion kernels are available:
//!
//! - **Atkinson**: 75% error propagation, ideal for small palettes (default)
//! - **Floyd-Steinberg**: Classic algorithm, 100% propagation
//! - **Jarvis-Judice-Ninke**: Large kernel, smoother gradients
//! - **Sierra family**: Various speed/quality tradeoffs
//! - **Stucki**: Similar to JJN with sharper center weights
//! - **Burkes**: Simplified Stucki using 2 rows
//!
//! # Architecture
//!
//! All algorithms use `dither_with_kernel_noise` with per-algorithm kernel
//! constants. The noise_scale parameter controls blue noise jitter (0 = plain).

mod blue_noise_matrix;
mod kernel;
mod options;

pub use kernel::*;
pub use options::DitherOptions;

/// Dither algorithm selection for builder API.
///
/// Each variant maps to a specific error diffusion kernel with tuned defaults
/// for error_clamp and noise_scale.
///
/// # Example
///
/// ```
/// use eink_dither::{EinkDitherer, Palette, DitherAlgorithm, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
///
/// let ditherer = EinkDitherer::new(palette)
///     .algorithm(DitherAlgorithm::FloydSteinberg);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DitherAlgorithm {
    /// Atkinson error diffusion (75% propagation).
    ///
    /// Best for photographs with small palettes. Produces smooth gradients.
    #[default]
    Atkinson,

    /// Floyd-Steinberg error diffusion (100% propagation).
    ///
    /// Classic algorithm with full error propagation.
    FloydSteinberg,

    /// Jarvis-Judice-Ninke error diffusion (100% propagation, 12 neighbors).
    ///
    /// Large 3-row kernel with peak weight 7/48. The wide spread
    /// prevents oscillation artifacts on sparse chromatic palettes.
    JarvisJudiceNinke,

    /// Sierra (full) error diffusion (100% propagation, 10 neighbors).
    ///
    /// 3-row kernel with peak weight 5/32. Similar anti-oscillation
    /// properties to JJN with slightly fewer neighbors.
    Sierra,

    /// Sierra two-row error diffusion (100% propagation, 7 neighbors).
    ///
    /// 2-row kernel with peak weight 4/16 = 25%. Faster than full Sierra.
    SierraTwoRow,

    /// Sierra Lite error diffusion (100% propagation, 3 neighbors).
    ///
    /// Minimal 2-row kernel. Fastest Sierra variant.
    SierraLite,

    /// Stucki error diffusion (100% propagation, 12 neighbors).
    ///
    /// 3-row kernel similar to JJN but with higher center weights
    /// for slightly sharper results.
    Stucki,

    /// Burkes error diffusion (100% propagation, 7 neighbors).
    ///
    /// 2-row simplified variant of Stucki. Faster while maintaining
    /// wide error spread.
    Burkes,
}

impl DitherAlgorithm {
    /// Get the error diffusion kernel for this algorithm.
    pub fn kernel(&self) -> &'static Kernel {
        match self {
            Self::Atkinson => &ATKINSON,
            Self::FloydSteinberg => &FLOYD_STEINBERG,
            Self::JarvisJudiceNinke => &JARVIS_JUDICE_NINKE,
            Self::Sierra => &SIERRA,
            Self::SierraTwoRow => &SIERRA_TWO_ROW,
            Self::SierraLite => &SIERRA_LITE,
            Self::Stucki => &STUCKI,
            Self::Burkes => &BURKES,
        }
    }

    /// Get the per-algorithm default (error_clamp, noise_scale) for chromatic palettes.
    pub fn defaults(&self) -> (f32, f32) {
        match self {
            Self::Atkinson => (0.08, 0.0),
            Self::FloydSteinberg => (0.12, 4.0),
            Self::JarvisJudiceNinke => (0.03, 6.0),
            Self::Sierra => (0.10, 5.5),
            Self::SierraTwoRow => (0.10, 7.0),
            Self::SierraLite => (0.11, 2.5),
            Self::Stucki => (0.03, 6.0),
            Self::Burkes => (0.10, 7.0),
        }
    }
}

use crate::color::{LinearRgb, Oklab, Srgb};
use crate::palette::Palette;

/// Error buffer for efficient error diffusion.
///
/// Manages a sliding window of error rows, storing only the rows that
/// the diffusion kernel can reach (determined by `max_dy`). This avoids
/// allocating a full-image error buffer.
#[derive(Debug)]
pub struct ErrorBuffer {
    /// Error rows: rows[0] is current row, rows[1] is next, etc.
    rows: Vec<Vec<[f32; 3]>>,
    /// Image width
    width: usize,
}

impl ErrorBuffer {
    /// Create a new error buffer.
    pub fn new(width: usize, row_depth: usize) -> Self {
        Self {
            rows: (0..row_depth).map(|_| vec![[0.0; 3]; width]).collect(),
            width,
        }
    }

    /// Get accumulated error for a pixel in the current row.
    #[inline]
    pub fn get_accumulated(&self, x: usize) -> [f32; 3] {
        self.rows[0][x]
    }

    /// Add error to a future pixel.
    #[inline]
    pub fn add_error(&mut self, x: usize, row_offset: usize, error: [f32; 3]) {
        if x < self.width && row_offset < self.rows.len() {
            for c in 0..3 {
                self.rows[row_offset][x][c] += error[c];
            }
        }
    }

    /// Advance to the next row.
    pub fn advance_row(&mut self) {
        self.rows.rotate_left(1);
        if let Some(last) = self.rows.last_mut() {
            last.fill([0.0; 3]);
        }
    }
}

// ============================================================================
// Shared dithering infrastructure
// ============================================================================

/// Find exact byte-level match against official palette colors.
pub(crate) fn find_exact_match(pixel: LinearRgb, palette: &Palette) -> Option<u8> {
    if pixel.r < 0.0
        || pixel.r > 1.0
        || pixel.g < 0.0
        || pixel.g > 1.0
        || pixel.b < 0.0
        || pixel.b > 1.0
    {
        return None;
    }

    let srgb = Srgb::from(pixel);
    let pixel_bytes = srgb.to_bytes();

    for i in 0..palette.len() {
        if palette.official(i).to_bytes() == pixel_bytes {
            return Some(i as u8);
        }
    }
    None
}

/// Clamp a channel value with error to the valid range.
#[inline]
pub(crate) fn clamp_channel(value: f32, max_error: f32) -> f32 {
    value.clamp(-max_error, 1.0 + max_error)
}

/// Core error diffusion algorithm with blue noise jitter, parameterized by kernel.
///
/// This is the single dithering function used by all algorithms. When
/// `noise_scale` is 0, it behaves identically to a plain error diffusion
/// kernel (no jitter).
///
/// The jitter shifts weight between the kernel's `(1,0)` ("right") and
/// `(0,1)` ("below") entries per pixel using a blue noise value, breaking
/// directional "worm" artifacts while preserving total error propagation.
pub(crate) fn dither_with_kernel_noise(
    image: &[LinearRgb],
    width: usize,
    height: usize,
    palette: &Palette,
    kernel: &Kernel,
    options: &DitherOptions,
) -> Vec<u8> {
    use blue_noise_matrix::BLUE_NOISE_64;

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

    // Find the indices of the "right" (dx=1, dy=0) and "below" (dx=0, dy=1) entries
    let right_idx = kernel
        .entries
        .iter()
        .position(|&(dx, dy, _)| dx == 1 && dy == 0);
    let below_idx = kernel
        .entries
        .iter()
        .position(|&(dx, dy, _)| dx == 0 && dy == 1);

    let base_right = right_idx.map(|i| kernel.entries[i].2 as f32).unwrap_or(0.0);
    let base_below = below_idx.map(|i| kernel.entries[i].2 as f32).unwrap_or(0.0);

    // Create error buffer with depth = max_dy + 1
    let mut error_buf = ErrorBuffer::new(width, kernel.max_dy + 1);

    for y in 0..height {
        let reverse = options.serpentine && y % 2 == 1;

        let x_range: Box<dyn Iterator<Item = usize>> = if reverse {
            Box::new((0..width).rev())
        } else {
            Box::new(0..width)
        };

        for x in x_range {
            let idx = y * width + x;

            // Blue noise jitter for this pixel
            let noise = BLUE_NOISE_64[y % 64][x % 64];
            let alpha = (noise as f32 - 128.0) / 256.0; // -0.5..+0.5
            let shift = (alpha * options.noise_scale).clamp(-base_below, base_right);
            let w_right = base_right - shift;
            let w_below = base_below + shift;

            // Exact palette match handling
            if let Some(palette_idx) = exact_matches[idx] {
                output[idx] = palette_idx;
                if options.exact_absorb_error {
                    continue;
                }
                let accumulated = error_buf.get_accumulated(x);
                let pixel = LinearRgb::new(
                    clamp_channel(image[idx].r + accumulated[0], options.error_clamp),
                    clamp_channel(image[idx].g + accumulated[1], options.error_clamp),
                    clamp_channel(image[idx].b + accumulated[2], options.error_clamp),
                );
                let nearest_linear = palette.actual_linear(palette_idx as usize);
                let error = [
                    pixel.r - nearest_linear.r,
                    pixel.g - nearest_linear.g,
                    pixel.b - nearest_linear.b,
                ];
                let divisor = kernel.divisor as f32;
                for (entry_i, &(dx, dy, weight)) in kernel.entries.iter().enumerate() {
                    let effective_dx = if reverse { -dx } else { dx };
                    let nx = x as i32 + effective_dx;
                    if nx >= 0 && (nx as usize) < width {
                        let ny = y + dy as usize;
                        if ny < height {
                            let w = if Some(entry_i) == right_idx {
                                w_right
                            } else if Some(entry_i) == below_idx {
                                w_below
                            } else {
                                weight as f32
                            };
                            let scaled_error = [
                                error[0] * w / divisor,
                                error[1] * w / divisor,
                                error[2] * w / divisor,
                            ];
                            error_buf.add_error(nx as usize, dy as usize, scaled_error);
                        }
                    }
                }
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

            let oklab = Oklab::from(pixel);
            let (nearest_idx, _dist) = palette.find_nearest(oklab);
            output[idx] = nearest_idx as u8;

            let nearest_linear = palette.actual_linear(nearest_idx);
            let error = [
                pixel.r - nearest_linear.r,
                pixel.g - nearest_linear.g,
                pixel.b - nearest_linear.b,
            ];

            // Chromatic error damping
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

            // Diffuse error to neighbors using jittered kernel
            let divisor = kernel.divisor as f32;
            for (entry_i, &(dx, dy, weight)) in kernel.entries.iter().enumerate() {
                let effective_dx = if reverse { -dx } else { dx };
                let nx = x as i32 + effective_dx;

                if nx >= 0 && (nx as usize) < width {
                    let ny = y + dy as usize;
                    if ny < height {
                        let w = if Some(entry_i) == right_idx {
                            w_right
                        } else if Some(entry_i) == below_idx {
                            w_below
                        } else {
                            weight as f32
                        };
                        let scaled_error = [
                            damped_error[0] * w / divisor,
                            damped_error[1] * w / divisor,
                            damped_error[2] * w / divisor,
                        ];
                        error_buf.add_error(nx as usize, dy as usize, scaled_error);
                    }
                }
            }
        }

        error_buf.advance_row();
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_buffer_creation() {
        let buf = ErrorBuffer::new(100, 3);
        assert_eq!(buf.rows.len(), 3, "Should have 3 rows");
        assert_eq!(buf.width, 100, "Width should be 100");

        for row in &buf.rows {
            for pixel in row {
                assert_eq!(*pixel, [0.0, 0.0, 0.0]);
            }
        }
    }

    #[test]
    fn test_error_buffer_add_and_get() {
        let mut buf = ErrorBuffer::new(10, 2);

        buf.add_error(5, 0, [0.1, 0.2, 0.3]);
        let accumulated = buf.get_accumulated(5);
        assert!((accumulated[0] - 0.1).abs() < f32::EPSILON);
        assert!((accumulated[1] - 0.2).abs() < f32::EPSILON);
        assert!((accumulated[2] - 0.3).abs() < f32::EPSILON);

        buf.add_error(5, 0, [0.1, 0.1, 0.1]);
        let accumulated = buf.get_accumulated(5);
        assert!((accumulated[0] - 0.2).abs() < f32::EPSILON);
        assert!((accumulated[1] - 0.3).abs() < f32::EPSILON);
        assert!((accumulated[2] - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn test_error_buffer_advance_row() {
        let mut buf = ErrorBuffer::new(10, 3);

        buf.add_error(0, 0, [1.0, 0.0, 0.0]);
        buf.add_error(0, 1, [2.0, 0.0, 0.0]);
        buf.add_error(0, 2, [3.0, 0.0, 0.0]);

        assert!((buf.rows[0][0][0] - 1.0).abs() < f32::EPSILON);
        assert!((buf.rows[1][0][0] - 2.0).abs() < f32::EPSILON);
        assert!((buf.rows[2][0][0] - 3.0).abs() < f32::EPSILON);

        buf.advance_row();

        assert!(
            (buf.rows[0][0][0] - 2.0).abs() < f32::EPSILON,
            "Old row 1 should now be row 0"
        );
        assert!(
            (buf.rows[1][0][0] - 3.0).abs() < f32::EPSILON,
            "Old row 2 should now be row 1"
        );
        assert!(
            buf.rows[2][0][0].abs() < f32::EPSILON,
            "New last row should be cleared"
        );
    }

    #[test]
    fn test_error_buffer_bounds_checking() {
        let mut buf = ErrorBuffer::new(10, 2);

        buf.add_error(100, 0, [1.0, 1.0, 1.0]);
        buf.add_error(0, 10, [1.0, 1.0, 1.0]);

        buf.add_error(5, 0, [0.5, 0.5, 0.5]);
        let accumulated = buf.get_accumulated(5);
        assert!((accumulated[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_error_buffer_sized_for_kernels() {
        let atkinson_buf = ErrorBuffer::new(100, ATKINSON.max_dy + 1);
        assert_eq!(atkinson_buf.rows.len(), 3);

        let fs_buf = ErrorBuffer::new(100, FLOYD_STEINBERG.max_dy + 1);
        assert_eq!(fs_buf.rows.len(), 2);

        let jjn_buf = ErrorBuffer::new(100, JARVIS_JUDICE_NINKE.max_dy + 1);
        assert_eq!(jjn_buf.rows.len(), 3);
    }

    #[test]
    fn test_algorithm_kernel_mapping() {
        assert_eq!(DitherAlgorithm::Atkinson.kernel().divisor, 8);
        assert_eq!(DitherAlgorithm::FloydSteinberg.kernel().divisor, 16);
        assert_eq!(DitherAlgorithm::JarvisJudiceNinke.kernel().divisor, 48);
        assert_eq!(DitherAlgorithm::Sierra.kernel().divisor, 32);
        assert_eq!(DitherAlgorithm::SierraTwoRow.kernel().divisor, 16);
        assert_eq!(DitherAlgorithm::SierraLite.kernel().divisor, 4);
        assert_eq!(DitherAlgorithm::Stucki.kernel().divisor, 42);
        assert_eq!(DitherAlgorithm::Burkes.kernel().divisor, 32);
    }

    #[test]
    fn test_algorithm_defaults() {
        let (ec, ns) = DitherAlgorithm::Atkinson.defaults();
        assert!((ec - 0.08).abs() < f32::EPSILON);
        assert!((ns - 0.0).abs() < f32::EPSILON);

        let (ec, ns) = DitherAlgorithm::FloydSteinberg.defaults();
        assert!((ec - 0.12).abs() < f32::EPSILON);
        assert!((ns - 4.0).abs() < f32::EPSILON);
    }
}
