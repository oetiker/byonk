//! Oklch polar color space for saturation manipulation.
//!
//! Oklch is the polar form of Oklab, representing colors as:
//! - **L** (Lightness): Same as Oklab L
//! - **C** (Chroma): Saturation/colorfulness (distance from neutral axis)
//! - **h** (Hue): Hue angle in radians
//!
//! This representation makes saturation adjustment trivial: just scale chroma.
//! Unlike HSL/HSV, Oklch is perceptually uniform, so scaling chroma doesn't
//! cause hue shifts.
//!
//! # Internal Use
//!
//! This type is `pub(crate)` because it's an implementation detail of the
//! preprocessing pipeline. Users interact with it indirectly through
//! [`PreprocessOptions::saturation`](super::PreprocessOptions::saturation).
//!
//! # References
//!
//! Bjorn Ottosson, "A perceptual color space for image processing"
//! <https://bottosson.github.io/posts/oklab/>

use crate::Oklab;

/// Oklch: Polar form of Oklab (Lightness, Chroma, Hue).
///
/// Chroma scaling preserves hue and lightness exactly, making this
/// ideal for saturation adjustments.
///
/// # Components
///
/// - `l`: Lightness (same as Oklab L, 0.0 = black, 1.0 = white)
/// - `c`: Chroma (saturation, sqrt(a^2 + b^2), 0.0 = achromatic)
/// - `h`: Hue angle in radians (atan2(b, a))
///
/// # Note
///
/// For achromatic colors (c near zero), hue is undefined. The conversion
/// sets h to 0.0 in this case, which is harmless since chroma scaling
/// on zero chroma produces zero chroma regardless of hue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Oklch {
    /// Lightness: 0.0 (black) to 1.0 (white) for in-gamut colors
    pub l: f32,
    /// Chroma: distance from neutral axis (0.0 = gray)
    pub c: f32,
    /// Hue: angle in radians
    pub h: f32,
}

impl Oklch {
    /// Scale chroma by a factor.
    ///
    /// Factor > 1.0 increases saturation, < 1.0 decreases.
    /// Chroma is clamped to 0.0 minimum (no negative chroma).
    ///
    /// # Arguments
    /// * `factor` - Chroma multiplier (1.0 = no change)
    ///
    /// # Returns
    /// New Oklch with scaled chroma, same lightness and hue
    #[inline]
    pub fn scale_chroma(self, factor: f32) -> Self {
        Self {
            l: self.l,
            c: (self.c * factor).max(0.0),
            h: self.h,
        }
    }
}

impl From<Oklab> for Oklch {
    /// Convert from Oklab to Oklch (Cartesian to polar).
    ///
    /// For achromatic colors (a and b both near zero), hue is set to 0.0.
    fn from(lab: Oklab) -> Self {
        let c = (lab.a * lab.a + lab.b * lab.b).sqrt();
        // For achromatic colors, atan2(0, 0) returns 0.0 in Rust,
        // which is fine since chroma is zero anyway
        let h = lab.b.atan2(lab.a);
        Self { l: lab.l, c, h }
    }
}

