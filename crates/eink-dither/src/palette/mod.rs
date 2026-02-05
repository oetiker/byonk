//! Palette types and utilities
//!
//! This module provides types for working with color palettes, including
//! error types for parsing and validation.

mod error;
mod palette;

pub use error::{PaletteError, ParseColorError};
pub use palette::{DistanceMetric, Palette};
