//! Output types for the dithering pipeline.
//!
//! This module provides [`DitheredImage`], the canonical output of all
//! dithering operations.
//!
//! # Output Formats
//!
//! [`DitheredImage`] stores palette indices with dimension metadata
//! and an owned [`Palette`](crate::palette::Palette), offering three output
//! formats on demand:
//!
//! - **Indexed** ([`DitheredImage::indices`]): Raw `u8` palette indices
//! - **Official RGB** ([`DitheredImage::to_rgb_official`]): Device upload colors
//! - **Actual RGB** ([`DitheredImage::to_rgb_actual`]): True appearance preview

mod dithered_image;

pub use dithered_image::DitheredImage;
