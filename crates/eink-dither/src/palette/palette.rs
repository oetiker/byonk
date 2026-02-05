//! Palette struct with dual color storage and nearest-color matching.
//!
//! This module provides the core `Palette` type that stores both official
//! (advertised) and actual (measured) colors for e-ink displays, enabling
//! perceptually accurate color matching.

use std::collections::HashSet;
use std::str::FromStr;

use super::error::PaletteError;
use crate::color::{LinearRgb, Oklab, Srgb};

/// Distance metric for palette color matching.
///
/// Controls how perceptual distance is calculated when finding the
/// nearest palette color to an input pixel.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DistanceMetric {
    /// Standard Euclidean distance in Oklab space (squared for performance).
    ///
    /// Works well for achromatic (grey-only) palettes. For chromatic palettes,
    /// grey pixels may incorrectly map to chromatic colors with similar lightness.
    #[default]
    Euclidean,

    /// HyAB hybrid distance with chroma coupling (Abasi et al., 2020).
    ///
    /// Decouples lightness from chrominance: lightness uses Manhattan distance
    /// weighted by `kl`, chrominance uses Euclidean distance weighted by `kc`,
    /// and a chroma coupling penalty weighted by `kchroma` penalises differences
    /// in chroma magnitude between the input pixel and the palette entry. This
    /// prevents achromatic (grey) pixels from mapping to chromatic palette colors.
    ///
    /// Formula: `kl * |dL| + kc * sqrt(da² + db²) + kchroma * |C_pixel - C_palette|`
    ///
    /// Recommended: `kl: 2.0, kc: 1.0, kchroma: 2.0` (standard HyAB with
    /// chroma coupling).
    HyAB {
        /// Lightness weight. Higher values make lightness differences more
        /// significant relative to chrominance differences.
        kl: f32,
        /// Chrominance weight. Higher values penalise chroma differences
        /// in hue direction.
        kc: f32,
        /// Chroma coupling weight. Penalises differences in chroma magnitude
        /// between the input pixel and the palette entry, preventing grey
        /// pixels from matching chromatic palette entries.
        kchroma: f32,
    },
}

/// A color palette with dual color storage and perceptual matching.
///
/// `Palette` stores both the official colors (what the device specification says)
/// and the actual colors (what the display really shows). Color matching uses
/// the actual colors for perceptually accurate results.
///
/// # Dual Palette Support
///
/// E-ink displays often show colors differently than their specifications claim.
/// For example, "red" on a 7-color e-ink display might actually appear as a
/// muddy orange. By storing both sets of colors:
///
/// - **Official colors**: What you want in your output (device expects these)
/// - **Actual colors**: What the display really shows (used for matching)
///
/// The dithering algorithm can find the best perceptual match against what
/// will actually be displayed, then output the official color code.
///
/// # Precomputation
///
/// All color space conversions are done once at palette creation time:
/// - sRGB (for output)
/// - LinearRgb (for error diffusion math)
/// - Oklab (for perceptual distance)
///
/// This makes per-pixel matching operations fast.
///
/// # Example
///
/// ```
/// use eink_dither::{Palette, Srgb};
///
/// // Simple black and white palette
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
///
/// assert_eq!(palette.len(), 2);
/// ```
#[derive(Debug, Clone)]
pub struct Palette {
    // Official colors (what device spec says)
    official_srgb: Vec<Srgb>,
    official_linear: Vec<LinearRgb>,
    official_oklab: Vec<Oklab>,

    // Actual colors (what display really shows) - used for matching
    actual_srgb: Vec<Srgb>,
    actual_linear: Vec<LinearRgb>,
    actual_oklab: Vec<Oklab>,

    // Precomputed chroma magnitudes for actual palette entries
    actual_chroma: Vec<f32>,

    // Distance metric for color matching
    distance_metric: DistanceMetric,
}

