//! Dithering options and configuration.
//!
//! This module provides the [`DitherOptions`] struct for configuring
//! error diffusion dithering behavior.

/// Configuration options for error diffusion dithering.
///
/// `DitherOptions` controls the behavior of all dithering algorithms,
/// including serpentine scanning, exact match preservation, and error clamping.
///
/// # Defaults
///
/// The default configuration is optimized for e-ink displays:
/// - Serpentine scanning: enabled (eliminates directional artifacts)
/// - Preserve exact matches: enabled (keeps text and UI crisp)
/// - Error clamp: 0.5 (prevents blooming with small palettes)
///
/// # Example
///
/// ```
/// use eink_dither::DitherOptions;
///
/// // Use defaults (recommended for most cases)
/// let options = DitherOptions::new();
///
/// // Or customize with builder pattern
/// let options = DitherOptions::new()
///     .serpentine(false)
///     .error_clamp(0.3);
/// ```
#[derive(Debug, Clone)]
pub struct DitherOptions {
    /// Enable serpentine scanning (alternating row direction).
    ///
    /// When enabled, odd rows are processed right-to-left and the diffusion
    /// kernel is horizontally flipped. This eliminates directional "worm"
    /// artifacts that appear when processing all rows left-to-right.
    ///
    /// Default: `true`
    pub serpentine: bool,

    /// Preserve exact palette matches without dithering.
    ///
    /// When a pixel exactly matches a palette color (byte-for-byte), skip
    /// dithering entirely. This keeps text and solid UI elements crisp.
    /// Error is neither diffused into nor out of these pixels.
    ///
    /// Default: `true`
    pub preserve_exact_matches: bool,

    /// Maximum error magnitude per channel (in linear RGB space).
    ///
    /// Accumulated error is clamped to this range to prevent "blooming"
    /// with small palettes where quantization errors can be large.
    ///
    /// Default: `0.5`
    pub error_clamp: f32,
}

impl Default for DitherOptions {
    fn default() -> Self {
        Self {
            serpentine: true,
            preserve_exact_matches: true,
            error_clamp: 0.5,
        }
    }
}

impl DitherOptions {
    /// Create new dither options with default values.
    ///
    /// This is equivalent to `DitherOptions::default()` but more discoverable.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set serpentine scanning mode.
    ///
    /// # Arguments
    /// * `enabled` - Whether to enable serpentine scanning
    #[inline]
    pub fn serpentine(mut self, enabled: bool) -> Self {
        self.serpentine = enabled;
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

    /// Set error clamping threshold.
    ///
    /// # Arguments
    /// * `clamp` - Maximum error magnitude per channel (typically 0.3-0.5)
    #[inline]
    pub fn error_clamp(mut self, clamp: f32) -> Self {
        self.error_clamp = clamp;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let opts = DitherOptions::default();
        assert!(opts.serpentine, "serpentine should default to true");
        assert!(
            opts.preserve_exact_matches,
            "preserve_exact_matches should default to true"
        );
        assert!(
            (opts.error_clamp - 0.5).abs() < f32::EPSILON,
            "error_clamp should default to 0.5"
        );
    }

    #[test]
    fn test_new_equals_default() {
        let new_opts = DitherOptions::new();
        let default_opts = DitherOptions::default();

        assert_eq!(new_opts.serpentine, default_opts.serpentine);
        assert_eq!(
            new_opts.preserve_exact_matches,
            default_opts.preserve_exact_matches
        );
        assert!((new_opts.error_clamp - default_opts.error_clamp).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_serpentine() {
        let opts = DitherOptions::new().serpentine(false);
        assert!(!opts.serpentine);
        // Other values unchanged
        assert!(opts.preserve_exact_matches);
        assert!((opts.error_clamp - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_preserve_exact_matches() {
        let opts = DitherOptions::new().preserve_exact_matches(false);
        assert!(!opts.preserve_exact_matches);
        // Other values unchanged
        assert!(opts.serpentine);
        assert!((opts.error_clamp - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_error_clamp() {
        let opts = DitherOptions::new().error_clamp(0.3);
        assert!((opts.error_clamp - 0.3).abs() < f32::EPSILON);
        // Other values unchanged
        assert!(opts.serpentine);
        assert!(opts.preserve_exact_matches);
    }

    #[test]
    fn test_builder_chaining() {
        let opts = DitherOptions::new()
            .serpentine(false)
            .preserve_exact_matches(false)
            .error_clamp(0.25);

        assert!(!opts.serpentine);
        assert!(!opts.preserve_exact_matches);
        assert!((opts.error_clamp - 0.25).abs() < f32::EPSILON);
    }
}
