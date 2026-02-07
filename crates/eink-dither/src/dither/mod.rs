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
//!
//! # Architecture
//!
//! All algorithms implement the [`Dither`] trait, allowing easy algorithm
//! swapping. Configuration is done via [`DitherOptions`].
//!
//! # Example
//!
//! ```ignore
//! use eink_dither::{Atkinson, Dither, DitherOptions, Palette, LinearRgb};
//!
//! let palette = Palette::new(&colors, None).unwrap();
//! let options = DitherOptions::new();
//!
//! let indices: Vec<u8> = Atkinson.dither(&pixels, width, height, &palette, &options);
//! ```

mod atkinson;
mod blue_noise;
mod blue_noise_matrix;
mod floyd_steinberg;
mod floyd_steinberg_noise;
mod jjn;
mod kernel;
mod kernel_noise;
mod options;
mod sierra;
mod simplex;

pub use atkinson::Atkinson;
pub use blue_noise::BlueNoiseDither;
pub use floyd_steinberg::FloydSteinberg;
pub use floyd_steinberg_noise::FloydSteinbergNoise;
pub use jjn::JarvisJudiceNinke;
pub use kernel::*;
pub use kernel_noise::*;
pub use options::DitherOptions;
pub use sierra::{Sierra, SierraLite, SierraTwoRow};
pub use simplex::SimplexDither;

/// Dither algorithm selection for builder API.
///
/// This enum allows selecting the dithering algorithm via the
/// [`EinkDitherer`](crate::EinkDitherer) builder, overriding the
/// default algorithm for the rendering intent.
///
/// # Default Algorithms by Intent
///
/// - **Photo**: [`Atkinson`] (error diffusion)
/// - **Graphics**: [`BlueNoiseDither`] (ordered dithering)
///
/// # Example
///
/// ```
/// use eink_dither::{EinkDitherer, Palette, RenderingIntent, DitherAlgorithm, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
///
/// // Use SimplexDither instead of the default BlueNoiseDither for Graphics
/// let ditherer = EinkDitherer::new(palette, RenderingIntent::Graphics)
///     .algorithm(DitherAlgorithm::Simplex);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DitherAlgorithm {
    /// Use the default algorithm for the rendering intent.
    ///
    /// - Photo: Atkinson error diffusion
    /// - Graphics: Blue noise ordered dithering
    #[default]
    Auto,

    /// Atkinson error diffusion (75% propagation).
    ///
    /// Best for photographs. Produces smooth gradients but can have
    /// directional artifacts.
    Atkinson,

    /// Floyd-Steinberg error diffusion (100% propagation).
    ///
    /// Classic algorithm with full error propagation.
    FloydSteinberg,

    /// Blue noise ordered dithering.
    ///
    /// Best for graphics. No error bleeding across edges, deterministic.
    /// Uses 2-color interpolation per pixel.
    BlueNoise,

    /// Simplex (barycentric) ordered dithering.
    ///
    /// Uses Delaunay triangulation in OKLab space for up to 4-color
    /// blending per pixel. 27% better color accuracy than blue noise
    /// while maintaining ordered dithering benefits.
    Simplex,

    /// Floyd-Steinberg with blue noise kernel weight jitter.
    ///
    /// Varies the error diffusion direction per pixel using blue noise
    /// to break "worm" artifacts while maintaining 100% error propagation.
    /// Best for photographs with smooth gradients.
    FloydSteinbergNoise,

    /// Jarvis-Judice-Ninke error diffusion (100% propagation, 12 neighbors).
    ///
    /// Large 3-row kernel with peak weight 7/48 ≈ 14.6%. The wide spread
    /// prevents oscillation artifacts on sparse chromatic palettes while
    /// maintaining full error propagation for accurate color reproduction.
    JarvisJudiceNinke,

    /// Sierra (full) error diffusion (100% propagation, 10 neighbors).
    ///
    /// 3-row kernel with peak weight 5/32 ≈ 15.6%. Similar anti-oscillation
    /// properties to JJN with slightly fewer neighbors.
    Sierra,

    /// Sierra two-row error diffusion (100% propagation, 7 neighbors).
    ///
    /// 2-row kernel with peak weight 4/16 = 25%. Faster than full Sierra
    /// with good oscillation resistance.
    SierraTwoRow,

    /// Sierra Lite error diffusion (100% propagation, 3 neighbors).
    ///
    /// Minimal 2-row kernel with peak weight 2/4 = 50%. Fastest Sierra
    /// variant, similar characteristics to Floyd-Steinberg.
    SierraLite,

    /// Jarvis-Judice-Ninke with blue noise kernel weight jitter.
    ///
    /// Varies the error diffusion direction per pixel using blue noise
    /// to break "worm" artifacts while maintaining 100% error propagation.
    JarvisJudiceNinkeNoise,

    /// Sierra (full) with blue noise kernel weight jitter.
    ///
    /// Varies the error diffusion direction per pixel using blue noise
    /// to break "worm" artifacts while maintaining 100% error propagation.
    SierraNoise,

    /// Sierra Two-Row with blue noise kernel weight jitter.
    ///
    /// Varies the error diffusion direction per pixel using blue noise
    /// to break "worm" artifacts while maintaining 100% error propagation.
    SierraTwoRowNoise,

    /// Sierra Lite with blue noise kernel weight jitter.
    ///
    /// Varies the error diffusion direction per pixel using blue noise
    /// to break "worm" artifacts while maintaining 100% error propagation.
    SierraLiteNoise,
}

