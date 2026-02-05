//! Gamma lookup table access functions
//!
//! This module provides fast gamma conversion using pre-computed lookup tables
//! generated at compile time by build.rs.

// Include the generated LUT from build.rs
include!(concat!(env!("OUT_DIR"), "/gamma_lut.rs"));

/// Convert an sRGB value (0.0..=1.0) to linear RGB using LUT with linear interpolation.
///
/// # Panics (debug only)
/// Debug-asserts that the input is in the range 0.0..=1.0.
#[inline]
pub fn srgb_to_linear(srgb: f32) -> f32 {
    debug_assert!(
        srgb >= 0.0 && srgb <= 1.0,
        "srgb_to_linear: input {srgb} out of range 0.0..=1.0"
    );

    // Clamp for safety in release mode
    let srgb = srgb.clamp(0.0, 1.0);

    // Scale to LUT index range (0..4095)
    let scaled = srgb * 4095.0;
    let index = scaled as usize;

    // Handle edge case: index 4095 (no interpolation possible)
    if index >= 4095 {
        return SRGB_TO_LINEAR[4095];
    }

    // Linear interpolation between adjacent LUT entries
    let frac = scaled - index as f32;
    let a = SRGB_TO_LINEAR[index];
    let b = SRGB_TO_LINEAR[index + 1];
    a + (b - a) * frac
}

/// Convert a linear RGB value (0.0..=1.0) to sRGB using LUT with linear interpolation.
///
/// # Panics (debug only)
/// Debug-asserts that the input is in the range 0.0..=1.0.
#[inline]
pub fn linear_to_srgb(linear: f32) -> f32 {
    debug_assert!(
        linear >= 0.0 && linear <= 1.0,
        "linear_to_srgb: input {linear} out of range 0.0..=1.0"
    );

    // Clamp for safety in release mode
    let linear = linear.clamp(0.0, 1.0);

    // Scale to LUT index range (0..4095)
    let scaled = linear * 4095.0;
    let index = scaled as usize;

    // Handle edge case: index 4095 (no interpolation possible)
    if index >= 4095 {
        return LINEAR_TO_SRGB[4095];
    }

    // Linear interpolation between adjacent LUT entries
    let frac = scaled - index as f32;
    let a = LINEAR_TO_SRGB[index];
    let b = LINEAR_TO_SRGB[index + 1];
    a + (b - a) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srgb_to_linear_boundaries() {
        // Exact boundaries should match formula
        assert!((srgb_to_linear(0.0) - 0.0).abs() < 1e-6);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_linear_to_srgb_boundaries() {
        assert!((linear_to_srgb(0.0) - 0.0).abs() < 1e-6);
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_monotonicity() {
        // sRGB to linear should be monotonically increasing
        let mut prev = srgb_to_linear(0.0);
        for i in 1..=1000 {
            let curr = srgb_to_linear(i as f32 / 1000.0);
            assert!(curr >= prev, "srgb_to_linear not monotonic at {i}");
            prev = curr;
        }

        // Linear to sRGB should be monotonically increasing
        let mut prev = linear_to_srgb(0.0);
        for i in 1..=1000 {
            let curr = linear_to_srgb(i as f32 / 1000.0);
            assert!(curr >= prev, "linear_to_srgb not monotonic at {i}");
            prev = curr;
        }
    }
}
