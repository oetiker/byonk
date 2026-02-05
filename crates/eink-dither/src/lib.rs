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
//! The [`EinkDitherer`] builder is the primary entry point. Configure your
//! palette, rendering intent, and optional tweaks in a single chain:
//!
//! ```
//! use eink_dither::{EinkDitherer, Palette, RenderingIntent, Srgb};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo);
//! let pixels = vec![Srgb::from_u8(128, 128, 128); 4];
//! let result = ditherer.dither(&pixels, 2, 2);
//! ```
//!
//! # Rendering Intents
//!
//! Two rendering intents are available:
//!
//! - **Photo** ([`RenderingIntent::Photo`]): Atkinson error diffusion with
//!   saturation/contrast boost. Best for photographs and natural images.
//! - **Graphics** ([`RenderingIntent::Graphics`]): Blue noise ordered dithering
//!   with no enhancement. Best for logos, text, and UI elements.
//!
//! # Direct Pixel API
//!
//! For in-memory pixels, use [`EinkDitherer::dither()`]:
//!
//! ```
//! use eink_dither::{EinkDitherer, Palette, RenderingIntent, Srgb};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo);
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
//! Multiple error diffusion algorithms are available via the [`Dither`] trait:
//!
//! - Atkinson (75% error propagation, ideal for small palettes)
//! - Floyd-Steinberg (classic algorithm)
//! - Jarvis-Judice-Ninke (large kernel, smooth gradients)
//! - Sierra family (various speed/quality tradeoffs)
//! - Blue noise ordered dithering (no error diffusion artifacts)

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
pub use dither::{
    Atkinson, BlueNoiseDither, Dither, DitherOptions, FloydSteinberg, JarvisJudiceNinke, Sierra,
    SierraLite, SierraTwoRow,
};
pub use output::{DitheredImage, RenderingIntent};
pub use palette::{DistanceMetric, Palette, PaletteError, ParseColorError};
pub use preprocess::{PreprocessOptions, PreprocessResult, Preprocessor};
