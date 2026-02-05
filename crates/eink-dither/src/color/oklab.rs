//! Oklab perceptual color space
//!
//! Oklab is a perceptual color space designed for uniform color perception.
//! It is used for calculating perceptual color differences (e.g., finding
//! the nearest palette color).
//!
//! # References
//!
//! Björn Ottosson, "A perceptual color space for image processing"
//! <https://bottosson.github.io/posts/oklab/>

use super::linear_rgb::LinearRgb;

/// A color in Oklab perceptual color space.
///
/// Oklab provides perceptually uniform distances - equal numerical differences
/// correspond to equal perceived differences. This makes it ideal for:
/// - Finding the perceptually nearest color in a palette
/// - Error diffusion dithering
/// - Color interpolation
///
/// # Components
///
/// - `l`: Lightness (0.0 = black, 1.0 = white for in-gamut colors)
/// - `a`: Green-red axis (negative = green, positive = red)
/// - `b`: Blue-yellow axis (negative = blue, positive = yellow)
///
/// # Note
///
/// Values are not clamped. Out-of-gamut colors (from error diffusion) may
/// have components outside typical ranges. This is intentional to preserve
/// accuracy during intermediate calculations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklab {
    /// Lightness: 0.0 (black) to 1.0 (white) for in-gamut colors
    pub l: f32,
    /// Green-red axis: typically -0.5 to 0.5
    pub a: f32,
    /// Blue-yellow axis: typically -0.5 to 0.5
    pub b: f32,
}

impl Oklab {
    /// Create a new Oklab color.
    ///
    /// # Arguments
    /// * `l` - Lightness (typically 0.0..=1.0)
    /// * `a` - Green-red axis (typically -0.5..=0.5)
    /// * `b` - Blue-yellow axis (typically -0.5..=0.5)
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::Oklab;
    ///
    /// // Create a mid-gray color (neutral, no chroma)
    /// let gray = Oklab::new(0.5, 0.0, 0.0);
    /// ```
    #[inline]
    pub fn new(l: f32, a: f32, b: f32) -> Self {
        Self { l, a, b }
    }

    /// Squared Euclidean distance in Oklab space (perceptual distance metric).
    ///
    /// Use squared distance to avoid sqrt when comparing distances.
    /// For actual distance, take the square root of this result.
    ///
    /// This is the foundation for palette matching - finding the perceptually
    /// nearest color in a limited palette.
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::Oklab;
    ///
    /// let white = Oklab::new(1.0, 0.0, 0.0);
    /// let black = Oklab::new(0.0, 0.0, 0.0);
    /// let gray = Oklab::new(0.5, 0.0, 0.0);
    ///
    /// // Gray is equidistant from black and white
    /// let d_to_black = gray.distance_squared(black);
    /// let d_to_white = gray.distance_squared(white);
    /// assert!((d_to_black - d_to_white).abs() < 1e-6);
    /// ```
    #[inline]
    pub fn distance_squared(self, other: Oklab) -> f32 {
        let dl = self.l - other.l;
        let da = self.a - other.a;
        let db = self.b - other.b;
        dl * dl + da * da + db * db
    }

    /// HyAB perceptual distance (Abasi et al., 2020).
    ///
    /// Decouples lightness from chrominance for better palette matching
    /// with chromatic palettes. Uses Manhattan distance for lightness and
    /// Euclidean distance for chrominance, added together:
    ///
    /// `distance = kL * |L1 - L2| + sqrt((a1-a2)^2 + (b1-b2)^2)`
    ///
    /// With `kL > 1`, lightness differences dominate, preventing grey pixels
    /// from incorrectly mapping to chromatic colors that happen to have
    /// similar lightness.
    ///
    /// # Arguments
    ///
    /// * `other` - The color to measure distance to
    /// * `kl` - Lightness weight (typically 2.0; higher = more lightness-sensitive)
    #[inline]
    pub fn hyab_distance(self, other: Oklab, kl: f32) -> f32 {
        let dl = (self.l - other.l).abs();
        let da = self.a - other.a;
        let db = self.b - other.b;
        kl * dl + (da * da + db * db).sqrt()
    }
}

