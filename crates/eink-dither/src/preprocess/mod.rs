//! Image preprocessing for e-ink dithering.
//!
//! This module provides preprocessing operations that optimize source images
//! for e-ink display characteristics before dithering. The complete pipeline:
//!
//! 1. **Resize** (Lanczos3) - High-quality scaling to target dimensions
//! 2. **Exact match detection** - Flag pixels matching palette colors
//! 3. **Saturation boost** (Oklch) - Perceptually correct chroma scaling
//! 4. **Contrast adjustment** (linear RGB) - Midpoint-centered scaling
//!
//! # Processing Order
//!
//! Resize happens **first** (before enhancement) per the e-ink dithering guide.
//! This ensures:
//! - Lanczos resampling sees the original color values
//! - Enhancement is applied at the target resolution
//! - Exact match detection works on resized pixels
//!
//! Note: Resize may destroy exact matches in photos - this is expected since
//! photographic content rarely has exact palette colors anyway.
//!
//! # Design Philosophy
//!
//! E-ink displays have limited color gamuts and muted appearance. Preprocessing
//! compensates by boosting saturation and contrast before dithering, resulting
//! in more vibrant output while maintaining perceptual accuracy.
//!
//! # Exact Match Preservation
//!
//! A key feature is detecting and preserving exact palette matches. Pixels that
//! exactly match a palette color (by sRGB bytes) are:
//! - Flagged in [`PreprocessResult::exact_matches`]
//! - NOT enhanced (saturation/contrast skipped)
//! - Ready to be passed through dithering without error diffusion
//!
//! This keeps text and UI elements crisp while photos get full enhancement.
//!
//! # Complete Workflow Example
//!
//! ```
//! use eink_dither::{Srgb, Palette, Preprocessor, PreprocessOptions};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! // Configure preprocessing
//! let options = PreprocessOptions::new().saturation(1.2).contrast(1.1);
//! let preprocessor = Preprocessor::new(&palette, options);
//!
//! // Load your image pixels (example: 10x10 mid-gray)
//! let input_pixels: Vec<Srgb> = vec![Srgb::from_u8(128, 128, 128); 10 * 10];
//!
//! // Preprocess: detect matches -> enhance
//! let result = preprocessor.process(&input_pixels, 10, 10);
//!
//! assert_eq!(result.width, 10);
//! assert_eq!(result.height, 10);
//! assert_eq!(result.pixels.len(), 10 * 10);
//! ```
//!
//! # Exact Match Example
//!
//! Pixels that exactly match palette colors are preserved without enhancement:
//!
//! ```
//! use eink_dither::{Srgb, Palette, Preprocessor, PreprocessOptions};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! let options = PreprocessOptions::new();
//! let preprocessor = Preprocessor::new(&palette, options);
//!
//! // Black pixels stay exactly black
//! let input = vec![Srgb::from_u8(0, 0, 0); 100];
//! let result = preprocessor.process(&input, 10, 10);
//!
//! // All pixels match palette exactly
//! assert!(result.exact_matches.iter().all(|m| m.is_some()));
//! ```

mod oklch;
mod options;
mod preprocessor;
mod resize;

#[cfg(test)]
pub(crate) use oklch::Oklch;
pub use options::PreprocessOptions;
pub use preprocessor::{PreprocessResult, Preprocessor};
