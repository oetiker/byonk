//! EinkDitherer builder -- the primary ergonomic entry point for the crate.
//!
//! [`EinkDitherer`] wraps the dithering pipeline with fluent configuration
//! and optional preprocessing overrides.

use crate::color::Srgb;
use crate::dither::{dither_with_kernel_noise, DitherAlgorithm, DitherOptions};
use crate::output::DitheredImage;
use crate::palette::Palette;
use crate::preprocess::{PreprocessOptions, Preprocessor};

/// High-level dithering builder for e-ink displays.
///
/// `EinkDitherer` is the recommended entry point for the crate. It wraps the
/// complete pipeline (preprocessing, dithering, output) behind a fluent builder
/// API with sensible defaults.
///
/// # Design
///
/// - Constructor requires [`Palette`] (no invalid states)
/// - Configuration methods consume and return `self` (standard builder pattern)
/// - [`dither()`](Self::dither) takes `&self` so the builder is **reusable**
///   across multiple images
/// - Per-algorithm defaults are applied when `.algorithm()` is called
///
/// # Example
///
/// ```
/// use eink_dither::{EinkDitherer, Palette, Srgb};
///
/// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&colors, None).unwrap();
///
/// let ditherer = EinkDitherer::new(palette)
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
    preprocess: PreprocessOptions,
    dither_opts: DitherOptions,
    algorithm: DitherAlgorithm,
    /// Whether error_clamp was explicitly set by the user (vs algorithm default).
    error_clamp_explicit: bool,
}

impl EinkDitherer {
    /// Create a new ditherer with the given palette.
    ///
    /// Default algorithm is Atkinson with error_clamp=0.08.
    /// Preprocessing defaults: saturation 1.0, contrast 1.0 (no enhancement).
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{EinkDitherer, Palette, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    ///
    /// let ditherer = EinkDitherer::new(palette);
    /// ```
    pub fn new(palette: Palette) -> Self {
        Self {
            palette,
            preprocess: PreprocessOptions::default(),
            dither_opts: DitherOptions::new().error_clamp(0.08),
            algorithm: DitherAlgorithm::Atkinson,
            error_clamp_explicit: false,
        }
    }

    /// Set target dimensions for resize.
    #[inline]
    pub fn resize(mut self, width: u32, height: u32) -> Self {
        self.preprocess = self.preprocess.resize(width, height);
        self
    }

    /// Set saturation multiplier (Oklch space).
    #[inline]
    pub fn saturation(mut self, factor: f32) -> Self {
        self.preprocess = self.preprocess.saturation(factor);
        self
    }

    /// Set contrast multiplier (linear RGB space).
    #[inline]
    pub fn contrast(mut self, factor: f32) -> Self {
        self.preprocess = self.preprocess.contrast(factor);
        self
    }

    /// Set serpentine scanning mode.
    #[inline]
    pub fn serpentine(mut self, enabled: bool) -> Self {
        self.dither_opts = self.dither_opts.serpentine(enabled);
        self
    }

    /// Set error clamping threshold.
    ///
    /// This explicitly overrides the per-algorithm default and the
    /// greyscale palette auto-detection override.
    #[inline]
    pub fn error_clamp(mut self, clamp: f32) -> Self {
        self.dither_opts = self.dither_opts.error_clamp(clamp);
        self.error_clamp_explicit = true;
        self
    }

    /// Set chromatic error clamping threshold.
    #[inline]
    pub fn chroma_clamp(mut self, clamp: f32) -> Self {
        self.dither_opts = self.dither_opts.chroma_clamp(clamp);
        self
    }

    /// Set whether to preserve exact palette matches.
    #[inline]
    pub fn preserve_exact_matches(mut self, enabled: bool) -> Self {
        self.dither_opts = self.dither_opts.preserve_exact_matches(enabled);
        self.preprocess = self.preprocess.preserve_exact_matches(enabled);
        self
    }

    /// Set blue noise jitter scale.
    #[inline]
    pub fn noise_scale(mut self, scale: f32) -> Self {
        self.dither_opts = self.dither_opts.noise_scale(scale);
        self
    }

    /// Set whether exact-match pixels absorb accumulated error.
    #[inline]
    pub fn exact_absorb_error(mut self, absorb: bool) -> Self {
        self.dither_opts = self.dither_opts.exact_absorb_error(absorb);
        self
    }

    /// Set the dithering algorithm.
    ///
    /// Applies per-algorithm defaults for error_clamp and noise_scale.
    /// Subsequent `.error_clamp()` / `.noise_scale()` calls override these.
    ///
    /// # Example
    ///
    /// ```
    /// use eink_dither::{EinkDitherer, Palette, DitherAlgorithm, Srgb};
    ///
    /// let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
    /// let palette = Palette::new(&colors, None).unwrap();
    ///
    /// let ditherer = EinkDitherer::new(palette)
    ///     .algorithm(DitherAlgorithm::FloydSteinberg);
    /// ```
    #[inline]
    pub fn algorithm(mut self, algorithm: DitherAlgorithm) -> Self {
        self.algorithm = algorithm;
        let (error_clamp, noise_scale) = algorithm.defaults();
        self.dither_opts = self
            .dither_opts
            .error_clamp(error_clamp)
            .noise_scale(noise_scale);
        self.error_clamp_explicit = false;
        self
    }

    /// Dither raw sRGB pixels into a [`DitheredImage`].
    ///
    /// Applies the full pipeline:
    /// 1. Preprocess (resize, saturation, contrast)
    /// 2. Dither (error diffusion with selected kernel)
    /// 3. Wrap in [`DitheredImage`]
    ///
    /// The builder is reusable -- `dither()` takes `&self`.
    pub fn dither(&self, pixels: &[Srgb], width: usize, height: usize) -> DitheredImage {
        // 1. Preprocess
        let preprocessor = Preprocessor::new(&self.palette, self.preprocess.clone());
        let result = preprocessor.process(pixels, width, height);

        // 2. Resolve dither options, applying greyscale override if needed
        let dither_opts = if !self.error_clamp_explicit && self.palette.is_greyscale() {
            self.dither_opts.clone().error_clamp(0.6)
        } else {
            self.dither_opts.clone()
        };

        // 3. Dither using unified kernel dispatch
        let photo_palette = self.palette.for_error_diffusion();
        let kernel = self.algorithm.kernel();
        let indices = dither_with_kernel_noise(
            &result.pixels,
            result.width,
            result.height,
            &photo_palette,
            kernel,
            &dither_opts,
        );

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
    fn test_new_defaults() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette);

        assert!(
            (ditherer.preprocess.saturation - 1.0).abs() < f32::EPSILON,
            "Should default to saturation 1.0"
        );
        assert!(
            (ditherer.preprocess.contrast - 1.0).abs() < f32::EPSILON,
            "Should default to contrast 1.0"
        );
    }

