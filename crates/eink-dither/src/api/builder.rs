//! EinkDitherer builder -- the primary ergonomic entry point for the crate.
//!
//! [`EinkDitherer`] wraps the dithering pipeline with fluent configuration
//! and optional preprocessing overrides.
//! Users configure palette, intent, and optional tweaks in a single chain,
//! then call [`dither()`](EinkDitherer::dither) for the complete pipeline.

use crate::color::Srgb;
use crate::dither::{
    Atkinson, BlueNoiseDither, Dither, DitherAlgorithm, DitherOptions, FloydSteinberg,
    FloydSteinbergNoise, SimplexDither,
};
use crate::output::{DitheredImage, RenderingIntent};
use crate::palette::Palette;
use crate::preprocess::{PreprocessOptions, Preprocessor};

/// High-level dithering builder for e-ink displays.
///
/// `EinkDitherer` is the recommended entry point for the crate. It wraps the
/// complete pipeline (preprocessing, dithering, output) behind a fluent builder
/// API with sensible defaults for each rendering intent.
///
/// # Design
///
/// - Constructor requires [`Palette`] + [`RenderingIntent`] (no invalid states)
/// - Configuration methods consume and return `self` (standard builder pattern)
/// - [`dither()`](Self::dither) takes `&self` so the builder is **reusable**
///   across multiple images
/// - Intent-specific defaults are set in the constructor but can be overridden
///
/// # Example
///
/// ```
/// use eink_dither::{EinkDitherer, Palette, RenderingIntent, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
///
/// let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo)
///     .saturation(1.8)
///     .contrast(1.2);
///
/// let pixels = vec![Srgb::from_u8(128, 128, 128); 4];
/// let result = ditherer.dither(&pixels, 2, 2);
///
/// assert_eq!(result.width(), 2);
/// assert_eq!(result.height(), 2);
/// ```
pub struct EinkDitherer {
    palette: Palette,
    intent: RenderingIntent,
    preprocess: PreprocessOptions,
    dither_opts: DitherOptions,
    algorithm: DitherAlgorithm,
}

