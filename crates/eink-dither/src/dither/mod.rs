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
//! - **AtkinsonHybrid**: Hybrid propagation — 100% achromatic, 75% chromatic
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

    /// Atkinson hybrid error diffusion.
    ///
    /// Uses the same 6-neighbor Atkinson kernel shape but with hybrid
    /// error propagation: 100% for the achromatic (mean) component and
    /// 75% for the chromatic (deviation from mean) component. This fixes
    /// color drift on chromatic palettes while preserving Atkinson's
    /// distinctive high-contrast character.
    AtkinsonHybrid,

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
            Self::Atkinson | Self::AtkinsonHybrid => &ATKINSON,
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
    ///
    /// `error_clamp` bounds the accumulated diffusion error per channel (see
    /// [`apply_error`]). It is deliberately uniform across algorithms: the
    /// old per-algorithm values (0.03-0.12) were tuned when the clamp bounded
    /// the resulting VALUE rather than the error, where the useful range
    /// depended on how close the content sat to a channel extreme. Under the
    /// current meaning that variation no longer corresponds to anything, and
    /// measuring every algorithm against the palette's physical bound shows
    /// the same shape for all of them: sharply better up to ~0.5, flattening
    /// by ~1.0, negligible past ~2.0.
    ///
    /// 1.0 is the knee, and it has a natural reading — accumulated error may
    /// not exceed full scale in a channel. Going higher buys ~0.003 dE on
    /// flat patches while removing the bound that keeps one dark or blown-out
    /// region from dragging its neighbours.
    pub fn defaults(&self) -> (f32, f32) {
        let noise_scale = match self {
            Self::Atkinson | Self::AtkinsonHybrid => 0.0,
            Self::FloydSteinberg => 4.0,
            Self::JarvisJudiceNinke => 6.0,
            Self::Sierra => 5.5,
            Self::SierraTwoRow => 7.0,
            Self::SierraLite => 2.5,
            Self::Stucki => 6.0,
            Self::Burkes => 7.0,
        };
        (1.0, noise_scale)
    }

    /// Whether this algorithm uses hybrid achromatic/chromatic error propagation.
    ///
    /// When true, the dither loop splits error into achromatic (mean) and
    /// chromatic (deviation) components, propagating each with a different
    /// divisor to prevent color drift.
    pub fn is_hybrid_propagation(&self) -> bool {
        matches!(self, Self::AtkinsonHybrid)
    }
}

use crate::color::{LinearRgb, Oklab};
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

