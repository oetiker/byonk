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
//! # Presets
//!
//! Two presets cover common use cases:
//!
//! - [`PreprocessOptions::photo()`]: Saturation 1.5, contrast 1.1 - for photographs
//! - [`PreprocessOptions::graphics()`]: No enhancement - for logos, text, UI
//!
//! # Complete Workflow Example
//!
//! ```ignore
//! use eink_dither::{
//!     Srgb, Palette, Preprocessor, PreprocessOptions, PreprocessResult,
//!     Atkinson, Dither, DitherOptions,
//! };
//!
//! // 1. Define your e-ink palette
//! let colors = [
//!     Srgb::from_u8(0, 0, 0),       // Black
//!     Srgb::from_u8(255, 255, 255), // White
//! ];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! // 2. Configure preprocessing for photos with resize
//! let preprocess_opts = PreprocessOptions::photo()
//!     .resize(10, 10);  // Target e-ink display size
//!
//! // 3. Create preprocessor
//! let preprocessor = Preprocessor::new(&palette, preprocess_opts);
//!
//! // 4. Load your image pixels (example: 20x20 mid-gray)
//! let input_pixels: Vec<Srgb> = vec![Srgb::from_u8(128, 128, 128); 20 * 20];
//!
//! // 5. Preprocess: resize -> detect matches -> enhance
//! let result: PreprocessResult = preprocessor.process(&input_pixels, 20, 20);
//!
//! // Result contains resized dimensions
//! assert_eq!(result.width, 10);
//! assert_eq!(result.height, 10);
//! assert_eq!(result.pixels.len(), 10 * 10);
//!
//! // 6. Dither the preprocessed image
//! let dither_opts = DitherOptions::new();
//! let indices = Atkinson.dither(
//!     &result.pixels,
//!     result.width,
//!     result.height,
//!     &palette,
//!     &dither_opts,
//! );
//!
//! // indices contains palette indices for each pixel
//! assert_eq!(indices.len(), 10 * 10);
//! ```
//!
//! # Graphics Mode Example
//!
//! For logos and UI elements, use graphics mode (no enhancement):
//!
//! ```
//! use eink_dither::{Srgb, Palette, Preprocessor, PreprocessOptions};
//!
//! let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
//! let palette = Palette::new(&colors, None).unwrap();
//!
//! // Graphics preset: saturation=1.0, contrast=1.0 (no enhancement)
//! let options = PreprocessOptions::graphics();
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

pub use options::PreprocessOptions;
pub use preprocessor::{PreprocessResult, Preprocessor};