impl EinkDitherer {
    /// Create a new ditherer with the given palette and rendering intent.
    ///
    /// Preprocessing defaults are selected based on the intent:
    /// - **Photo**: `PreprocessOptions::photo()` (saturation 1.2, contrast 1.1)
    /// - **Graphics**: `PreprocessOptions::graphics()` (no enhancement)
    ///
    /// Dither options default to `DitherOptions::new()` (serpentine, error clamp 0.5).
    ///
    /// # Arguments
    ///
    /// * `palette` - The e-ink display's color palette
    /// * `intent` - Rendering intent (Photo or Graphics)
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{EinkDitherer, Palette, RenderingIntent, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    ///
    /// let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo);
    /// ```
    pub fn new(palette: Palette, intent: RenderingIntent) -> Self {
        let preprocess = match intent {
            RenderingIntent::Photo => PreprocessOptions::photo(),
            RenderingIntent::Graphics => PreprocessOptions::graphics(),
        };
        Self {
            palette,
            intent,
            preprocess,
            dither_opts: DitherOptions::new(),
            algorithm: DitherAlgorithm::Auto,
        }
    }

    /// Set target dimensions for resize.
    ///
    /// The image will be resized to these exact dimensions using Lanczos3
    /// filtering before any preprocessing or dithering.
    ///
    /// # Arguments
    ///
    /// * `width` - Target width in pixels
    /// * `height` - Target height in pixels
    #[inline]
    pub fn resize(mut self, width: u32, height: u32) -> Self {
        self.preprocess = self.preprocess.resize(width, height);
        self
    }

    /// Set saturation multiplier.
    ///
    /// Saturation is adjusted in Oklch color space for perceptually correct
    /// results without hue shifts.
    ///
    /// - 1.0 = no change
    /// - 1.2 = default for Photo intent
    /// - 0.5 = reduce saturation by half
    ///
    /// # Arguments
    ///
    /// * `factor` - Saturation multiplier
    #[inline]
    pub fn saturation(mut self, factor: f32) -> Self {
        self.preprocess = self.preprocess.saturation(factor);
        self
    }

    /// Set contrast multiplier.
    ///
    /// Contrast is adjusted in linear RGB space around the midpoint (0.5).
    ///
    /// - 1.0 = no change
    /// - 1.1 = default for Photo intent
    /// - 1.5 = high contrast
    ///
    /// # Arguments
    ///
    /// * `factor` - Contrast multiplier
    #[inline]
    pub fn contrast(mut self, factor: f32) -> Self {
        self.preprocess = self.preprocess.contrast(factor);
        self
    }

    /// Set serpentine scanning mode.
    ///
    /// When enabled (default), odd rows are processed right-to-left to
    /// eliminate directional "worm" artifacts in error diffusion.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable serpentine scanning
    #[inline]
    pub fn serpentine(mut self, enabled: bool) -> Self {
        self.dither_opts = self.dither_opts.serpentine(enabled);
        self
    }

    /// Set error clamping threshold.
    ///
    /// Accumulated error is clamped to this range to prevent "blooming"
    /// with small palettes.
    ///
    /// # Arguments
    ///
    /// * `clamp` - Maximum error magnitude per channel (default: 0.5)
    #[inline]
    pub fn error_clamp(mut self, clamp: f32) -> Self {
        self.dither_opts = self.dither_opts.error_clamp(clamp);
        self
    }

    /// Set chromatic error clamping threshold.
    ///
    /// Controls how much per-channel error can deviate from the mean
    /// (achromatic) error during error diffusion. Lower values prevent
    /// color blowout in photos while preserving B&W dithering quality.
    ///
    /// # Arguments
    ///
    /// * `clamp` - Maximum chromatic deviation per channel (default: f32::INFINITY)
    #[inline]
    pub fn chroma_clamp(mut self, clamp: f32) -> Self {
        self.dither_opts = self.dither_opts.chroma_clamp(clamp);
        self
    }

    /// Set the dithering algorithm.
    ///
    /// Overrides the default algorithm for the rendering intent:
    /// - **Photo** default: [`DitherAlgorithm::Atkinson`]
    /// - **Graphics** default: [`DitherAlgorithm::BlueNoise`]
    ///
    /// Use [`DitherAlgorithm::Simplex`] for barycentric ordered dithering
    /// with up to 4-color blending per pixel (27% better accuracy than
    /// blue noise).
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The dithering algorithm to use
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{EinkDitherer, Palette, RenderingIntent, DitherAlgorithm, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    ///
    /// let ditherer = EinkDitherer::new(palette, RenderingIntent::Graphics)
    ///     .algorithm(DitherAlgorithm::Simplex);
    /// ```
    #[inline]
    pub fn algorithm(mut self, algorithm: DitherAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Dither raw sRGB pixels into a [`DitheredImage`].
    ///
    /// This is the primary processing method. It applies the full pipeline:
    /// 1. Preprocess (resize, saturation, contrast)
    /// 2. Dither (Atkinson for Photo, BlueNoise for Graphics)
    /// 3. Wrap in [`DitheredImage`]
    ///
    /// The builder is reusable -- `dither()` takes `&self` so you can call
    /// it multiple times with different images.
    ///
    /// # Arguments
    ///
    /// * `pixels` - Input pixels in sRGB space, row-major order
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    ///
    /// # Returns
    ///
    /// A [`DitheredImage`] with palette indices, dimensions, and the palette.
    ///
    /// # Panics (debug only)
    ///
    /// Debug-asserts that `pixels.len() == width * height`.
    pub fn dither(&self, pixels: &[Srgb], width: usize, height: usize) -> DitheredImage {
        // 1. Preprocess with the builder's custom options
        let preprocessor = Preprocessor::new(&self.palette, self.preprocess.clone());
        let result = preprocessor.process(pixels, width, height);

        // 2. Resolve algorithm (Auto uses intent defaults)
        let algorithm = match self.algorithm {
            DitherAlgorithm::Auto => match self.intent {
                RenderingIntent::Photo => DitherAlgorithm::Atkinson,
                RenderingIntent::Graphics => DitherAlgorithm::BlueNoise,
            },
            explicit => explicit,
        };

        // 3. Dither using the selected algorithm
        let indices = match algorithm {
            DitherAlgorithm::Auto => unreachable!("resolved above"),
            DitherAlgorithm::Atkinson => {
                // Use Euclidean distance for error diffusion — see
                // Palette::for_error_diffusion() for rationale.
                let photo_palette = self.palette.for_error_diffusion();
                Atkinson.dither(
                    &result.pixels,
                    result.width,
                    result.height,
                    &photo_palette,
                    &self.dither_opts,
                )
            }
            DitherAlgorithm::FloydSteinberg => {
                let photo_palette = self.palette.for_error_diffusion();
                FloydSteinberg.dither(
                    &result.pixels,
                    result.width,
                    result.height,
                    &photo_palette,
                    &self.dither_opts,
                )
            }
            DitherAlgorithm::BlueNoise => BlueNoiseDither.dither(
                &result.pixels,
                result.width,
                result.height,
                &self.palette,
                &self.dither_opts,
            ),
            DitherAlgorithm::Simplex => {
                let simplex = SimplexDither::new(&self.palette);
                simplex.dither(
                    &result.pixels,
                    result.width,
                    result.height,
                    &self.palette,
                    &self.dither_opts,
                )
            }
            DitherAlgorithm::FloydSteinbergNoise => {
                let photo_palette = self.palette.for_error_diffusion();
                FloydSteinbergNoise.dither(
                    &result.pixels,
                    result.width,
                    result.height,
                    &photo_palette,
                    &self.dither_opts,
                )
            }
        };

        // 4. Wrap in DitheredImage
        DitheredImage::new(indices, result.width, result.height, self.palette.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a 3-color palette with distinct official and actual colors.
    fn test_palette() -> Palette {
        let official = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red (official)
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),       // black (same)
            Srgb::from_u8(255, 255, 255), // white (same)
            Srgb::from_u8(200, 50, 50),   // muddy red (actual)
        ];
        Palette::new(&official, Some(&actual)).unwrap()
    }

    /// Helper: create a 4x4 gradient image (dark to light).
    fn gradient_4x4() -> Vec<Srgb> {
        (0..16)
            .map(|i| {
                let v = (i as f32 / 15.0 * 255.0) as u8;
                Srgb::from_u8(v, v, v)
            })
            .collect()
    }

    #[test]
    fn test_new_photo_defaults() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo);

        // Photo defaults: saturation 1.0, contrast 1.0 (no boost —
        // error diffusion naturally amplifies chroma)
        assert!(
            (ditherer.preprocess.saturation - 1.0).abs() < f32::EPSILON,
            "Photo should default to saturation 1.0"
        );
        assert!(
            (ditherer.preprocess.contrast - 1.0).abs() < f32::EPSILON,
            "Photo should default to contrast 1.0"
        );
    }

    #[test]
    fn test_new_graphics_defaults() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette, RenderingIntent::Graphics);

