//! Linear RGB color type
//!
//! Linear RGB is the color space where light addition is physically accurate.
//! All color math (blending, distance calculations) should be done in linear RGB.

use super::lut::srgb_to_linear;
use super::srgb::Srgb;

/// A color in linear RGB color space.
///
/// Linear RGB values represent light intensity proportional to physical light power.
/// Use this type for all color calculations (blending, distance, etc.) because
/// arithmetic operations are only meaningful in linear space.
///
/// Values are typically in the range 0.0..=1.0, but may exceed this range
/// for HDR content or intermediate calculations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearRgb {
    /// Red channel (linear light intensity)
    pub r: f32,
    /// Green channel (linear light intensity)
    pub g: f32,
    /// Blue channel (linear light intensity)
    pub b: f32,
}

impl LinearRgb {
    /// Create a new LinearRgb color from linear RGB values.
    ///
    /// # Arguments
    /// * `r` - Red channel (typically 0.0..=1.0)
    /// * `g` - Green channel (typically 0.0..=1.0)
    /// * `b` - Blue channel (typically 0.0..=1.0)
    #[inline]
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }
}

impl From<Srgb> for LinearRgb {
    /// Convert from sRGB to linear RGB using the gamma lookup table.
    ///
    /// This conversion is necessary before performing any color calculations,
    /// as sRGB encodes perceptual uniformity while linear RGB represents
    /// physical light intensity.
    fn from(srgb: Srgb) -> Self {
        // WHY LUT-based gamma decode (IEC 61966-2-1): sRGB's nonlinear gamma
        // curve makes arithmetic incorrect. Linear space is required for
        // physically accurate color math (error diffusion, contrast, blending).
        Self {
            r: srgb_to_linear(srgb.r),
            g: srgb_to_linear(srgb.g),
            b: srgb_to_linear(srgb.b),
        }
    }
}
