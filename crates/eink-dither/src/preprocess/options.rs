//! Preprocessing options and configuration.
//!
//! This module provides the [`PreprocessOptions`] struct for configuring
//! image preprocessing before dithering.

/// Configuration options for image preprocessing.
///
/// `PreprocessOptions` controls the preprocessing pipeline applied to images
/// before dithering, including resize, saturation boost, and contrast adjustment.
///
/// # Defaults
///
/// The default configuration is optimized for photo rendering:
/// - Saturation: 1.5 (50% boost for e-ink's muted colors)
/// - Contrast: 1.1 (slight boost for limited dynamic range)
/// - Preserve exact matches: enabled (keeps palette colors untouched)
/// - Resize: disabled (preserve original dimensions)
///
/// # Presets
///
/// # Example
///
/// ```
/// use eink_dither::PreprocessOptions;
///
/// // Default options (saturation 1.0, contrast 1.0)
/// let options = PreprocessOptions::new();
///
/// // Customize with builder pattern
/// let options = PreprocessOptions::new()
///     .resize(800, 600)
///     .saturation(1.3)
///     .contrast(1.2);
/// ```
#[derive(Debug, Clone)]
pub struct PreprocessOptions {
    /// Target width for resize (None = preserve original).
    pub target_width: Option<u32>,

    /// Target height for resize (None = preserve original).
    pub target_height: Option<u32>,

    /// Saturation multiplier in Oklch space.
    ///
    /// - 1.0 = no change
    /// - 1.5 = 50% boost (default for photos)
    /// - 0.5 = reduce saturation by half
    pub saturation: f32,

    /// Contrast multiplier in linear RGB space.
    ///
    /// - 1.0 = no change
    /// - 1.1 = 10% boost (default for photos)
    /// - 1.5 = high contrast
    pub contrast: f32,

    /// Whether to preserve exact palette matches.
    ///
    /// When enabled, pixels that exactly match a palette color are not
    /// preprocessed, preserving their original values. This keeps text
    /// and solid UI elements crisp.
    pub preserve_exact_matches: bool,
}

impl Default for PreprocessOptions {
    fn default() -> Self {
        Self {
            target_width: None,
            target_height: None,
            saturation: 1.0,
            contrast: 1.0,
            preserve_exact_matches: true,
        }
    }
}

impl PreprocessOptions {
    /// Create new preprocessing options with default values.
    ///
    /// This is equivalent to `PreprocessOptions::default()` but more discoverable.
    /// The defaults are optimized for photo rendering.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set target dimensions for resize.
    ///
    /// When both width and height are specified, the image is resized
    /// to those exact dimensions using Lanczos3 filtering.
    ///
    /// # Arguments
    /// * `width` - Target width in pixels
    /// * `height` - Target height in pixels
    #[inline]
    pub fn resize(mut self, width: u32, height: u32) -> Self {
        self.target_width = Some(width);
        self.target_height = Some(height);
        self
    }

    /// Set saturation multiplier.
    ///
    /// Saturation is adjusted in Oklch color space for perceptually
    /// correct results without hue shifts.
    ///
    /// # Arguments
    /// * `factor` - Saturation multiplier (1.0 = no change)
    #[inline]
    pub fn saturation(mut self, factor: f32) -> Self {
        self.saturation = factor;
        self
    }

    /// Set contrast multiplier.
    ///
    /// Contrast is adjusted in linear RGB space around the midpoint (0.5).
    ///
    /// # Arguments
    /// * `factor` - Contrast multiplier (1.0 = no change)
    #[inline]
    pub fn contrast(mut self, factor: f32) -> Self {
        self.contrast = factor;
        self
    }