        // Graphics defaults: saturation 1.0, contrast 1.0
        assert!(
            (ditherer.preprocess.saturation - 1.0).abs() < f32::EPSILON,
            "Graphics should default to saturation 1.0"
        );
        assert!(
            (ditherer.preprocess.contrast - 1.0).abs() < f32::EPSILON,
            "Graphics should default to contrast 1.0"
        );
    }

    #[test]
    fn test_builder_chaining() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo)
            .resize(800, 600)
            .saturation(1.8)
            .contrast(1.2)
            .serpentine(false)
            .error_clamp(0.3);

        // Verify all values were set
        assert_eq!(ditherer.preprocess.target_width, Some(800));
        assert_eq!(ditherer.preprocess.target_height, Some(600));
        assert!((ditherer.preprocess.saturation - 1.8).abs() < f32::EPSILON);
        assert!((ditherer.preprocess.contrast - 1.2).abs() < f32::EPSILON);
        assert!(!ditherer.dither_opts.serpentine);
        assert!((ditherer.dither_opts.error_clamp - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_dither_produces_valid_output() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Photo);
        let pixels = gradient_4x4();

        let result = ditherer.dither(&pixels, 4, 4);

        assert_eq!(result.width(), 4);
        assert_eq!(result.height(), 4);
        assert_eq!(result.indices().len(), 16);

        // All indices must be valid
        for &idx in result.indices() {
            assert!(
                (idx as usize) < palette.len(),
                "Index {} out of palette range {}",
                idx,
                palette.len()
            );
        }
    }

    #[test]
    fn test_dither_reusable() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Photo);
        let pixels = gradient_4x4();

        // Call dither twice with the same builder
        let result1 = ditherer.dither(&pixels, 4, 4);
        let result2 = ditherer.dither(&pixels, 4, 4);

        // Both should be valid
        assert_eq!(result1.width(), 4);
        assert_eq!(result1.height(), 4);
        assert_eq!(result2.width(), 4);
        assert_eq!(result2.height(), 4);

        // Same input should produce same output (deterministic)
        assert_eq!(result1.indices(), result2.indices());
    }

    #[test]
    fn test_photo_and_graphics_differ() {
        let palette = test_palette();
        let pixels = gradient_4x4();

        let photo = EinkDitherer::new(palette.clone(), RenderingIntent::Photo);
        let graphics = EinkDitherer::new(palette, RenderingIntent::Graphics);

        let photo_result = photo.dither(&pixels, 4, 4);
        let graphics_result = graphics.dither(&pixels, 4, 4);

        // Different algorithms + different preprocessing should produce different output
        assert_ne!(
            photo_result.indices(),
            graphics_result.indices(),
            "Photo and Graphics should produce different dither patterns"
        );
    }

    #[test]
    fn test_custom_saturation_affects_output() {
        // Use a richer palette so saturation differences are visible
        let colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(0, 255, 0),     // green
            Srgb::from_u8(0, 0, 255),     // blue
        ];
        let palette = Palette::new(&colors, None).unwrap();

        // Desaturated pastel input that sits between palette colors
        let pixels: Vec<Srgb> = (0..16)
            .map(|i| {
                let r = 128u8.wrapping_add((i * 5) as u8);
                let g = 100u8.wrapping_add((i * 3) as u8);
                let b = 110u8.wrapping_add((i * 7) as u8);
                Srgb::from_u8(r, g, b)
            })
            .collect();

        let low_sat = EinkDitherer::new(palette.clone(), RenderingIntent::Photo).saturation(0.5);
        let high_sat = EinkDitherer::new(palette, RenderingIntent::Photo).saturation(3.0);

        let low_result = low_sat.dither(&pixels, 4, 4);
        let high_result = high_sat.dither(&pixels, 4, 4);

        // Very different saturation should produce different output with a rich palette
        assert_ne!(
            low_result.indices(),
            high_result.indices(),
            "Different saturation should produce different dither patterns on colorful input"
        );
    }
}