use crate::color::{LinearRgb, Oklab, Srgb};
use crate::palette::Palette;

/// Trait for error diffusion dithering algorithms.
///
/// Implementors provide a specific diffusion kernel and algorithm for
/// converting continuous-tone images to indexed palette images.
///
/// # Error Diffusion
///
/// Error diffusion works by:
/// 1. For each pixel, find the nearest palette color
/// 2. Compute the quantization error (desired - actual)
/// 3. Distribute that error to neighboring unprocessed pixels
/// 4. Repeat, with accumulated error influencing future decisions
///
/// This produces smooth gradients and natural-looking dithering.
pub trait Dither {
    /// Dither an image to palette indices.
    ///
    /// # Arguments
    ///
    /// * `image` - Input pixels in linear RGB space (row-major order)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `palette` - Color palette for quantization
    /// * `options` - Dithering configuration
    ///
    /// # Returns
    ///
    /// A `Vec<u8>` of palette indices, one per pixel, in row-major order.
    /// Each index is in the range `0..palette.len()`.
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8>;
}

/// Error buffer for efficient error diffusion.
///
/// Manages a sliding window of error rows, storing only the rows that
/// the diffusion kernel can reach (determined by `max_dy`). This avoids
/// allocating a full-image error buffer.
///
/// # Usage Pattern
///
/// 1. Create buffer with `new(width, row_depth)`
/// 2. For each row:
///    a. Read accumulated error with `get_accumulated(x)`
///    b. After processing pixel, distribute error with `add_error(x, dy, error)`
///    c. After row complete, call `advance_row()`
#[derive(Debug)]
pub struct ErrorBuffer {
    /// Error rows: rows[0] is current row, rows[1] is next, etc.
    rows: Vec<Vec<[f32; 3]>>,
    /// Image width
    width: usize,
}

impl ErrorBuffer {
    /// Create a new error buffer.
    ///
    /// # Arguments
    ///
    /// * `width` - Image width in pixels
    /// * `row_depth` - Number of rows to track (kernel's `max_dy + 1`)
    pub fn new(width: usize, row_depth: usize) -> Self {
        Self {
            rows: (0..row_depth).map(|_| vec![[0.0; 3]; width]).collect(),
            width,
        }
    }

    /// Get accumulated error for a pixel in the current row.
    ///
    /// # Arguments
    ///
    /// * `x` - Pixel x-coordinate
    ///
    /// # Returns
    ///
    /// RGB error values accumulated from previous pixels' diffusion.
    #[inline]
    pub fn get_accumulated(&self, x: usize) -> [f32; 3] {
        self.rows[0][x]
    }