    /// Set exact match preservation mode.
    ///
    /// # Arguments
    /// * `enabled` - Whether to preserve pixels that exactly match palette colors
    #[inline]
    pub fn preserve_exact_matches(mut self, enabled: bool) -> Self {
        self.preserve_exact_matches = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let opts = PreprocessOptions::default();
        assert!(
            opts.target_width.is_none(),
            "target_width should default to None"
        );
        assert!(
            opts.target_height.is_none(),
            "target_height should default to None"
        );
        assert!(
            (opts.saturation - 1.0).abs() < f32::EPSILON,
            "saturation should default to 1.0"
        );
        assert!(
            (opts.contrast - 1.0).abs() < f32::EPSILON,
            "contrast should default to 1.1"
        );
        assert!(
            opts.preserve_exact_matches,
            "preserve_exact_matches should default to true"
        );
    }

    #[test]
    fn test_new_equals_default() {
        let new_opts = PreprocessOptions::new();
        let default_opts = PreprocessOptions::default();

        assert_eq!(new_opts.target_width, default_opts.target_width);
        assert_eq!(new_opts.target_height, default_opts.target_height);
        assert!((new_opts.saturation - default_opts.saturation).abs() < f32::EPSILON);
        assert!((new_opts.contrast - default_opts.contrast).abs() < f32::EPSILON);
        assert_eq!(
            new_opts.preserve_exact_matches,
            default_opts.preserve_exact_matches
        );
    }

    #[test]
    fn test_builder_resize() {
        let opts = PreprocessOptions::new().resize(800, 600);
        assert_eq!(opts.target_width, Some(800));
        assert_eq!(opts.target_height, Some(600));
        // Other values unchanged
        assert!((opts.saturation - 1.0).abs() < f32::EPSILON);
        assert!((opts.contrast - 1.0).abs() < f32::EPSILON);
        assert!(opts.preserve_exact_matches);
    }

    #[test]
    fn test_builder_saturation() {
        let opts = PreprocessOptions::new().saturation(2.0);
        assert!((opts.saturation - 2.0).abs() < f32::EPSILON);
        // Other values unchanged
        assert!(opts.target_width.is_none());
        assert!(opts.target_height.is_none());
        assert!((opts.contrast - 1.0).abs() < f32::EPSILON);
        assert!(opts.preserve_exact_matches);
    }

    #[test]
    fn test_builder_contrast() {
        let opts = PreprocessOptions::new().contrast(1.3);
        assert!((opts.contrast - 1.3).abs() < f32::EPSILON);
        // Other values unchanged
        assert!(opts.target_width.is_none());
        assert!(opts.target_height.is_none());
        assert!((opts.saturation - 1.0).abs() < f32::EPSILON);
        assert!(opts.preserve_exact_matches);
    }

    #[test]
    fn test_builder_preserve_exact_matches() {
        let opts = PreprocessOptions::new().preserve_exact_matches(false);
        assert!(!opts.preserve_exact_matches);
        // Other values unchanged
        assert!(opts.target_width.is_none());
        assert!(opts.target_height.is_none());
        assert!((opts.saturation - 1.0).abs() < f32::EPSILON);
        assert!((opts.contrast - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_chaining() {
        let opts = PreprocessOptions::new()
            .resize(1024, 768)
            .saturation(1.8)
            .contrast(1.2)
            .preserve_exact_matches(false);

        assert_eq!(opts.target_width, Some(1024));
        assert_eq!(opts.target_height, Some(768));
        assert!((opts.saturation - 1.8).abs() < f32::EPSILON);
        assert!((opts.contrast - 1.2).abs() < f32::EPSILON);
        assert!(!opts.preserve_exact_matches);
    }

    #[test]
    fn test_builder_with_resize_and_overrides() {
        let opts = PreprocessOptions::new().resize(800, 600).saturation(1.3);

        assert_eq!(opts.target_width, Some(800));
        assert_eq!(opts.target_height, Some(600));
        assert!((opts.saturation - 1.3).abs() < f32::EPSILON);
        assert!((opts.contrast - 1.0).abs() < f32::EPSILON);
    }
}
