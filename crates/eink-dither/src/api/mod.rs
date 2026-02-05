//! Public API for the eink-dither crate.
//!
//! This module provides the high-level API: [`EinkDitherer`] builder and
//! [`DitherError`] unified error type.

mod builder;
mod error;

pub use builder::EinkDitherer;
pub use error::DitherError;