impl From<LinearRgb> for Oklab {
    /// Convert from linear RGB to Oklab.
    ///
    /// Uses the updated 2021-01-25 matrices from Björn Ottosson.
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{LinearRgb, Oklab};
    ///
    /// let linear = LinearRgb::new(0.5, 0.5, 0.5);
    /// let oklab = Oklab::from(linear);
    /// // Gray has near-zero a and b (no chroma)
    /// assert!(oklab.a.abs() < 0.001);
    /// assert!(oklab.b.abs() < 0.001);
    /// ```
    fn from(rgb: LinearRgb) -> Self {
        // Step 1: Linear sRGB to LMS (M1 matrix)
        let l = 0.4122214708 * rgb.r + 0.5363325363 * rgb.g + 0.0514459929 * rgb.b;
        let m = 0.2119034982 * rgb.r + 0.6806995451 * rgb.g + 0.1073969566 * rgb.b;
        let s = 0.0883024619 * rgb.r + 0.2817188376 * rgb.g + 0.6299787005 * rgb.b;

        // Step 2: Cube root (nonlinearity)
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();

        // Step 3: LMS to Lab (M2 matrix)
        Oklab {
            l: 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
            a: 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
            b: 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
        }
    }
}

impl From<Oklab> for LinearRgb {
    /// Convert from Oklab to linear RGB.
    ///
    /// Uses the inverse matrices to reverse the Oklab transformation.
    ///
    /// # Note
    ///
    /// The result is not clamped. Out-of-gamut Oklab colors will produce
    /// LinearRgb values outside 0.0..=1.0.
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{LinearRgb, Oklab};
    ///
    /// let oklab = Oklab::new(0.5, 0.0, 0.0);
    /// let linear = LinearRgb::from(oklab);
    /// // Neutral gray has equal RGB components
    /// assert!((linear.r - linear.g).abs() < 0.001);
    /// assert!((linear.g - linear.b).abs() < 0.001);
    /// ```
    fn from(lab: Oklab) -> Self {
        // Step 1: Lab to LMS (inverse M2)
        let l_ = lab.l + 0.3963377774 * lab.a + 0.2158037573 * lab.b;
        let m_ = lab.l - 0.1055613458 * lab.a - 0.0638541728 * lab.b;
        let s_ = lab.l - 0.0894841775 * lab.a - 1.2914855480 * lab.b;

        // Step 2: Cube (reverse nonlinearity)
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        // Step 3: LMS to linear sRGB (inverse M1)
        LinearRgb {
            r: 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            g: -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            b: -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Srgb;

    /// Tolerance for palette crate comparison (single matrix transform)
    const PALETTE_TOLERANCE: f32 = 1e-6;

    /// Tolerance for round-trip through two matrix transforms (f32 accumulates error)
    const ROUND_TRIP_TOLERANCE: f32 = 1e-5;

    /// Helper to check if two f32 values are approximately equal
    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn test_oklab_matches_palette_crate() {
        use palette::{IntoColor, LinSrgb, Oklab as PaletteOklab};

        // Test colors: primaries, white, black, mid-gray
        let test_colors = [
            (1.0, 0.0, 0.0), // Red
            (0.0, 1.0, 0.0), // Green
            (0.0, 0.0, 1.0), // Blue
            (0.5, 0.5, 0.5), // Mid gray
            (1.0, 1.0, 1.0), // White
            (0.0, 0.0, 0.0), // Black
        ];

        for (r, g, b) in test_colors {
            // Our implementation
            let our_linear = LinearRgb::new(r, g, b);
            let our_oklab = Oklab::from(our_linear);

            // Palette crate implementation
            let palette_linear: LinSrgb<f32> = LinSrgb::new(r, g, b);
            let palette_oklab: PaletteOklab<f32> = palette_linear.into_color();

            assert!(
                approx_eq(our_oklab.l, palette_oklab.l, PALETTE_TOLERANCE),
                "L mismatch for ({}, {}, {}): ours={}, palette={}",
                r,
                g,
                b,
                our_oklab.l,
                palette_oklab.l
            );
            assert!(
                approx_eq(our_oklab.a, palette_oklab.a, PALETTE_TOLERANCE),
                "a mismatch for ({}, {}, {}): ours={}, palette={}",
                r,
                g,
                b,
                our_oklab.a,
                palette_oklab.a
            );
            assert!(
                approx_eq(our_oklab.b, palette_oklab.b, PALETTE_TOLERANCE),
                "b mismatch for ({}, {}, {}): ours={}, palette={}",
                r,
                g,
                b,
                our_oklab.b,
                palette_oklab.b
            );
        }
    }

    #[test]
    fn test_oklab_round_trip() {
        // Test colors including primaries, secondaries, grays
        let test_colors = [
            (1.0, 0.0, 0.0), // Red
            (0.0, 1.0, 0.0), // Green
            (0.0, 0.0, 1.0), // Blue
            (1.0, 1.0, 0.0), // Yellow
            (1.0, 0.0, 1.0), // Magenta
            (0.0, 1.0, 1.0), // Cyan
            (0.5, 0.5, 0.5), // Mid gray
            (0.25, 0.25, 0.25),
            (0.75, 0.75, 0.75),
            (1.0, 1.0, 1.0), // White
            (0.0, 0.0, 0.0), // Black
        ];

        for (r, g, b) in test_colors {
            let original = LinearRgb::new(r, g, b);
            let oklab = Oklab::from(original);
            let round_trip = LinearRgb::from(oklab);

            assert!(
                approx_eq(original.r, round_trip.r, ROUND_TRIP_TOLERANCE),
                "R round-trip failed for ({}, {}, {}): original={}, round_trip={}",
                r,
                g,
                b,
                original.r,
                round_trip.r
            );
            assert!(
                approx_eq(original.g, round_trip.g, ROUND_TRIP_TOLERANCE),
                "G round-trip failed for ({}, {}, {}): original={}, round_trip={}",
                r,
                g,
                b,
                original.g,
                round_trip.g
            );
            assert!(
                approx_eq(original.b, round_trip.b, ROUND_TRIP_TOLERANCE),
                "B round-trip failed for ({}, {}, {}): original={}, round_trip={}",
                r,
                g,
                b,
                original.b,
                round_trip.b
            );
        }
    }

    #[test]
    fn test_oklab_known_values() {
        // White should have L close to 1.0, a and b close to 0.0
        let white = Oklab::from(LinearRgb::new(1.0, 1.0, 1.0));
        assert!(
            approx_eq(white.l, 1.0, PALETTE_TOLERANCE),
            "White L should be 1.0, got {}",
            white.l
        );
        assert!(
            approx_eq(white.a, 0.0, PALETTE_TOLERANCE),
            "White a should be 0.0, got {}",
            white.a
        );
        assert!(
            approx_eq(white.b, 0.0, PALETTE_TOLERANCE),
            "White b should be 0.0, got {}",
            white.b
        );

        // Black should have L close to 0.0, a and b close to 0.0
        let black = Oklab::from(LinearRgb::new(0.0, 0.0, 0.0));
        assert!(
            approx_eq(black.l, 0.0, PALETTE_TOLERANCE),
            "Black L should be 0.0, got {}",
            black.l
        );
        assert!(
            approx_eq(black.a, 0.0, PALETTE_TOLERANCE),
            "Black a should be 0.0, got {}",
            black.a
        );
        assert!(
            approx_eq(black.b, 0.0, PALETTE_TOLERANCE),
            "Black b should be 0.0, got {}",
            black.b
        );

        // Mid gray should have a and b close to 0.0 (achromatic)
        let gray = Oklab::from(LinearRgb::new(0.5, 0.5, 0.5));
        assert!(
            approx_eq(gray.a, 0.0, PALETTE_TOLERANCE),
            "Gray a should be 0.0, got {}",
            gray.a
        );
        assert!(
            approx_eq(gray.b, 0.0, PALETTE_TOLERANCE),
            "Gray b should be 0.0, got {}",
            gray.b
        );
    }

    #[test]
    fn test_full_conversion_chain() {
        // Start with sRGB (255, 128, 64)
        let original_srgb = Srgb::from_u8(255, 128, 64);

        // Full chain: Srgb -> LinearRgb -> Oklab -> LinearRgb -> Srgb
        let linear1 = LinearRgb::from(original_srgb);
        let oklab = Oklab::from(linear1);
        let linear2 = LinearRgb::from(oklab);

        // Clamp to valid range before sRGB conversion (matrix math may produce
        // tiny out-of-range values due to f32 precision, e.g., 1.0000002)
        let linear2_clamped = LinearRgb::new(
            linear2.r.clamp(0.0, 1.0),
            linear2.g.clamp(0.0, 1.0),
            linear2.b.clamp(0.0, 1.0),
        );
        let final_srgb = Srgb::from(linear2_clamped);

        let original_bytes = original_srgb.to_bytes();
        let final_bytes = final_srgb.to_bytes();

        // Should be within 1 LSB (allow for floating point accumulation)
        let r_diff = (original_bytes[0] as i32 - final_bytes[0] as i32).abs();
        let g_diff = (original_bytes[1] as i32 - final_bytes[1] as i32).abs();
        let b_diff = (original_bytes[2] as i32 - final_bytes[2] as i32).abs();

        assert!(
            r_diff <= 1,
            "R channel differs by {} (original={}, final={})",
            r_diff,
            original_bytes[0],
            final_bytes[0]
        );
        assert!(
            g_diff <= 1,
            "G channel differs by {} (original={}, final={})",
            g_diff,
            original_bytes[1],
            final_bytes[1]
        );
        assert!(
            b_diff <= 1,
            "B channel differs by {} (original={}, final={})",
            b_diff,
            original_bytes[2],
            final_bytes[2]
        );
    }

    #[test]
    fn test_hyab_distance_known_values() {
        // Pure lightness difference (achromatic)
        let black = Oklab::new(0.0, 0.0, 0.0);
        let white = Oklab::new(1.0, 0.0, 0.0);
        // kl=2.0: 2.0 * |1.0 - 0.0| + 0.0 = 2.0
        let d = black.hyab_distance(white, 2.0);
        assert!(
            (d - 2.0).abs() < 1e-6,
            "HyAB black-white should be 2.0, got {}",
            d
        );

        // Pure chromatic difference (same lightness)
        let a = Oklab::new(0.5, 0.1, 0.0);
        let b = Oklab::new(0.5, -0.1, 0.0);
        // kl=2.0: 2.0 * 0.0 + sqrt(0.04) = 0.2
        let d = a.hyab_distance(b, 2.0);
        assert!(
            (d - 0.2).abs() < 1e-6,
            "HyAB pure chroma should be 0.2, got {}",
            d
        );
    }

    #[test]
    fn test_hyab_distance_symmetry() {
        let a = Oklab::new(0.6, 0.1, -0.05);
        let b = Oklab::new(0.3, -0.2, 0.1);
        let d_ab = a.hyab_distance(b, 2.0);
        let d_ba = b.hyab_distance(a, 2.0);
        assert!(
            (d_ab - d_ba).abs() < 1e-6,
            "HyAB should be symmetric: {} vs {}",
            d_ab,
            d_ba
        );
    }

    #[test]
    fn test_hyab_distance_identity() {
        let c = Oklab::new(0.7, 0.15, -0.1);
        let d = c.hyab_distance(c, 2.0);
        assert!(d.abs() < 1e-10, "HyAB self-distance should be 0, got {}", d);
    }

    #[test]
    fn test_oklab_distance() {
        let white = Oklab::new(1.0, 0.0, 0.0);
        let black = Oklab::new(0.0, 0.0, 0.0);
        let gray = Oklab::new(0.5, 0.0, 0.0);

        // Distance from black to white should be 1.0 (just L difference)
        assert!(
            (white.distance_squared(black) - 1.0).abs() < 1e-6,
            "Distance squared from white to black should be 1.0, got {}",
            white.distance_squared(black)
        );

        // Gray is equidistant from black and white
        let d_to_black = gray.distance_squared(black);
        let d_to_white = gray.distance_squared(white);
        assert!(
            (d_to_black - d_to_white).abs() < 1e-6,
            "Gray should be equidistant: d_to_black={}, d_to_white={}",
            d_to_black,
            d_to_white
        );

        // Distance to self is zero
        assert!(
            white.distance_squared(white) < 1e-10,
            "Distance to self should be zero, got {}",
            white.distance_squared(white)
        );

        // Test with chromatic components
        let red_ish = Oklab::new(0.5, 0.2, 0.0);
        let blue_ish = Oklab::new(0.5, 0.0, -0.2);
        // Same L, different a/b: distance should be sqrt(0.04 + 0.04) squared = 0.08
        let d_chroma = red_ish.distance_squared(blue_ish);
        assert!(
            (d_chroma - 0.08).abs() < 1e-6,
            "Chromatic distance should be 0.08, got {}",
            d_chroma
        );
    }
}
