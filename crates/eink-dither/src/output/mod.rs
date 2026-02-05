//! Output types and rendering intents for the dithering pipeline.
//!
//! This module provides [`DitheredImage`], the canonical output of all
//! dithering operations, and [`RenderingIntent`], the primary entry point
//! that orchestrates preprocessing and dithering for different content types.
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
//!
//! # Rendering Intents
//!
//! [`RenderingIntent`] selects the optimal pipeline for content type:
//!
//! - **Photo**: Atkinson error diffusion with saturation/contrast boost
//! - **Graphics**: Blue noise ordered dithering with no enhancement

mod dithered_image;
mod render;

pub use dithered_image::DitheredImage;
pub use render::RenderingIntent;