impl From<Oklch> for Oklab {
    /// Convert from Oklch to Oklab (polar to Cartesian).
    fn from(lch: Oklch) -> Self {
        Self::new(lch.l, lch.c * lch.h.cos(), lch.c * lch.h.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LinearRgb;

    /// Tolerance for round-trip conversion (f32 trig functions)
    const ROUND_TRIP_TOLERANCE: f32 = 1e-6;

    /// Helper to check if two f32 values are approximately equal
    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn test_oklab_to_oklch_round_trip() {
        // Test colors with various hues and saturations
        let test_colors = [
            Oklab::new(0.5, 0.1, 0.0),    // Red-ish
            Oklab::new(0.5, 0.0, 0.1),    // Yellow-ish
            Oklab::new(0.5, -0.1, 0.0),   // Green-ish
            Oklab::new(0.5, 0.0, -0.1),   // Blue-ish
            Oklab::new(0.5, 0.1, 0.1),    // Orange-ish
            Oklab::new(0.5, -0.1, -0.1),  // Cyan-ish
            Oklab::new(0.8, 0.05, 0.02),  // Light saturated
            Oklab::new(0.2, -0.02, 0.05), // Dark saturated
        ];

        for original in test_colors {
            let oklch = Oklch::from(original);
            let round_trip = Oklab::from(oklch);

            assert!(
                approx_eq(original.l, round_trip.l, ROUND_TRIP_TOLERANCE),
                "L round-trip failed: original={}, round_trip={}",
                original.l,
                round_trip.l
            );
            assert!(
                approx_eq(original.a, round_trip.a, ROUND_TRIP_TOLERANCE),
                "a round-trip failed for {:?}: original={}, round_trip={}",
                original,
                original.a,
                round_trip.a
            );
            assert!(
                approx_eq(original.b, round_trip.b, ROUND_TRIP_TOLERANCE),
                "b round-trip failed for {:?}: original={}, round_trip={}",
                original,
                original.b,
                round_trip.b
            );
        }
    }

    #[test]
    fn test_achromatic_no_nan() {
        // Pure gray: a=0, b=0
        let gray = Oklab::new(0.5, 0.0, 0.0);
        let oklch = Oklch::from(gray);

        // Should not produce NaN
        assert!(!oklch.l.is_nan(), "L should not be NaN");
        assert!(!oklch.c.is_nan(), "C should not be NaN");
        assert!(!oklch.h.is_nan(), "h should not be NaN");

        // Chroma should be zero for achromatic
        assert!(
            oklch.c.abs() < 1e-10,
            "Chroma should be zero for gray, got {}",
            oklch.c
        );

        // Round trip should work
        let round_trip = Oklab::from(oklch);
        assert!(approx_eq(gray.l, round_trip.l, ROUND_TRIP_TOLERANCE));
        assert!(approx_eq(gray.a, round_trip.a, ROUND_TRIP_TOLERANCE));
        assert!(approx_eq(gray.b, round_trip.b, ROUND_TRIP_TOLERANCE));
    }

    #[test]
    fn test_black_and_white() {
        // Black
        let black = Oklab::new(0.0, 0.0, 0.0);
        let black_lch = Oklch::from(black);
        assert!(approx_eq(black_lch.l, 0.0, 1e-10), "Black L should be 0");
        assert!(approx_eq(black_lch.c, 0.0, 1e-10), "Black C should be 0");

        // White
        let white = Oklab::new(1.0, 0.0, 0.0);
        let white_lch = Oklch::from(white);
        assert!(approx_eq(white_lch.l, 1.0, 1e-10), "White L should be 1");
        assert!(approx_eq(white_lch.c, 0.0, 1e-10), "White C should be 0");
    }

    #[test]
    fn test_scale_chroma_double() {
        let original = Oklch {
            l: 0.5,
            c: 0.1,
            h: 1.0,
        };
        let scaled = original.scale_chroma(2.0);

        assert!(approx_eq(scaled.l, 0.5, 1e-10), "L should be unchanged");
        assert!(approx_eq(scaled.c, 0.2, 1e-10), "C should be doubled");
        assert!(approx_eq(scaled.h, 1.0, 1e-10), "h should be unchanged");
    }

    #[test]
    fn test_scale_chroma_half() {
        let original = Oklch {
            l: 0.5,
            c: 0.2,
            h: 2.0,
        };
        let scaled = original.scale_chroma(0.5);

        assert!(approx_eq(scaled.l, 0.5, 1e-10), "L should be unchanged");
        assert!(approx_eq(scaled.c, 0.1, 1e-10), "C should be halved");
        assert!(approx_eq(scaled.h, 2.0, 1e-10), "h should be unchanged");
    }

    #[test]
    fn test_scale_chroma_zero() {
        let original = Oklch {
            l: 0.5,
            c: 0.15,
            h: 0.5,
        };
        let scaled = original.scale_chroma(0.0);

        assert!(approx_eq(scaled.l, 0.5, 1e-10), "L should be unchanged");
        assert!(approx_eq(scaled.c, 0.0, 1e-10), "C should be zero");
        assert!(approx_eq(scaled.h, 0.5, 1e-10), "h should be unchanged");
    }

    #[test]
    fn test_scale_chroma_negative_clamps() {
        let original = Oklch {
            l: 0.5,
            c: 0.1,
            h: 1.0,
        };
        let scaled = original.scale_chroma(-1.0);

        assert!(approx_eq(scaled.l, 0.5, 1e-10), "L should be unchanged");
        assert!(approx_eq(scaled.c, 0.0, 1e-10), "C should clamp to 0");
        assert!(approx_eq(scaled.h, 1.0, 1e-10), "h should be unchanged");
    }

    #[test]
    fn test_chroma_calculation() {
        // Chroma = sqrt(a^2 + b^2)
        let lab = Oklab::new(0.5, 0.3, 0.4);
        let lch = Oklch::from(lab);

        let expected_c = (0.3_f32 * 0.3 + 0.4 * 0.4).sqrt(); // 0.5
        assert!(
            approx_eq(lch.c, expected_c, 1e-6),
            "Chroma calculation wrong: expected {}, got {}",
            expected_c,
            lch.c
        );
    }

    #[test]
    fn test_hue_calculation() {
        // Hue = atan2(b, a)
        // Pure +a direction (red): h = 0
        let red = Oklab::new(0.5, 0.1, 0.0);
        let red_lch = Oklch::from(red);
        assert!(
            approx_eq(red_lch.h, 0.0, 1e-6),
            "Pure +a should have h=0, got {}",
            red_lch.h
        );

        // Pure +b direction (yellow): h = pi/2
        let yellow = Oklab::new(0.5, 0.0, 0.1);
        let yellow_lch = Oklch::from(yellow);
        assert!(
            approx_eq(yellow_lch.h, std::f32::consts::FRAC_PI_2, 1e-6),
            "Pure +b should have h=pi/2, got {}",
            yellow_lch.h
        );

        // Pure -a direction (green): h = pi
        let green = Oklab::new(0.5, -0.1, 0.0);
        let green_lch = Oklch::from(green);
        assert!(
            approx_eq(green_lch.h.abs(), std::f32::consts::PI, 1e-6),
            "Pure -a should have h=pi or -pi, got {}",
            green_lch.h
        );

        // Pure -b direction (blue): h = -pi/2
        let blue = Oklab::new(0.5, 0.0, -0.1);
        let blue_lch = Oklch::from(blue);
        assert!(
            approx_eq(blue_lch.h, -std::f32::consts::FRAC_PI_2, 1e-6),
            "Pure -b should have h=-pi/2, got {}",
            blue_lch.h
        );
    }

    #[test]
    fn test_full_color_round_trip() {
        // Start from LinearRgb, go through Oklab -> Oklch -> Oklab -> LinearRgb
        let test_colors = [
            LinearRgb::new(0.8, 0.2, 0.1), // Orange-red
            LinearRgb::new(0.1, 0.6, 0.2), // Green
            LinearRgb::new(0.2, 0.3, 0.9), // Blue
            LinearRgb::new(0.5, 0.5, 0.5), // Gray
        ];

        for original in test_colors {
            let oklab1 = Oklab::from(original);
            let oklch = Oklch::from(oklab1);
            let oklab2 = Oklab::from(oklch);
            let final_rgb = LinearRgb::from(oklab2);

            assert!(
                approx_eq(original.r, final_rgb.r, 1e-5),
                "R round-trip failed: original={}, final={}",
                original.r,
                final_rgb.r
            );
            assert!(
                approx_eq(original.g, final_rgb.g, 1e-5),
                "G round-trip failed: original={}, final={}",
                original.g,
                final_rgb.g
            );
            assert!(
                approx_eq(original.b, final_rgb.b, 1e-5),
                "B round-trip failed: original={}, final={}",
                original.b,
                final_rgb.b
            );
        }
    }

    #[test]
    fn test_saturation_boost_preserves_hue() {
        // Boost saturation and verify hue is preserved
        let original = Oklab::new(0.6, 0.1, 0.15);
        let oklch = Oklch::from(original);
        let boosted = oklch.scale_chroma(1.5);

        // Hue should be exactly the same
        assert!(
            approx_eq(oklch.h, boosted.h, 1e-10),
            "Hue should be preserved: original={}, boosted={}",
            oklch.h,
            boosted.h
        );

        // Lightness should be exactly the same
        assert!(
            approx_eq(oklch.l, boosted.l, 1e-10),
            "Lightness should be preserved: original={}, boosted={}",
            oklch.l,
            boosted.l
        );

        // Chroma should be 1.5x
        assert!(
            approx_eq(boosted.c, oklch.c * 1.5, 1e-10),
            "Chroma should be 1.5x: expected {}, got {}",
            oklch.c * 1.5,
            boosted.c
        );
    }
}