    /// Add error to a future pixel.
    ///
    /// # Arguments
    ///
    /// * `x` - Target pixel x-coordinate
    /// * `row_offset` - Row offset (0 = current row, 1 = next row, etc.)
    /// * `error` - RGB error values to add
    ///
    /// Silently ignores out-of-bounds coordinates.
    #[inline]
    pub fn add_error(&mut self, x: usize, row_offset: usize, error: [f32; 3]) {
        if x < self.width && row_offset < self.rows.len() {
            for c in 0..3 {
                self.rows[row_offset][x][c] += error[c];
            }
        }
    }

    /// Advance to the next row.
    ///
    /// Rotates the row buffer: the first row is discarded, subsequent rows
    /// shift forward, and a new zeroed row is added at the end.
    pub fn advance_row(&mut self) {
        // Rotate left: [0,1,2] -> [1,2,0]
        self.rows.rotate_left(1);
        // Clear the last row (which was row[0])
        if let Some(last) = self.rows.last_mut() {
            last.fill([0.0; 3]);
        }
    }
}

// ============================================================================
// Shared dithering infrastructure
// ============================================================================

/// Find exact byte-level match against official palette colors.
///
/// Converts the input LinearRgb back to sRGB bytes and compares
/// against each palette entry's official color bytes. We match official
/// (not actual/measured) because input content is authored with official
/// palette colors in mind.
///
/// # Arguments
///
/// * `pixel` - Input pixel in linear RGB space
/// * `palette` - Palette to match against
///
/// # Returns
///
/// `Some(index)` if an exact match is found, `None` otherwise.
pub(crate) fn find_exact_match(pixel: LinearRgb, palette: &Palette) -> Option<u8> {
    // Out-of-gamut pixels (from saturation/contrast boost) can never
    // be exact matches -- palette colors are always in gamut.
    if pixel.r < 0.0
        || pixel.r > 1.0
        || pixel.g < 0.0
        || pixel.g > 1.0
        || pixel.b < 0.0
        || pixel.b > 1.0
    {
        return None;
    }

    // WHY sRGB for exact match: Device palette entries are defined in sRGB.
    // Byte-exact comparison avoids floating-point rounding issues that would
    // cause false negatives with LinearRgb or OKLab comparisons.
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
///
/// The valid range is `[-max_error, 1.0 + max_error]` to allow for
/// accumulated error while preventing extreme values.
#[inline]
pub(crate) fn clamp_channel(value: f32, max_error: f32) -> f32 {
    value.clamp(-max_error, 1.0 + max_error)
}

/// Core error diffusion algorithm parameterized by kernel.
///
/// This function contains the complete error diffusion loop shared by all
/// algorithms. Each algorithm simply calls this with its specific kernel.
///
/// # Arguments
///
/// * `image` - Input pixels in linear RGB space (row-major order)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `palette` - Color palette for quantization
/// * `kernel` - Error diffusion kernel to use
/// * `options` - Dithering configuration
///
/// # Returns
///
/// A `Vec<u8>` of palette indices, one per pixel.
pub(crate) fn dither_with_kernel(
    image: &[LinearRgb],
    width: usize,
    height: usize,
    palette: &Palette,
    kernel: &Kernel,
    options: &DitherOptions,
) -> Vec<u8> {
    let mut output = vec![0u8; width * height];

    // Pre-detect exact matches for entire image (against actual palette colors)
    let exact_matches: Vec<Option<u8>> = if options.preserve_exact_matches {
        image
            .iter()
            .map(|&pixel| find_exact_match(pixel, palette))
            .collect()
    } else {
        vec![None; width * height]
    };

    let threshold_sq = options.chroma_clamp * options.chroma_clamp;

    // Create error buffer with depth = max_dy + 1
    let mut error_buf = ErrorBuffer::new(width, kernel.max_dy + 1);

    for y in 0..height {
        // Determine scan direction
        let reverse = options.serpentine && y % 2 == 1;

        // Process pixels in correct order
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
                    // Absorb: discard accumulated error, preventing color bleed
                    // across hard boundaries like text, UI lines, and borders.
                    continue;
                }
                // Pass-through: compute and diffuse error normally so gradient
                // continuity is maintained across exact-match pixels.
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
                for &(dx, dy, weight) in kernel.entries {
                    let effective_dx = if reverse { -dx } else { dx };
                    let nx = x as i32 + effective_dx;
                    if nx >= 0 && (nx as usize) < width {
                        let ny = y + dy as usize;
                        if ny < height {
                            let scaled_error = [
                                error[0] * weight as f32 / divisor,
                                error[1] * weight as f32 / divisor,
                                error[2] * weight as f32 / divisor,
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

            // Compute OKLab chroma of the ORIGINAL pixel (before accumulated
            // error) for chromatic damping. OKLab chroma is a perceptually
            // uniform measure of colorfulness — unlike linear RGB spread which
            // is distorted by gamma and channel sensitivity differences.
            // Example: an overcast sky (175,198,230) has linear spread 0.37
            // but OKLab chroma only 0.055 — perceptually muted, not vivid.
            let original_oklab = Oklab::from(image[idx]);
            let original_chroma_sq =
                original_oklab.a * original_oklab.a + original_oklab.b * original_oklab.b;

            // WHY OKLab for matching: Perceptual uniformity ensures the palette
            // match minimizes visible color difference. Using sRGB or LinearRgb
            // for distance would produce matches that look wrong to human eyes.
            let oklab = Oklab::from(pixel);
            let (nearest_idx, _dist) = palette.find_nearest(oklab);
            output[idx] = nearest_idx as u8;

            // WHY LinearRgb for error: Quantization error represents a physical
            // light intensity difference. Light adds linearly, so error must
            // accumulate in LinearRgb. sRGB would distort error magnitudes due
            // to its gamma curve; OKLab does not represent light addition correctly.
            let nearest_linear = palette.actual_linear(nearest_idx);
            let error = [
                pixel.r - nearest_linear.r,
                pixel.g - nearest_linear.g,
                pixel.b - nearest_linear.b,
            ];

            // Damp chromatic error before diffusion.
            // Decompose error into achromatic (mean) + chromatic (deviation).
            // Scale the chromatic part by the ORIGINAL pixel's OKLab chroma:
            //   alpha = (chroma / chroma_clamp)⁴ capped at 1.0
            // Quartic ramp ensures muted pixels (chroma ≪ threshold) leak
            // virtually zero chromatic error, preventing accumulation across
            // hundreds of pixels in uniform regions. Quadratic was too gentle —
            // 14% leakage at chroma=0.045 accumulated to 69% chromatic output.
            // Muted pixels (alpha≈0): only achromatic error diffuses → B&W dither.
            // Vivid pixels (alpha=1): full error diffuses → accurate color.
            let damped_error = if options.chroma_clamp < f32::INFINITY {
                let ratio_sq = (original_chroma_sq / threshold_sq).min(1.0);
                let alpha = ratio_sq * ratio_sq; // quartic in chroma/threshold
                let err_mean = (error[0] + error[1] + error[2]) * (1.0 / 3.0);
                [
                    err_mean + alpha * (error[0] - err_mean),
                    err_mean + alpha * (error[1] - err_mean),
                    err_mean + alpha * (error[2] - err_mean),
                ]
            } else {
                error
            };

            // Diffuse error to neighbors using kernel
            let divisor = kernel.divisor as f32;
            for &(dx, dy, weight) in kernel.entries {
                // Flip dx for serpentine reverse rows
                let effective_dx = if reverse { -dx } else { dx };
                let nx = x as i32 + effective_dx;

                // Bounds check
                if nx >= 0 && (nx as usize) < width {
                    let ny = y + dy as usize;
                    if ny < height {
                        let scaled_error = [
                            damped_error[0] * weight as f32 / divisor,
                            damped_error[1] * weight as f32 / divisor,
                            damped_error[2] * weight as f32 / divisor,
                        ];
                        error_buf.add_error(nx as usize, dy as usize, scaled_error);
                    }
                }
            }
        }

        // Advance error buffer after processing row
        error_buf.advance_row();
    }

    output
}

/// Core error diffusion algorithm with blue noise jitter, parameterized by kernel.
///
/// Like [`dither_with_kernel`], but shifts weight between the kernel's `(1,0)` ("right")
/// and `(0,1)` ("below") entries per pixel using a blue noise value. This breaks
/// directional "worm" artifacts while preserving total error propagation.
///
/// The jitter formula:
/// ```text
/// alpha = (BLUE_NOISE_64[y%64][x%64] - 128) / 256    // -0.5..+0.5
/// shift = (alpha * noise_scale).clamp(-base_below, base_right)
/// w_right = base_right - shift
/// w_below = base_below + shift
/// ```
/// All other kernel entries remain unchanged. Total propagation is preserved.
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
    // in the kernel. These are the two entries whose weights get jittered.
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

        // All values should be zero
        for row in &buf.rows {
            for pixel in row {
                assert_eq!(*pixel, [0.0, 0.0, 0.0]);
            }
        }
    }

    #[test]
    fn test_error_buffer_add_and_get() {
        let mut buf = ErrorBuffer::new(10, 2);

        // Add error to current row
        buf.add_error(5, 0, [0.1, 0.2, 0.3]);
        let accumulated = buf.get_accumulated(5);
        assert!((accumulated[0] - 0.1).abs() < f32::EPSILON);
        assert!((accumulated[1] - 0.2).abs() < f32::EPSILON);
        assert!((accumulated[2] - 0.3).abs() < f32::EPSILON);

        // Add more error to same pixel (should accumulate)
        buf.add_error(5, 0, [0.1, 0.1, 0.1]);
        let accumulated = buf.get_accumulated(5);
        assert!((accumulated[0] - 0.2).abs() < f32::EPSILON);
        assert!((accumulated[1] - 0.3).abs() < f32::EPSILON);
        assert!((accumulated[2] - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn test_error_buffer_advance_row() {
        let mut buf = ErrorBuffer::new(10, 3);

        // Add error to rows 0, 1, and 2
        buf.add_error(0, 0, [1.0, 0.0, 0.0]); // row 0
        buf.add_error(0, 1, [2.0, 0.0, 0.0]); // row 1
        buf.add_error(0, 2, [3.0, 0.0, 0.0]); // row 2

        // Verify initial state
        assert!((buf.rows[0][0][0] - 1.0).abs() < f32::EPSILON);
        assert!((buf.rows[1][0][0] - 2.0).abs() < f32::EPSILON);
        assert!((buf.rows[2][0][0] - 3.0).abs() < f32::EPSILON);

        // Advance row
        buf.advance_row();

        // Row 1 should now be row 0, row 2 should be row 1, row 0 should be cleared
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

        // Out of bounds x - should be silently ignored
        buf.add_error(100, 0, [1.0, 1.0, 1.0]);

        // Out of bounds row_offset - should be silently ignored
        buf.add_error(0, 10, [1.0, 1.0, 1.0]);

        // Verify nothing crashed and in-bounds still works
        buf.add_error(5, 0, [0.5, 0.5, 0.5]);
        let accumulated = buf.get_accumulated(5);
        assert!((accumulated[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_error_buffer_sized_for_kernels() {
        // Atkinson needs max_dy=2, so 3 rows
        let atkinson_buf = ErrorBuffer::new(100, ATKINSON.max_dy + 1);
        assert_eq!(atkinson_buf.rows.len(), 3);

        // Floyd-Steinberg needs max_dy=1, so 2 rows
        let fs_buf = ErrorBuffer::new(100, FLOYD_STEINBERG.max_dy + 1);
        assert_eq!(fs_buf.rows.len(), 2);

        // JJN needs max_dy=2, so 3 rows
        let jjn_buf = ErrorBuffer::new(100, JARVIS_JUDICE_NINKE.max_dy + 1);
        assert_eq!(jjn_buf.rows.len(), 3);
    }
}
