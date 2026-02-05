//! Error types for palette operations
//!
//! This module provides error types for color parsing and palette validation.

use std::fmt;
use std::num::ParseIntError;

/// Error type for parsing hex color strings.
///
/// Returned when parsing a hex color string fails, either due to
/// invalid length or invalid hexadecimal characters.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseColorError {
    /// Hex string has invalid length (must be 3 or 6 characters after stripping '#')
    InvalidLength,
    /// Invalid hexadecimal character encountered
    InvalidHex(ParseIntError),
}

impl From<ParseIntError> for ParseColorError {
    fn from(err: ParseIntError) -> Self {
        ParseColorError::InvalidHex(err)
    }
}

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseColorError::InvalidLength => {
                write!(f, "invalid hex color length (expected 3 or 6 characters)")
            }
            ParseColorError::InvalidHex(err) => {
                write!(f, "invalid hex character: {}", err)
            }
        }
    }
}

impl std::error::Error for ParseColorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseColorError::InvalidHex(err) => Some(err),
            _ => None,
        }
    }
}

/// Error type for palette validation.
///
/// Returned when palette configuration is invalid, such as empty palettes,
/// duplicate colors, or mismatched array lengths.
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteError {
    /// No colors provided in palette
    EmptyPalette,
    /// Duplicate color found at the specified index
    DuplicateColor {
        /// Index where the duplicate was found
        index: usize,
    },
    /// Official and actual palette lengths don't match
    LengthMismatch {
        /// Length of the official palette
        official: usize,
        /// Length of the actual palette
        actual: usize,
    },
    /// Invalid hex color string
    ParseColor(ParseColorError),
}

impl From<ParseColorError> for PaletteError {
    fn from(err: ParseColorError) -> Self {
        PaletteError::ParseColor(err)
    }
}

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaletteError::EmptyPalette => {
                write!(f, "palette cannot be empty")
            }
            PaletteError::DuplicateColor { index } => {
                write!(f, "duplicate color found at index {}", index)
            }
            PaletteError::LengthMismatch { official, actual } => {
                write!(
                    f,
                    "palette length mismatch: official has {} colors, actual has {}",
                    official, actual
                )
            }
            PaletteError::ParseColor(err) => {
                write!(f, "invalid color: {}", err)
            }
        }
    }
}

impl std::error::Error for PaletteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PaletteError::ParseColor(err) => Some(err),
            _ => None,
        }
    }
}
