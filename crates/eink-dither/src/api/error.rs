//! Unified error type for the eink-dither public API.
//!
//! [`DitherError`] wraps all error types from the crate into a single enum
//! for convenient `?` propagation in application code.

use crate::palette::{PaletteError, ParseColorError};
use std::fmt;

/// Unified error type for the eink-dither public API.
///
/// Wraps all error types from the crate into a single enum for
/// convenient `?` propagation in application code.
///
/// # Example
///
/// ```
/// use eink_dither::{DitherError, Palette};
///
/// fn create_palette() -> Result<Palette, DitherError> {
///     let palette = Palette::from_hex(&["#000000", "#FFFFFF"], None)?;
///     Ok(palette)
/// }
/// ```
#[derive(Debug)]
pub enum DitherError {
    /// Palette validation error (empty, duplicate, length mismatch, or parse error)
    Palette(PaletteError),
    /// Color parsing error (invalid hex string)
    ParseColor(ParseColorError),
}

impl fmt::Display for DitherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DitherError::Palette(err) => write!(f, "palette error: {}", err),
            DitherError::ParseColor(err) => write!(f, "color parse error: {}", err),
        }
    }
}

impl std::error::Error for DitherError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DitherError::Palette(err) => Some(err),
            DitherError::ParseColor(err) => Some(err),
        }
    }
}

impl From<PaletteError> for DitherError {
    fn from(err: PaletteError) -> Self {
        DitherError::Palette(err)
    }
}

impl From<ParseColorError> for DitherError {
    fn from(err: ParseColorError) -> Self {
        DitherError::ParseColor(err)
    }
}
