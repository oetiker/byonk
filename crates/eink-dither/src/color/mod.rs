//! Color types and conversion utilities
//!
//! This module provides type-safe color handling with compile-time distinction
//! between sRGB and linear RGB color spaces.
//!
//! # Color Spaces
//!
//! - **sRGB**: The standard color space for image storage and display. Use for I/O.
//! - **LinearRgb**: Linear light intensity. Use for all color calculations.
//!
//! # Example
//!
//! ```
//! use eink_dither::{Srgb, LinearRgb};
//!
//! // Load a pixel from an image (sRGB)
//! let srgb = Srgb::from_u8(128, 64, 32);
//!
//! // Convert to linear for calculations
//! let linear = LinearRgb::from(srgb);
//!
//! // After calculations, convert back to sRGB for output
//! let output = Srgb::from(linear);
//! ```

mod linear_rgb;
mod lut;
mod oklab;
mod srgb;

pub use linear_rgb::LinearRgb;
pub use oklab::Oklab;
pub use srgb::Srgb;