/// Apply accumulated diffusion error to a channel, bounding the error itself.
///
/// The bound is on the ERROR, not on the resulting value. Bounding the value
/// instead — clamping `channel + error` into `[-max, 1 + max]`, as this used
/// to — makes the available headroom depend on where the channel already
/// sits. A saturated colour is at a channel extreme by definition, so it got
/// only `max` of room for error to accumulate, the same entry won every
/// pixel, and the region came out flat. Neutral mid-tones, which need the
/// help least, got the most headroom.
///
/// Bounding the error gives every channel the same room wherever it sits,
/// while still capping how far one pixel's debt can drag its neighbours.
#[inline]
pub(crate) fn apply_error(channel: f32, error: f32, max_error: f32) -> f32 {
    channel + error.clamp(-max_error, max_error)
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

    // For hybrid propagation: achromatic divisor = weight_sum (100%), chromatic = kernel.divisor (75%)
    let weight_sum: f32 = kernel.entries.iter().map(|&(_, _, w)| w as f32).sum();

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

            // Add accumulated error to input pixel
            let accumulated = error_buf.get_accumulated(x);
            let pixel = LinearRgb::new(
                apply_error(image[idx].r, accumulated[0], options.error_clamp),
                apply_error(image[idx].g, accumulated[1], options.error_clamp),
                apply_error(image[idx].b, accumulated[2], options.error_clamp),
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

            // Apply strength scaling
            let strength_error = [
                damped_error[0] * options.strength,
                damped_error[1] * options.strength,
                damped_error[2] * options.strength,
            ];

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
                        let scaled_error = if options.hybrid_propagation {
                            // Hybrid: 100% achromatic + 75% chromatic propagation
                            let em = (strength_error[0] + strength_error[1] + strength_error[2])
                                * (1.0 / 3.0);
                            [
                                em * w / weight_sum + (strength_error[0] - em) * w / divisor,
                                em * w / weight_sum + (strength_error[1] - em) * w / divisor,
                                em * w / weight_sum + (strength_error[2] - em) * w / divisor,
                            ]
                        } else {
                            [
                                strength_error[0] * w / divisor,
                                strength_error[1] * w / divisor,
                                strength_error[2] * w / divisor,
                            ]
                        };
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
    use crate::color::Srgb;

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
        assert_eq!(DitherAlgorithm::AtkinsonHybrid.kernel().divisor, 8);
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
        // error_clamp is uniform across algorithms; only noise_scale varies.
        for algo in [
            DitherAlgorithm::Atkinson,
            DitherAlgorithm::AtkinsonHybrid,
            DitherAlgorithm::FloydSteinberg,
            DitherAlgorithm::JarvisJudiceNinke,
            DitherAlgorithm::Sierra,
            DitherAlgorithm::SierraTwoRow,
            DitherAlgorithm::SierraLite,
            DitherAlgorithm::Stucki,
            DitherAlgorithm::Burkes,
        ] {
            let (ec, _) = algo.defaults();
            assert!(
                (ec - 1.0).abs() < f32::EPSILON,
                "{algo:?} should use the uniform error_clamp default"
            );
        }

        let (_, ns) = DitherAlgorithm::Atkinson.defaults();
        assert!((ns - 0.0).abs() < f32::EPSILON);
        let (_, ns) = DitherAlgorithm::FloydSteinberg.defaults();
        assert!((ns - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_hybrid_propagation_flag() {
        assert!(!DitherAlgorithm::Atkinson.is_hybrid_propagation());
        assert!(DitherAlgorithm::AtkinsonHybrid.is_hybrid_propagation());
        assert!(!DitherAlgorithm::FloydSteinberg.is_hybrid_propagation());
    }

    /// Helper: create a B&W palette for strength tests.
    fn bw_palette() -> Palette {
        Palette::new(
            &[Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)],
            None,
        )
        .unwrap()
    }

    /// Helper: create a 4x4 mid-grey image (forces dithering between B&W).
    fn grey_4x4() -> Vec<LinearRgb> {
        let mid = Srgb::from_u8(128, 128, 128);
        let lin = LinearRgb::from(mid);
        vec![lin; 16]
    }

    #[test]
    fn test_strength_1_matches_default() {
        let palette = bw_palette();
        let image = grey_4x4();
        let kernel = DitherAlgorithm::FloydSteinberg.kernel();

        let default_opts = DitherOptions::new().error_clamp(0.12).noise_scale(0.0);
        let strength_1_opts = default_opts.clone().strength(1.0);

        let result_default =
            dither_with_kernel_noise(&image, 4, 4, &palette, kernel, &default_opts);
        let result_strength_1 =
            dither_with_kernel_noise(&image, 4, 4, &palette, kernel, &strength_1_opts);

        assert_eq!(
            result_default, result_strength_1,
            "strength=1.0 should produce identical output to default"
        );
    }

    #[test]
    fn test_strength_0_produces_nearest_color() {
        let palette = bw_palette();
        let image = grey_4x4();
        let kernel = DitherAlgorithm::FloydSteinberg.kernel();
        let opts = DitherOptions::new()
            .error_clamp(0.12)
            .noise_scale(0.0)
            .strength(0.0);

        let result = dither_with_kernel_noise(&image, 4, 4, &palette, kernel, &opts);

        // With strength=0, no error diffusion occurs. Every pixel gets the
        // same nearest-color mapping (mid-grey is closer to white in linear space).
        let first = result[0];
        assert!(
            result.iter().all(|&v| v == first),
            "strength=0 should produce uniform nearest-color (no dithering pattern)"
        );
    }

    #[test]
    fn test_strength_half_differs_from_1() {
        let palette = bw_palette();
        let image = grey_4x4();
        let kernel = DitherAlgorithm::FloydSteinberg.kernel();

        let opts_1 = DitherOptions::new()
            .error_clamp(0.12)
            .noise_scale(0.0)
            .strength(1.0);
        let opts_half = DitherOptions::new()
            .error_clamp(0.12)
            .noise_scale(0.0)
            .strength(0.5);

        let result_1 = dither_with_kernel_noise(&image, 4, 4, &palette, kernel, &opts_1);
        let result_half = dither_with_kernel_noise(&image, 4, 4, &palette, kernel, &opts_half);

        assert_ne!(
            result_1, result_half,
            "strength=0.5 should produce a different pattern than strength=1.0"
        );
    }
}
