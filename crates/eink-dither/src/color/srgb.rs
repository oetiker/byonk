//! sRGB color type
//!
//! sRGB is the standard color space for display and storage of images.
//! It applies a gamma curve to linear light values for perceptual uniformity.

use std::str::FromStr;

use super::linear_rgb::LinearRgb;
use super::lut::linear_to_srgb;

// Re-export path for ParseColorError - will be wired through crate root
use crate::palette::ParseColorError;

/// A color in sRGB color space.
///
/// sRGB is the standard color space for image storage and display.
/// It applies gamma correction to make the perceptual brightness steps
/// appear uniform. Use this type for input/output (loading images, writing pixels).
///
/// Values are in the range 0.0..=1.0 (mapping to 0..255 for 8-bit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgb {
    /// Red channel (gamma-corrected, 0.0..=1.0)
    pub r: f32,
    /// Green channel (gamma-corrected, 0.0..=1.0)
    pub g: f32,
    /// Blue channel (gamma-corrected, 0.0..=1.0)
    pub b: f32,
}

impl Srgb {
    /// Create a new Srgb color from float values.
    ///
    /// # Arguments
    /// * `r` - Red channel (0.0..=1.0)
    /// * `g` - Green channel (0.0..=1.0)
    /// * `b` - Blue channel (0.0..=1.0)
    #[inline]
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Create an Srgb color from 8-bit unsigned integer values.
    ///
    /// # Arguments
    /// * `r` - Red channel (0..=255)
    /// * `g` - Green channel (0..=255)
    /// * `b` - Blue channel (0..=255)
    ///
    /// # Example
    /// ```
    /// use eink_dither::Srgb;
    /// let red = Srgb::from_u8(255, 0, 0);
    /// assert_eq!(red.r, 1.0);
    /// ```
    #[inline]
    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
        }
    }

    /// Create an Srgb color from a byte array [R, G, B].
    ///
    /// # Example
    /// ```
    /// use eink_dither::Srgb;
    /// let white = Srgb::from_bytes([255, 255, 255]);
    /// assert_eq!(white.r, 1.0);
    /// ```
    #[inline]
    pub fn from_bytes(bytes: [u8; 3]) -> Self {
        Self::from_u8(bytes[0], bytes[1], bytes[2])
    }

    /// Convert to a byte array [R, G, B].
    ///
    /// Rounds and clamps values to the 0..=255 range.
    ///
    /// # Example
    /// ```
    /// use eink_dither::Srgb;
    /// let color = Srgb::new(1.0, 0.5, 0.0);
    /// let bytes = color.to_bytes();
    /// assert_eq!(bytes[0], 255); // red
    /// assert_eq!(bytes[2], 0);   // blue
    /// ```
    #[inline]
    pub fn to_bytes(self) -> [u8; 3] {
        [
            (self.r * 255.0).round().clamp(0.0, 255.0) as u8,
            (self.g * 255.0).round().clamp(0.0, 255.0) as u8,
            (self.b * 255.0).round().clamp(0.0, 255.0) as u8,
        ]
    }
}

impl From<LinearRgb> for Srgb {
    /// Convert from linear RGB to sRGB using the gamma lookup table.
    ///
    /// This conversion is necessary after performing color calculations,
    /// to encode the result for display or storage.
    fn from(linear: LinearRgb) -> Self {
        Self {
            r: linear_to_srgb(linear.r),
            g: linear_to_srgb(linear.g),
            b: linear_to_srgb(linear.b),
        }
    }
}

impl FromStr for Srgb {
    type Err = ParseColorError;

    /// Parse an sRGB color from a hex string.
    ///
    /// Supports the following formats:
    /// - `#RRGGBB` - standard 6-digit hex with hash
    /// - `RRGGBB` - standard 6-digit hex without hash
    /// - `#RGB` - shorthand 3-digit hex with hash (expands to RRGGBB)
    /// - `RGB` - shorthand 3-digit hex without hash
    ///
    /// Parsing is case-insensitive. Leading and trailing whitespace is trimmed.
    ///
    /// # Examples
    ///
    /// ```
    /// use eink_dither::Srgb;
    ///
    /// let white: Srgb = "#FFFFFF".parse().unwrap();
    /// assert_eq!(white.r, 1.0);
    ///
    /// let red: Srgb = "#F00".parse().unwrap();
    /// assert_eq!(red.r, 1.0);
    /// assert_eq!(red.g, 0.0);
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let s = s.strip_prefix('#').unwrap_or(s);

