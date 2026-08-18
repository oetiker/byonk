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
    /// All error diffusion algorithms support this jitter.
    ///
    /// Default: `5.0`
    pub noise_scale: f32,

    /// Error diffusion strength multiplier.
    ///
    /// Scales the diffused error uniformly before propagation to neighbors:
    /// - `0.0` = no error diffusion (pure nearest-color posterization)
    /// - `0.5` = subtle dithering, less texture
    /// - `1.0` = standard behavior (backward compatible default)
    /// - `>1.0` = exaggerated texture
    ///
    /// Default: `1.0`
    pub strength: f32,

    /// Use hybrid achromatic/chromatic error propagation.
    ///
    /// When enabled, the error is split into achromatic (mean RGB) and
    /// chromatic (deviation from mean) components. The achromatic component
    /// is propagated at 100% (weight/weight_sum) while the chromatic component
    /// is propagated at the kernel's native rate (weight/divisor, typically 75%
    /// for Atkinson). This prevents color drift on chromatic palettes while
    /// preserving the algorithm's distinctive character.
    ///
    /// Default: `false`
    pub hybrid_propagation: bool,

    /// Fraction of accumulated error a pinned pixel passes on to its neighbours.
    ///
    /// A pinned pixel is one that already sits exactly on a palette ink in a
    /// region the caller marked as eligible. It outputs that ink and ignores the
    /// error diffused into it, so its own quantisation error is zero. This value
    /// decides what happens to the error that arrived:
    ///
    /// - `1.0` — pass it on unchanged. Total error is conserved, so no seam, but
    ///   error can travel the full width of a large pinned region and dump as a
    ///   fringe at its far edge.
    /// - `0.0` — absorb it. Crisp, but a coincidental match mid-gradient drops
    ///   its neighbours' error and leaves a seam across a smooth ramp.
    /// - between — the error decays geometrically with depth into the region.
    ///   At depth `n` the surviving fraction is `pin_carry^n`. A 2 px grid line
    ///   or a text stroke is crossed in one or two steps and passes error through
    ///   nearly intact; a wide flat area absorbs it within a few pixels of its
    ///   edge.
    ///
    /// The count of pinned pixels the error has crossed IS its distance into the
    /// region, measured along the path the error actually travelled. That is why
    /// no distance transform is needed.
    ///
    /// Has no effect unless the caller supplies a pin map.
    ///
    /// Default: `0.9`
    pub pin_carry: f32,
}

impl Default for DitherOptions {
    fn default() -> Self {
        Self {
            serpentine: true,
            error_clamp: 1.0,
            chroma_clamp: f32::INFINITY,
            noise_scale: 5.0,
            strength: 1.0,
            hybrid_propagation: false,
            pin_carry: 0.9,
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

    /// Set error diffusion strength.
    ///
    /// # Arguments
    /// * `strength` - Multiplier for diffused error (0.0 = no diffusion, 1.0 = standard)
    #[inline]
    pub fn strength(mut self, strength: f32) -> Self {
        self.strength = strength;
        self
    }

    /// Enable or disable hybrid achromatic/chromatic error propagation.
    ///
    /// # Arguments
    /// * `enabled` - Whether to use hybrid propagation (true for AtkinsonHybrid)
    #[inline]
    pub fn hybrid_propagation(mut self, enabled: bool) -> Self {
        self.hybrid_propagation = enabled;
        self
    }

    /// Set the fraction of accumulated error a pinned pixel passes on.
    ///
    /// Clamped to `[0.0, 1.0]`. Values outside that range have no physical
    /// meaning: below zero would invert the error, above one would amplify it
    /// with depth.
    #[inline]
    pub fn pin_carry(mut self, value: f32) -> Self {
        self.pin_carry = value.clamp(0.0, 1.0);
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
            (opts.error_clamp - 1.0).abs() < f32::EPSILON,
            "error_clamp should default to 1.0"
        );
    }

    #[test]
    fn test_new_equals_default() {
        let new_opts = DitherOptions::new();
        let default_opts = DitherOptions::default();

        assert_eq!(new_opts.serpentine, default_opts.serpentine);
        assert!((new_opts.error_clamp - default_opts.error_clamp).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_serpentine() {
        let opts = DitherOptions::new().serpentine(false);
        assert!(!opts.serpentine);
        // Other values unchanged
        assert!((opts.error_clamp - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_error_clamp() {
        let opts = DitherOptions::new().error_clamp(0.3);
        assert!((opts.error_clamp - 0.3).abs() < f32::EPSILON);
        // Other values unchanged
        assert!(opts.serpentine);
    }

    #[test]
    fn test_builder_chaining() {
        let opts = DitherOptions::new().serpentine(false).error_clamp(0.25);

        assert!(!opts.serpentine);
        assert!((opts.error_clamp - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn test_default_strength() {
        let opts = DitherOptions::default();
        assert!(
            (opts.strength - 1.0).abs() < f32::EPSILON,
            "strength should default to 1.0"
        );
    }

    #[test]
    fn test_builder_strength() {
        let opts = DitherOptions::new().strength(0.5);
        assert!((opts.strength - 0.5).abs() < f32::EPSILON);
        // Other values unchanged
        assert!(opts.serpentine);
        assert!((opts.error_clamp - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pin_carry_defaults_to_the_shipping_value() {
        assert!(
            (DitherOptions::default().pin_carry - 0.9).abs() < f32::EPSILON,
            "pin_carry default changed; the sweep in the plan's Task 5 chose 0.9 \
             provisionally and any change needs re-measuring"
        );
    }

    #[test]
    fn pin_carry_is_clamped_to_a_meaningful_range() {
        // Above 1.0 the carry would amplify error with depth into a pinned
        // region rather than decaying it; below 0.0 it would invert the sign.
        assert!((DitherOptions::new().pin_carry(2.0).pin_carry - 1.0).abs() < f32::EPSILON);
        assert!((DitherOptions::new().pin_carry(-1.0).pin_carry - 0.0).abs() < f32::EPSILON);
    }
}
