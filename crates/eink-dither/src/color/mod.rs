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
mod oklch;
mod srgb;

pub use linear_rgb::LinearRgb;
pub use oklab::Oklab;
pub use oklch::Oklch;
pub use srgb::Srgb;

#[cfg(test)]
mod public_oklch_tests {
    use crate::{LinearRgb, Oklab, Oklch};

    #[test]
    fn oklch_is_publicly_reachable_and_round_trips() {
        let lab = Oklab::from(LinearRgb::new(0.3, 0.1, 0.05));
        let lch = Oklch::from(lab);
        let back = Oklab::from(lch);
        assert!((back.l - lab.l).abs() < 1e-6, "l drifted");
        assert!((back.a - lab.a).abs() < 1e-6, "a drifted");
        assert!((back.b - lab.b).abs() < 1e-6, "b drifted");
        assert!(lch.c > 0.0, "a saturated colour must have non-zero chroma");
    }
}
