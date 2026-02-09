// Vendored crate — suppress clippy lints that are impractical to fix
// (generated LUT tables, upstream code style, etc.)
#![allow(
    clippy::excessive_precision,
    clippy::needless_range_loop,
    clippy::module_inception,
    clippy::doc_overindented_list_items,
    clippy::manual_range_contains
)]

//! eink-dither: High-quality dithering for e-ink displays
//!
//! This library provides perceptually accurate color conversion and dithering
//! algorithms optimized for e-ink displays with limited color palettes.
//!
//! # Quick Start
//!
//! The [`EinkDitherer`] builder is the primary entry point:
//!
//! ```
//! use eink_dither::{EinkDitherer, Palette, Srgb};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! let ditherer = EinkDitherer::new(palette);
//! let pixels = vec![Srgb::from_u8(128, 128, 128); 4];
//! let result = ditherer.dither(&pixels, 2, 2);
//! ```
//!
//! # Direct Pixel API
//!
//! For in-memory pixels, use [`EinkDitherer::dither()`]:
//!
//! ```
//! use eink_dither::{EinkDitherer, Palette, Srgb};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! let ditherer = EinkDitherer::new(palette);
//! let pixels = vec![Srgb::from_u8(128, 128, 128); 4];
//! let result = ditherer.dither(&pixels, 2, 2);
//!
//! assert_eq!(result.width(), 2);
//! assert_eq!(result.height(), 2);
//! ```
//!
//! # Color Spaces
//!
//! The library enforces type-safe color handling:
//!
//! - [`Srgb`]: Standard gamma-corrected sRGB for input/output
//! - [`LinearRgb`]: Linear light intensity for color calculations
//! - [`Oklab`]: Perceptually uniform color space for matching
//!
//! # Dithering Algorithms
//!
//! Eight error diffusion algorithms are available via [`DitherAlgorithm`]:
//!
//! - Atkinson (75% error propagation, ideal for small palettes — default)
//! - Floyd-Steinberg (classic algorithm)
//! - Jarvis-Judice-Ninke (large kernel, smooth gradients)
//! - Sierra family (full, two-row, lite — various speed/quality tradeoffs)
//! - Stucki (sharper center weights)
//! - Burkes (simplified Stucki, 2 rows)
//!
//! All algorithms support blue noise kernel jitter via the `noise_scale` option.
//!
//! # Color Science
//!
//! This section explains the rationale behind the color space choices and
//! distance metric used in the dithering pipeline. Understanding these
//! decisions is essential for maintaining correctness -- subtle changes
//! (e.g., computing distance in sRGB instead of OKLab, or diffusing error
//! in a perceptual space) produce dramatically wrong visual results on
//! limited e-ink palettes.
//!
//! ## Three Color Spaces, Three Purposes
//!
//! The pipeline uses three color spaces, each chosen for a specific physical
//! or perceptual property:
//!
//! | Color Space | Key Property | Used For |
//! |-------------|--------------|----------|
//! | **sRGB** | Standard encoding (IEC 61966-2-1) | Input/output: image files, device communication, byte-exact palette matching |
//! | **Linear RGB** | Physically proportional to light intensity | Error diffusion, contrast adjustment, blending |
//! | **OKLab** | Perceptually uniform distances | Palette matching via [`Palette::find_nearest()`] |
//!
//! **sRGB** is a gamma-corrected encoding designed so that brightness steps
//! look uniform to human eyes on a display. It is NOT suitable for
//! arithmetic -- adding two sRGB values does not produce the correct
//! combined light output. All image files and device palette definitions
//! use sRGB.
//!
//! **Linear RGB** represents physical light intensity: doubling a value
//! doubles the photon count. Adding two Linear RGB values produces the
//! physically correct combined light output. This is why error diffusion
//! (the quantization error distributed to neighboring pixels) must operate
//! in Linear RGB: the error represents a light intensity difference.
//!
//! **OKLab** (Björn Ottosson, 2020) is a perceptually uniform color space
//! where Euclidean distance correlates with human-perceived color
//! difference. Two colors that are 0.1 apart in OKLab look equally
//! different regardless of where in the color gamut they fall. This is why
//! palette matching uses OKLab-based distances.
//!
//! ## Pipeline Overview
//!
//! ```text
//! sRGB input              (from image file / SVG renderer)
//!     |
//!     v
//! LinearRgb               (gamma decode via LUT)
//!     |
//!     +---> Oklch          (saturation boost: scale chroma, preserves hue)
//!     |       |
//!     |     Oklab           (back to Cartesian)
//!     |       |
//!     +<-- LinearRgb        (back to linear for contrast adjustment)
//!     |
//!     v
//! [Contrast adjust]        (scale around midpoint in LinearRgb)
//!     |
//!     v
//! ╔═══════════════════════════════════════════╗
//! ║  Dither Loop (error diffusion path)       ║
//! ║                                           ║
//! ║  pixel + accumulated error  (LinearRgb)   ║
//! ║      |                                    ║
//! ║      +---> Oklab  (perceptual matching)   ║
//! ║      |       |                            ║
//! ║      |  find_nearest() using              ║
//! ║      |    HyAB + chroma coupling          ║
//! ║      |       |                            ║
//! ║      |  palette index (output)            ║
//! ║      |                                    ║
//! ║      v                                    ║
//! ║  error = pixel - palette[idx] (LinearRgb) ║
//! ║      |                                    ║
//! ║      v                                    ║
//! ║  diffuse error to neighbors (LinearRgb)   ║
//! ╚═══════════════════════════════════════════╝
//! ```
//!
//! ## Distance Metric: HyAB + Chroma Coupling
//!
//! Standard Euclidean distance in OKLab treats lightness and chrominance
//! symmetrically. This works well for continuous color spaces but fails
//! for discrete e-ink palettes (typically 6--16 colors) where a grey
//! pixel can map to a chromatic color with similar lightness. For
//! example, yellow has OKLab L=0.97 -- nearly identical to white
//! (L=1.0) -- so Euclidean distance happily maps light greys to yellow.
//!
//! **HyAB** (Abasi et al., 2020) improves on this by decoupling
//! lightness from chrominance:
//!
//! ```text
//! d_HyAB = kl * |L1 - L2| + kc * sqrt((a1 - a2)^2 + (b1 - b2)^2)
//! ```
//!
//! With `kl = 2.0`, lightness differences are weighted 2x relative to
//! chrominance. This helps but is still insufficient for the e-ink
//! palette matching problem because it does not account for the
//! *absolute* chroma of the pixel versus the palette entry.
//!
//! **Chroma coupling** is a domain-specific extension that we add on
//! top of standard HyAB. It is NOT from published literature. It adds
//! a penalty proportional to the difference in chroma magnitude between
//! the input pixel and the palette entry:
//!
//! ```text
//! d = kl * |dL| + kc * sqrt(da^2 + db^2) + kchroma * |C_pixel - C_palette|
//! ```
//!
//! where `C = sqrt(a^2 + b^2)` is the chroma magnitude.
//!
//! With `kchroma = 10.0`, a grey pixel (C approximately 0) incurs a large penalty
//! when compared to any chromatic palette entry (C > 0), forcing it to
//! match black or white instead. Chromatic-to-chromatic matching is
//! minimally affected because colors with similar hue have similar
//! chroma magnitudes.
//!
//! **Tuning constants:** `kl = 2.0, kc = 1.0, kchroma = 10.0`. The high
//! `kchroma` value (increased from an initial estimate of 2.0) was
//! determined empirically.
//!
//! See [`DistanceMetric::HyAB`] for the implementation.
//!
//! ## Why Error Diffusion Stays in Linear RGB
//!
//! Quantization error represents the difference between the desired
//! light output and the chosen palette color's light output. Light adds
//! linearly in the physical world, so this difference must be computed
//! and propagated in Linear RGB to maintain physical accuracy.
//!
//! Computing error in sRGB would over-weight dark tones (where the
//! gamma curve is steep, so small sRGB differences correspond to large
//! physical differences) and under-weight light tones. Computing error
//! in OKLab would give perceptually uniform error distribution, but
//! error diffusion is fundamentally a *physical light addition*
//! operation -- the error is added to neighboring pixels' light values
//! -- and OKLab does not represent physical light addition correctly.

pub mod api;
pub mod color;
pub mod dither;
pub mod output;
pub mod palette;
pub mod preprocess;

#[cfg(test)]
mod domain_tests;

pub use api::{DitherError, EinkDitherer};
pub use color::{LinearRgb, Oklab, Srgb};
pub use dither::{DitherAlgorithm, DitherOptions};
pub use output::DitheredImage;
pub use palette::{DistanceMetric, Palette, PaletteError, ParseColorError};
pub use preprocess::{PreprocessOptions, PreprocessResult, Preprocessor};