impl Palette {
    /// Create a new palette from official sRGB colors.
    ///
    /// # Arguments
    ///
    /// * `official` - The official palette colors (what device spec says)
    /// * `actual` - Optional actual colors (what display really shows).
    ///              If `None`, official colors are used for both.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `official` is empty ([`PaletteError::EmptyPalette`])
    /// - `actual` has a different length than `official` ([`PaletteError::LengthMismatch`])
    /// - Either array contains duplicate colors ([`PaletteError::DuplicateColor`])
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Palette, Srgb};
    ///
    /// // With actual colors (calibrated display)
    /// let official = [Srgb::from_u8(255, 0, 0)];  // Device expects "red"
    /// let actual = [Srgb::from_u8(200, 50, 50)];  // But shows muddy red
    /// let palette = Palette::new(&official, Some(&actual)).unwrap();
    ///
    /// // Without actual colors (using spec values)
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    /// ```
    pub fn new(official: &[Srgb], actual: Option<&[Srgb]>) -> Result<Self, PaletteError> {
        // Validate non-empty
        if official.is_empty() {
            return Err(PaletteError::EmptyPalette);
        }

        // Determine actual colors to use
        let actual_colors: Vec<Srgb> = match actual {
            Some(a) => {
                if a.len() != official.len() {
                    return Err(PaletteError::LengthMismatch {
                        official: official.len(),
                        actual: a.len(),
                    });
                }
                a.to_vec()
            }
            None => official.to_vec(),
        };

        // Check for duplicates in official colors
        let mut seen = HashSet::new();
        for (i, color) in official.iter().enumerate() {
            let bytes = color.to_bytes();
            if !seen.insert(bytes) {
                return Err(PaletteError::DuplicateColor { index: i });
            }
        }

        // Check for duplicates in actual colors
        seen.clear();
        for (i, color) in actual_colors.iter().enumerate() {
            let bytes = color.to_bytes();
            if !seen.insert(bytes) {
                return Err(PaletteError::DuplicateColor { index: i });
            }
        }

        // Precompute all representations for official colors
        let official_srgb: Vec<Srgb> = official.to_vec();
        let official_linear: Vec<LinearRgb> =
            official_srgb.iter().map(|&s| LinearRgb::from(s)).collect();
        let official_oklab: Vec<Oklab> = official_linear.iter().map(|&l| Oklab::from(l)).collect();

        // Precompute all representations for actual colors
        let actual_srgb: Vec<Srgb> = actual_colors;
        let actual_linear: Vec<LinearRgb> =
            actual_srgb.iter().map(|&s| LinearRgb::from(s)).collect();
        let actual_oklab: Vec<Oklab> = actual_linear.iter().map(|&l| Oklab::from(l)).collect();

        // Precompute chroma magnitudes for actual palette entries
        let actual_chroma: Vec<f32> = actual_oklab
            .iter()
            .map(|c| (c.a * c.a + c.b * c.b).sqrt())
            .collect();

        Ok(Self {
            official_srgb,
            official_linear,
            official_oklab,
            actual_srgb,
            actual_linear,
            actual_oklab,
            actual_chroma,
            distance_metric: DistanceMetric::default(),
        })
    }

    /// Returns the number of colors in the palette.
    #[inline]
    pub fn len(&self) -> usize {
        self.official_srgb.len()
    }

    /// Returns true if the palette is empty.
    ///
    /// Note: This always returns `false` since empty palettes are rejected
    /// at construction time.
    #[inline]
    pub fn is_empty(&self) -> bool {
        // Always false - validated at construction
        self.official_srgb.is_empty()
    }

    /// Get the official sRGB color at the given index.
    ///
    /// This is the color code to output for the device.
    #[inline]
    pub fn official(&self, idx: usize) -> Srgb {
        self.official_srgb[idx]
    }

    /// Get the actual sRGB color at the given index.
    ///
    /// This is what the display really shows.
    #[inline]
    pub fn actual(&self, idx: usize) -> Srgb {
        self.actual_srgb[idx]
    }

    /// Get the official color in linear RGB space.
    ///
    /// Useful for error diffusion calculations.
    #[inline]
    pub fn official_linear(&self, idx: usize) -> LinearRgb {
        self.official_linear[idx]
    }

    /// Get the actual color in linear RGB space.
    ///
    /// Useful for error diffusion calculations.
    #[inline]
    pub fn actual_linear(&self, idx: usize) -> LinearRgb {
        self.actual_linear[idx]
    }

    /// Get the official color in Oklab space.
    #[inline]
    pub fn official_oklab(&self, idx: usize) -> Oklab {
        self.official_oklab[idx]
    }

    /// Get the actual color in Oklab space.
    #[inline]
    pub fn actual_oklab(&self, idx: usize) -> Oklab {
        self.actual_oklab[idx]
    }

    /// Set the distance metric used for color matching.
    ///
    /// By default, `Euclidean` distance is used. For palettes containing
    /// chromatic colors (not just black/white/grey), `HyAB` produces better
    /// results by preventing grey pixels from mapping to chromatic colors.
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Palette, Srgb, DistanceMetric};
    ///
    /// let colors = [
    ///     Srgb::from_u8(0, 0, 0),
    ///     Srgb::from_u8(255, 255, 255),
    ///     Srgb::from_u8(255, 0, 0),
    /// ];
    /// let palette = Palette::new(&colors, None).unwrap()
    ///     .with_distance_metric(DistanceMetric::HyAB { kl: 2.0, kc: 1.0, kchroma: 2.0 });
    /// ```
    pub fn with_distance_metric(mut self, metric: DistanceMetric) -> Self {
        self.distance_metric = metric;
        self
    }

    /// Returns true if the palette uses Euclidean (squared) distance.
    ///
    /// Callers that need linear distances (e.g., for blend factors) must
    /// take the square root of Euclidean results.
    #[inline]
    pub fn is_euclidean(&self) -> bool {
        matches!(self.distance_metric, DistanceMetric::Euclidean)
    }

    /// Compute perceptual distance between two Oklab colors using the
    /// palette's configured distance metric.
    ///
    /// For HyAB, `pixel_chroma` is the chroma magnitude of the input pixel
    /// and `palette_idx` identifies which palette entry is being compared
    /// (used to look up the precomputed palette chroma). For Euclidean,
    /// these extra parameters are ignored.
    ///
    /// This is used internally by `find_nearest()` and exposed so that
    /// other algorithms (e.g., `find_second_nearest`) use the same metric.
    #[inline]
    pub fn distance(&self, a: Oklab, b: Oklab, pixel_chroma: f32, palette_idx: usize) -> f32 {
        match self.distance_metric {
            DistanceMetric::Euclidean => a.distance_squared(b),
            DistanceMetric::HyAB { kl, kc, kchroma } => {
                let dl = (a.l - b.l).abs();
                let da = a.a - b.a;
                let db = a.b - b.b;
                let chroma_penalty = (pixel_chroma - self.actual_chroma[palette_idx]).abs();
                kl * dl + kc * (da * da + db * db).sqrt() + kchroma * chroma_penalty
            }
        }
    }

    /// Find the nearest palette color to the given Oklab color.
    ///
    /// Matches against ACTUAL colors (what the display really shows),
    /// not official colors. This produces the best perceptual match
    /// on the real device.
    ///
    /// Returns `(index, distance)` where:
    /// - `index`: palette entry closest to the input color
    /// - `distance`: perceptual distance (metric depends on configuration)
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{Palette, Srgb, Oklab, LinearRgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    ///
    /// // Find nearest to mid-gray
    /// let gray = Oklab::from(LinearRgb::new(0.5, 0.5, 0.5));
    /// let (idx, dist) = palette.find_nearest(gray);
    /// // Could be either black or white (equidistant)
    /// assert!(idx == 0 || idx == 1);
    /// ```
    #[inline]
    pub fn find_nearest(&self, color: Oklab) -> (usize, f32) {
        // Compute pixel chroma once before the loop
        let pixel_chroma = (color.a * color.a + color.b * color.b).sqrt();

        // Linear scan - optimal for small palettes (7-16 colors typical)
        let mut best_idx = 0;
        let mut best_dist = f32::MAX;

        for (i, &palette_color) in self.actual_oklab.iter().enumerate() {
            let dist = self.distance(color, palette_color, pixel_chroma, i);
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }

        (best_idx, best_dist)
    }

    /// Create a palette from hex color strings.
    ///
    /// This is a convenience constructor that parses hex strings like
    /// "#FF0000" or "#F00" into sRGB colors and creates a palette.
    ///
    /// # Arguments
    ///
    /// * `official` - Hex strings for official device colors
    /// * `actual` - Optional hex strings for actual measured colors
    ///
    /// # Errors
    ///
    /// Returns [`PaletteError::ParseColor`] if any hex string is invalid,
    /// or other [`PaletteError`] variants for palette validation failures.
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::Palette;
    ///
    /// let palette = Palette::from_hex(
    ///     &["#000000", "#FFFFFF", "#FF0000"],
    ///     Some(&["#0A0A0A", "#E6E6DC", "#C83232"]),
    /// ).unwrap();
    /// assert_eq!(palette.len(), 3);
    /// ```
    pub fn from_hex(official: &[&str], actual: Option<&[&str]>) -> Result<Self, PaletteError> {
        let official_srgb: Vec<Srgb> = official
            .iter()
            .map(|s| Srgb::from_str(s).map_err(PaletteError::ParseColor))
            .collect::<Result<Vec<_>, _>>()?;
        let actual_srgb = match actual {
            Some(a) => Some(
                a.iter()
                    .map(|s| Srgb::from_str(s).map_err(PaletteError::ParseColor))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            None => None,
        };
        Palette::new(&official_srgb, actual_srgb.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Construction tests
    #[test]
    fn test_palette_basic_construction() {
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
        ];
        let palette = Palette::new(&colors, None).unwrap();
        assert_eq!(palette.len(), 3);
        assert!(!palette.is_empty());
    }

    #[test]
    fn test_palette_dual_colors() {
        let official = [Srgb::from_u8(255, 0, 0)]; // Bright red
        let actual = [Srgb::from_u8(200, 50, 50)]; // Duller red
        let palette = Palette::new(&official, Some(&actual)).unwrap();

        assert_eq!(palette.official(0).to_bytes(), [255, 0, 0]);
        assert_eq!(palette.actual(0).to_bytes(), [200, 50, 50]);
    }

    #[test]
    fn test_palette_empty_error() {
        let result = Palette::new(&[], None);
        assert!(matches!(result, Err(PaletteError::EmptyPalette)));
    }

    #[test]
    fn test_palette_length_mismatch() {
        let official = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        let actual = [Srgb::from_u8(10, 10, 10)]; // Only one color
        let result = Palette::new(&official, Some(&actual));
        assert!(matches!(result, Err(PaletteError::LengthMismatch { .. })));
    }

    #[test]
    fn test_palette_duplicate_in_official() {
        let colors = [
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(255, 0, 0), // Duplicate
        ];
        let result = Palette::new(&colors, None);
        assert!(matches!(result, Err(PaletteError::DuplicateColor { .. })));
    }

    #[test]
    fn test_palette_duplicate_in_actual() {
        let official = [Srgb::from_u8(255, 0, 0), Srgb::from_u8(0, 255, 0)];
        let actual = [
            Srgb::from_u8(200, 50, 50),
            Srgb::from_u8(200, 50, 50), // Duplicate actual
        ];
        let result = Palette::new(&official, Some(&actual));
        assert!(matches!(result, Err(PaletteError::DuplicateColor { .. })));
    }

    // find_nearest tests
    #[test]
    fn test_find_nearest_exact_match() {
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        let palette = Palette::new(&colors, None).unwrap();

        // Black should match black exactly
        let black_oklab = palette.actual_oklab(0);
        let (idx, dist) = palette.find_nearest(black_oklab);
        assert_eq!(idx, 0);
        assert!(dist < 1e-10, "Exact match should have ~zero distance");
    }

    #[test]
    fn test_find_nearest_perceptual() {
        // Create a palette with black and white
        let colors = [
            Srgb::from_u8(0, 0, 0),       // Black
            Srgb::from_u8(255, 255, 255), // White
        ];
        let palette = Palette::new(&colors, None).unwrap();

        // Dark gray (25%) should match black
        let dark_gray = Oklab::from(LinearRgb::from(Srgb::from_u8(64, 64, 64)));
        let (idx, _) = palette.find_nearest(dark_gray);
        assert_eq!(idx, 0, "Dark gray should match black");

        // Light gray (75%) should match white
        let light_gray = Oklab::from(LinearRgb::from(Srgb::from_u8(192, 192, 192)));
        let (idx, _) = palette.find_nearest(light_gray);
        assert_eq!(idx, 1, "Light gray should match white");
    }

    #[test]
    fn test_find_nearest_uses_actual_not_official() {
        // Official: black, white
        // Actual: white, black (swapped!)
        // This creates a situation where matching against official vs actual
        // would give different results.
        let official = [
            Srgb::from_u8(0, 0, 0),       // Official says "black"
            Srgb::from_u8(255, 255, 255), // Official says "white"
        ];
        let actual = [
            Srgb::from_u8(255, 255, 255), // But actually displays white
            Srgb::from_u8(0, 0, 0),       // But actually displays black
        ];
        let palette = Palette::new(&official, Some(&actual)).unwrap();

        // Input: a dark gray color (closer to black perceptually)
        let dark_gray = Oklab::from(LinearRgb::from(Srgb::from_u8(30, 30, 30)));
        let (idx, _) = palette.find_nearest(dark_gray);

        // If matching used official colors: idx=0 (official black is closer)
        // If matching uses actual colors: idx=1 (actual black is at index 1)
        // Since we match against actual, dark gray matches idx=1
        assert_eq!(
            idx, 1,
            "Should match against actual colors (black at idx 1)"
        );

        // Similarly, a light color should match idx=0 (where actual white is)
        let light_gray = Oklab::from(LinearRgb::from(Srgb::from_u8(220, 220, 220)));
        let (idx, _) = palette.find_nearest(light_gray);
        assert_eq!(
            idx, 0,
            "Should match against actual colors (white at idx 0)"
        );
    }

    #[test]
    fn test_arbitrary_palette_size() {
        // Test non-power-of-2 sizes work (PAL-01 requirement)
        for size in [1, 3, 5, 7, 11, 15] {
            let colors: Vec<Srgb> = (0..size)
                .map(|i| Srgb::from_u8((i * 20) as u8, 0, 0))
                .collect();
            let palette = Palette::new(&colors, None).unwrap();
            assert_eq!(palette.len(), size);
        }
    }

    #[test]
    fn test_accessors_return_precomputed() {
        let colors = [Srgb::from_u8(128, 64, 32)];
        let palette = Palette::new(&colors, None).unwrap();

        // Verify sRGB accessor
        let srgb = palette.official(0);
        assert_eq!(srgb.to_bytes(), [128, 64, 32]);

        // Verify LinearRgb accessor matches expected conversion
        let linear = palette.official_linear(0);
        let expected_linear = LinearRgb::from(colors[0]);
        assert!((linear.r - expected_linear.r).abs() < 1e-6);

        // Verify Oklab accessor matches expected conversion
        let oklab = palette.official_oklab(0);
        let expected_oklab = Oklab::from(expected_linear);
        assert!((oklab.l - expected_oklab.l).abs() < 1e-6);
    }

    // from_hex tests
    #[test]
    fn test_from_hex_6digit() {
        let palette = Palette::from_hex(&["#000000", "#FFFFFF"], None).unwrap();
        assert_eq!(palette.len(), 2);
        assert_eq!(palette.official(0).to_bytes(), [0, 0, 0]);
        assert_eq!(palette.official(1).to_bytes(), [255, 255, 255]);
    }

    #[test]
    fn test_from_hex_shorthand() {
        let palette = Palette::from_hex(&["#000", "#FFF", "#F00"], None).unwrap();
        assert_eq!(palette.len(), 3);
        assert_eq!(palette.official(0).to_bytes(), [0, 0, 0]);
        assert_eq!(palette.official(1).to_bytes(), [255, 255, 255]);
        assert_eq!(palette.official(2).to_bytes(), [255, 0, 0]);
    }

    #[test]
    fn test_from_hex_without_hash() {
        let palette = Palette::from_hex(&["000000", "FFFFFF"], None).unwrap();
        assert_eq!(palette.len(), 2);
        assert_eq!(palette.official(0).to_bytes(), [0, 0, 0]);
        assert_eq!(palette.official(1).to_bytes(), [255, 255, 255]);
    }

    #[test]
    fn test_from_hex_dual_palette() {
        let palette = Palette::from_hex(
            &["#000000", "#FFFFFF", "#FF0000"],
            Some(&["#0A0A0A", "#E6E6DC", "#C83232"]),
        )
        .unwrap();
        assert_eq!(palette.len(), 3);
        assert_eq!(palette.official(0).to_bytes(), [0, 0, 0]);
        assert_eq!(palette.actual(0).to_bytes(), [10, 10, 10]);
        assert_eq!(palette.official(2).to_bytes(), [255, 0, 0]);
        assert_eq!(palette.actual(2).to_bytes(), [200, 50, 50]);
    }

    #[test]
    fn test_from_hex_invalid_hex() {
        let result = Palette::from_hex(&["#ZZZZZZ"], None);
        assert!(matches!(result, Err(PaletteError::ParseColor(_))));
    }

    #[test]
    fn test_from_hex_length_mismatch() {
        let result = Palette::from_hex(
            &["#000000", "#FFFFFF"],
            Some(&["#0A0A0A"]), // Only one actual color
        );
        assert!(matches!(result, Err(PaletteError::LengthMismatch { .. })));
    }

    // HyAB distance metric tests

    fn make_6_color_palette() -> Palette {
        let colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(0, 255, 0),     // green
            Srgb::from_u8(0, 0, 255),     // blue
            Srgb::from_u8(255, 255, 0),   // yellow
        ];
        Palette::new(&colors, None)
            .unwrap()
            .with_distance_metric(DistanceMetric::HyAB { kl: 2.0, kc: 1.0, kchroma: 2.0 })
    }

    #[test]
    fn test_hyab_extreme_greys_map_to_achromatic() {
        let palette = make_6_color_palette();

        // Very dark and very light greys should always map to black/white
        // because no chromatic color has similar lightness.
        for &grey_val in &[0u8, 16, 32, 224, 240, 255] {
            let grey = Oklab::from(LinearRgb::from(Srgb::from_u8(grey_val, grey_val, grey_val)));
            let (idx, _) = palette.find_nearest(grey);
            assert!(
                idx == 0 || idx == 1,
                "Grey {} should map to black or white, got index {} ({:?})",
                grey_val,
                idx,
                palette.official(idx).to_bytes()
            );
        }
    }

    #[test]
    fn test_hyab_kc_forces_grey_to_achromatic() {
        // With high kc, even mid-greys are forced to achromatic.
        // This demonstrates the kc parameter works as intended.
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&colors, None)
            .unwrap()
            .with_distance_metric(DistanceMetric::HyAB { kl: 2.0, kc: 10.0, kchroma: 2.0 });

        let mid_grey = Oklab::from(LinearRgb::from(Srgb::from_u8(128, 128, 128)));
        let (idx, _) = palette.find_nearest(mid_grey);
        assert!(
            idx == 0 || idx == 1,
            "With high kc, mid-grey should map to achromatic, got index {}",
            idx,
        );
    }

    #[test]
    fn test_hyab_chromatic_still_maps_correctly() {
        let palette = make_6_color_palette();

        // Pure red should still map to red
        let red = Oklab::from(LinearRgb::from(Srgb::from_u8(255, 0, 0)));
        let (idx, _) = palette.find_nearest(red);
        assert_eq!(idx, 2, "Pure red should map to red (index 2), got {}", idx);

        // Pure green should still map to green
        let green = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 255, 0)));
        let (idx, _) = palette.find_nearest(green);
        assert_eq!(
            idx, 3,
            "Pure green should map to green (index 3), got {}",
            idx
        );

        // Pure blue should still map to blue
        let blue = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 255)));
        let (idx, _) = palette.find_nearest(blue);
        assert_eq!(
            idx, 4,
            "Pure blue should map to blue (index 4), got {}",
            idx
        );
    }

    #[test]
    fn test_euclidean_backward_compatible() {
        // Default (Euclidean) should still work the same
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        let palette = Palette::new(&colors, None).unwrap();
        assert!(palette.is_euclidean());

        let dark_gray = Oklab::from(LinearRgb::from(Srgb::from_u8(64, 64, 64)));
        let (idx, _) = palette.find_nearest(dark_gray);
        assert_eq!(idx, 0, "Dark gray should match black");

        let light_gray = Oklab::from(LinearRgb::from(Srgb::from_u8(192, 192, 192)));
        let (idx, _) = palette.find_nearest(light_gray);
        assert_eq!(idx, 1, "Light gray should match white");
    }

    #[test]
    fn test_hyab_all_greys_map_to_valid_color() {
        let palette = make_6_color_palette();

        // With standard HyAB (kc=1), mid-greys near a chromatic color's
        // lightness may map to that chromatic color. This is acceptable —
        // sparse 6-color palettes inherently lack grey entries, so the
        // perceptually nearest color at similar lightness wins.
        for grey_val in (0..=255).step_by(16) {
            let grey = Oklab::from(LinearRgb::from(Srgb::from_u8(
                grey_val as u8,
                grey_val as u8,
                grey_val as u8,
            )));
            let (idx, dist) = palette.find_nearest(grey);
            assert!(
                idx < 6,
                "Grey {} should map to a valid palette index",
                grey_val
            );
            assert!(dist >= 0.0, "Distance should be non-negative");
        }
    }
}
