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
    /// These pixels absorb any accumulated error from neighbors, acting
    /// as error sinks that prevent color bleed across hard boundaries.
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

    /// Chromatic error damping threshold (OKLab chroma units).
    ///
    /// Controls how much chromatic (color) error is diffused from each pixel.
    /// The original pixel's OKLab chroma (`sqrt(a² + b²)`) is compared against
    /// this threshold:
    ///
    /// - Pixels with chroma >= threshold: full error diffusion (alpha=1.0)
    /// - Pixels with chroma < threshold: chromatic error scaled by `(chroma/threshold)²`
    ///
    /// Muted pixels (low chroma) diffuse mostly achromatic (mean) error,
    /// preventing chromatic buildup that causes color blowout. Vivid pixels
    /// diffuse full error for accurate color reproduction.
    ///
    /// OKLab chroma reference values:
    /// - Pure grey: 0.00
    /// - Overcast sky: ~0.05
    /// - Skin tones: ~0.03–0.05
    /// - Palette primaries (R/G/B/Y): ~0.25–0.35
    ///
    /// - `0.08` = aggressive damping (B&W except vivid colors)
    /// - `0.12` = moderate damping (recommended for photos)
    /// - `0.20` = gentle damping (more color in muted areas)
    /// - `f32::INFINITY` = no damping (legacy behavior)
    ///
    /// Default: `f32::INFINITY` (no damping — legacy behavior)
    pub chroma_clamp: f32,

    /// Blue noise jitter scale for Floyd-Steinberg Noise algorithm.
    ///
    /// Controls how much the error diffusion weights vary per pixel:
    /// - `0.0` = no jitter (equivalent to plain Floyd-Steinberg)
    /// - `2.0` = mild (±14% weight variation)
    /// - `5.0` = default (±31% weight variation)
    /// - `8.0` = aggressive (±50% weight variation)
    ///
    /// Only affects `FloydSteinbergNoise` algorithm; ignored by others.
    ///
    /// Default: `5.0`
    pub noise_scale: f32,

    /// Whether exact-match pixels absorb accumulated error.
    ///
    /// When `true`, exact-match pixels act as error sinks — accumulated error
    /// from neighbors is discarded, preventing color bleed across boundaries.
    /// When `false`, accumulated error passes through (original behavior),
    /// maintaining smooth gradient continuity but allowing bleed.
    ///
    /// Default: `true` (absorb — prevents bleed across boundaries)
    pub exact_absorb_error: bool,
}

impl Default for DitherOptions {
    fn default() -> Self {
        Self {
            serpentine: true,
            preserve_exact_matches: true,
            error_clamp: 0.5,
            chroma_clamp: f32::INFINITY,
            noise_scale: 5.0,
            exact_absorb_error: true,
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

    /// Set chromatic error clamping threshold.
    ///
    /// Controls how much per-channel error can deviate from the mean
    /// (achromatic) error. Lower values prevent color blowout in photos.
    ///
    /// # Arguments
    /// * `clamp` - Maximum chromatic deviation per channel (0.0 to disable color error, f32::INFINITY for no limit)
    #[inline]
    pub fn chroma_clamp(mut self, clamp: f32) -> Self {
        self.chroma_clamp = clamp;
        self
    }

    /// Set blue noise jitter scale for Floyd-Steinberg Noise algorithm.
    ///
    /// # Arguments
    /// * `scale` - Jitter scale (0.0 = no jitter, 2.0 = default, 4.0 = aggressive)
    #[inline]
    pub fn noise_scale(mut self, scale: f32) -> Self {
        self.noise_scale = scale;
        self
    }

    /// Set whether exact-match pixels absorb accumulated error.
    ///
    /// # Arguments
    /// * `absorb` - When true, exact matches discard error; when false, error passes through
    #[inline]
    pub fn exact_absorb_error(mut self, absorb: bool) -> Self {
        self.exact_absorb_error = absorb;
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