        match s.len() {
            3 => {
                // Shorthand: expand each digit by multiplying by 17 (0xF -> 0xFF)
                let r = u8::from_str_radix(&s[0..1], 16)? * 17;
                let g = u8::from_str_radix(&s[1..2], 16)? * 17;
                let b = u8::from_str_radix(&s[2..3], 16)? * 17;
                Ok(Self::from_u8(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16)?;
                let g = u8::from_str_radix(&s[2..4], 16)?;
                let b = u8::from_str_radix(&s[4..6], 16)?;
                Ok(Self::from_u8(r, g, b))
            }
            _ => Err(ParseColorError::InvalidLength),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::ParseColorError;

    /// Test round-trip accuracy: u8 -> Srgb -> LinearRgb -> Srgb -> u8
    /// Should have max 1 LSB error for all 256 values.
    #[test]
    fn test_srgb_round_trip_accuracy() {
        let mut max_error = 0i32;

        for i in 0..=255u8 {
            let original = Srgb::from_u8(i, i, i);
            let linear = LinearRgb::from(original);
            let back = Srgb::from(linear);
            let bytes = back.to_bytes();

            let error = (bytes[0] as i32 - i as i32).abs();
            max_error = max_error.max(error);

            assert!(
                error <= 1,
                "Round-trip error too large for value {i}: got {}, expected {i}, error {error}",
                bytes[0]
            );
        }

        println!("Max round-trip error: {max_error} LSB");
        assert!(max_error <= 1, "Max error {max_error} exceeds 1 LSB");
    }

    /// Test constructor behavior with key values.
    #[test]
    fn test_srgb_constructors() {
        // from_u8 produces correct float values
        let color = Srgb::from_u8(255, 128, 0);
        assert_eq!(color.r, 1.0);
        assert!((color.g - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(color.b, 0.0);

        // from_bytes matches from_u8
        let from_bytes = Srgb::from_bytes([255, 128, 0]);
        assert_eq!(from_bytes, color);

        // to_bytes round-trips correctly for key values
        assert_eq!(Srgb::from_u8(0, 0, 0).to_bytes(), [0, 0, 0]);
        assert_eq!(Srgb::from_u8(127, 127, 127).to_bytes(), [127, 127, 127]);
        assert_eq!(Srgb::from_u8(128, 128, 128).to_bytes(), [128, 128, 128]);
        assert_eq!(Srgb::from_u8(255, 255, 255).to_bytes(), [255, 255, 255]);
    }

    /// Test known gamma conversion values against the IEC 61966-2-1 formula.
    #[test]
    fn test_known_gamma_values() {
        // sRGB 0.0 -> linear 0.0
        let black = Srgb::new(0.0, 0.0, 0.0);
        let linear_black = LinearRgb::from(black);
        assert!((linear_black.r).abs() < 1e-6);

        // sRGB 1.0 -> linear 1.0
        let white = Srgb::new(1.0, 1.0, 1.0);
        let linear_white = LinearRgb::from(white);
        assert!((linear_white.r - 1.0).abs() < 1e-6);

        // sRGB 0.5 -> linear ~0.214
        // Exact: ((0.5 + 0.055) / 1.055)^2.4 = 0.214041...
        let mid_gray_srgb = Srgb::new(0.5, 0.5, 0.5);
        let mid_gray_linear = LinearRgb::from(mid_gray_srgb);
        assert!(
            (mid_gray_linear.r - 0.214).abs() < 0.001,
            "sRGB 0.5 -> linear expected ~0.214, got {}",
            mid_gray_linear.r
        );

        // linear 0.5 -> sRGB ~0.735
        // Exact: 1.055 * 0.5^(1/2.4) - 0.055 = 0.735356...
        let linear_mid = LinearRgb::new(0.5, 0.5, 0.5);
        let srgb_mid = Srgb::from(linear_mid);
        assert!(
            (srgb_mid.r - 0.735).abs() < 0.001,
            "linear 0.5 -> sRGB expected ~0.735, got {}",
            srgb_mid.r
        );
    }

    /// Test that Srgb and LinearRgb are distinct types.
    /// This is a compile-time check demonstrating type safety.
    #[test]
    fn test_type_safety() {
        let srgb = Srgb::new(0.5, 0.5, 0.5);
        let linear = LinearRgb::new(0.5, 0.5, 0.5);

        // These are different types - they can't be compared directly
        // (This test documents the API, it would fail to compile if we tried
        // to assign one to the other without conversion)

        // Explicit conversion is required
        let srgb_from_linear = Srgb::from(linear);
        let linear_from_srgb = LinearRgb::from(srgb);

        // The converted values are different (because gamma!)
        assert!(srgb_from_linear.r != srgb.r);
        assert!(linear_from_srgb.r != linear.r);

        // Verify the values are reasonable
        assert!(srgb_from_linear.r > 0.5); // gamma expansion
        assert!(linear_from_srgb.r < 0.5); // gamma compression
    }

    /// Test hex parsing with standard 6-digit format.
    #[test]
    fn test_hex_parsing_6digit() {
        // #FFFFFF -> white
        let white: Srgb = "#FFFFFF".parse().unwrap();
        assert_eq!(white.r, 1.0);
        assert_eq!(white.g, 1.0);
        assert_eq!(white.b, 1.0);

        // #000000 -> black
        let black: Srgb = "#000000".parse().unwrap();
        assert_eq!(black.r, 0.0);
        assert_eq!(black.g, 0.0);
        assert_eq!(black.b, 0.0);

        // #FF0000 -> red
        let red: Srgb = "#FF0000".parse().unwrap();
        assert_eq!(red.r, 1.0);
        assert_eq!(red.g, 0.0);
        assert_eq!(red.b, 0.0);

        // FFFFFF (no hash) -> white
        let white_no_hash: Srgb = "FFFFFF".parse().unwrap();
        assert_eq!(white_no_hash.r, 1.0);
        assert_eq!(white_no_hash.g, 1.0);
        assert_eq!(white_no_hash.b, 1.0);
    }

    /// Test hex parsing with 3-digit shorthand format.
    #[test]
    fn test_hex_parsing_shorthand() {
        // #FFF (shorthand) -> white
        let white: Srgb = "#FFF".parse().unwrap();
        assert_eq!(white.r, 1.0);
        assert_eq!(white.g, 1.0);
        assert_eq!(white.b, 1.0);

        // #f00 (shorthand lowercase) -> red
        let red: Srgb = "#f00".parse().unwrap();
        assert_eq!(red.r, 1.0);
        assert_eq!(red.g, 0.0);
        assert_eq!(red.b, 0.0);

        // #ABC -> expanded to #AABBCC
        let color: Srgb = "#ABC".parse().unwrap();
        assert_eq!(color, Srgb::from_u8(0xAA, 0xBB, 0xCC));
    }

    /// Test hex parsing error cases.
    #[test]
    fn test_hex_parsing_errors() {
        // #GGG -> InvalidHex error (invalid character)
        let result = "#GGG".parse::<Srgb>();
        assert!(matches!(result, Err(ParseColorError::InvalidHex(_))));

        // #FFFF (4 chars) -> InvalidLength error
        let result = "#FFFF".parse::<Srgb>();
        assert!(matches!(result, Err(ParseColorError::InvalidLength)));

        // Empty string -> InvalidLength
        let result = "".parse::<Srgb>();
        assert!(matches!(result, Err(ParseColorError::InvalidLength)));

        // Just hash -> InvalidLength
        let result = "#".parse::<Srgb>();
        assert!(matches!(result, Err(ParseColorError::InvalidLength)));
    }

    /// Test hex parsing handles whitespace.
    #[test]
    fn test_hex_parsing_whitespace() {
        // "  #FFFFFF  " (whitespace) -> white
        let white: Srgb = "  #FFFFFF  ".parse().unwrap();
        assert_eq!(white.r, 1.0);
        assert_eq!(white.g, 1.0);
        assert_eq!(white.b, 1.0);
    }

    /// Test hex parsing is case-insensitive.
    #[test]
    fn test_hex_parsing_case_insensitive() {
        let upper: Srgb = "#ABCDEF".parse().unwrap();
        let lower: Srgb = "#abcdef".parse().unwrap();
        let mixed: Srgb = "#AbCdEf".parse().unwrap();

        assert_eq!(upper, lower);
        assert_eq!(upper, mixed);
    }
}