    #[test]
    fn test_builder_chaining() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .resize(800, 600)
            .saturation(1.8)
            .contrast(1.2)
            .serpentine(false)
            .error_clamp(0.3);

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
        let ditherer = EinkDitherer::new(palette.clone());
        let pixels = gradient_4x4();

        let result = ditherer.dither(&pixels, 4, 4);

        assert_eq!(result.width(), 4);
        assert_eq!(result.height(), 4);
        assert_eq!(result.indices().len(), 16);

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
        let ditherer = EinkDitherer::new(palette);
        let pixels = gradient_4x4();

        let result1 = ditherer.dither(&pixels, 4, 4);
        let result2 = ditherer.dither(&pixels, 4, 4);

        assert_eq!(result1.indices(), result2.indices());
    }

    #[test]
    fn test_custom_saturation_affects_output() {
        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
        ];
        let palette = Palette::new(&colors, None).unwrap();

        let pixels: Vec<Srgb> = (0..16)
            .map(|i| {
                let r = 128u8.wrapping_add((i * 5) as u8);
                let g = 100u8.wrapping_add((i * 3) as u8);
                let b = 110u8.wrapping_add((i * 7) as u8);
                Srgb::from_u8(r, g, b)
            })
            .collect();

        let low_sat = EinkDitherer::new(palette.clone()).saturation(0.5);
        let high_sat = EinkDitherer::new(palette).saturation(3.0);

        let low_result = low_sat.dither(&pixels, 4, 4);
        let high_result = high_sat.dither(&pixels, 4, 4);

        assert_ne!(
            low_result.indices(),
            high_result.indices(),
            "Different saturation should produce different dither patterns"
        );
    }

    #[test]
    fn test_greyscale_palette_uses_higher_clamp() {
        let grey_palette = Palette::new(
            &[Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)],
            None,
        )
        .unwrap();
        assert!(grey_palette.is_greyscale());

        let color_palette = Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
            ],
            None,
        )
        .unwrap();
        assert!(!color_palette.is_greyscale());
    }

    #[test]
    fn test_algorithm_sets_defaults() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).algorithm(DitherAlgorithm::FloydSteinberg);
        assert!((ditherer.dither_opts.error_clamp - 0.12).abs() < f32::EPSILON);
        assert!((ditherer.dither_opts.noise_scale - 4.0).abs() < f32::EPSILON);
    }
}
