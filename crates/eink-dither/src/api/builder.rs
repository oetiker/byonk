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
    /// Default algorithm and its error_clamp/noise_scale come from
    /// [`DitherAlgorithm::default`]/[`DitherAlgorithm::defaults`], not restated here.
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
        // Derive from the default algorithm rather than restating a constant:
        // this used to hardcode error_clamp(0.08), so `new()` and
        // `.algorithm(Atkinson)` could silently disagree about the defaults.
        let algorithm = DitherAlgorithm::default();
        let (error_clamp, noise_scale) = algorithm.defaults();
        Self {
            palette,
            preprocess: PreprocessOptions::default(),
            dither_opts: DitherOptions::new()
                .error_clamp(error_clamp)
                .noise_scale(noise_scale)
                .hybrid_propagation(algorithm.is_hybrid_propagation()),
            algorithm,
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

    /// Set blue noise jitter scale.
    #[inline]
    pub fn noise_scale(mut self, scale: f32) -> Self {
        self.dither_opts = self.dither_opts.noise_scale(scale);
        self
    }

    /// Set error diffusion strength.
    ///
    /// Scales the diffused error before propagation (0.0 = no diffusion, 1.0 = standard).
    #[inline]
    pub fn strength(mut self, strength: f32) -> Self {
        self.dither_opts = self.dither_opts.strength(strength);
        self
    }

    /// Set the fraction of accumulated error a pinned pixel passes on.
    ///
    /// See [`DitherOptions::pin_carry`]. Has no effect without a pin map.
    #[inline]
    pub fn pin_carry(mut self, value: f32) -> Self {
        self.dither_opts = self.dither_opts.pin_carry(value);
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
            .noise_scale(noise_scale)
            .hybrid_propagation(algorithm.is_hybrid_propagation());
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
        self.dither_with_pinning(pixels, width, height, None)
    }

    /// Dither, holding pixels that already sit exactly on a palette ink.
    ///
    /// `pin_eligible`, when supplied, is one `bool` per input pixel: `true`
    /// where the caller permits pinning. A pixel is pinned when it is eligible
    /// AND its bytes equal a nominal palette entry exactly. Such a pixel renders
    /// as that ink and hands `DitherOptions::pin_carry` of the error diffused
    /// into it on to its neighbours.
    ///
    /// `None` means no pinning at all — identical output to [`Self::dither`]. A
    /// caller wanting frame-wide pinning passes an all-`true` slice.
    ///
    /// The match is resolved on these `Srgb` bytes, BEFORE preprocessing:
    /// saturation or contrast at anything but identity would move a pure ink off
    /// its palette entry and the match would silently never fire. A pinned pixel
    /// is therefore not enhanced — it renders the colour the author wrote, which
    /// is the right answer for the structural content pinning exists for.
    ///
    /// Pinning is refused when a resize is configured: resampling destroys exact
    /// matches and breaks the index correspondence between `pixels` and the
    /// preprocessed frame.
    pub fn dither_with_pinning(
        &self,
        pixels: &[Srgb],
        width: usize,
        height: usize,
        pin_eligible: Option<&[bool]>,
    ) -> DitheredImage {
        let resizing =
            self.preprocess.target_width.is_some() || self.preprocess.target_height.is_some();

        let pin_map: Option<Vec<Option<u8>>> = match pin_eligible {
            Some(mask) if !resizing && mask.len() == pixels.len() => {
                let inks: Vec<[u8; 3]> = (0..self.palette.len())
                    .map(|i| self.palette.official(i).to_bytes())
                    .collect();
                Some(
                    pixels
                        .iter()
                        .zip(mask.iter())
                        .map(|(px, &ok)| {
                            if !ok {
                                return None;
                            }
                            let bytes = px.to_bytes();
                            inks.iter().position(|ink| *ink == bytes).map(|i| i as u8)
                        })
                        .collect(),
                )
            }
            Some(mask) => {
                debug_assert!(
                    resizing || mask.len() == pixels.len(),
                    "pin_eligible mask length ({}) does not match pixel count ({}) — pinning silently disabled",
                    mask.len(),
                    pixels.len()
                );
                None
            }
            None => None,
        };

        // 1. Preprocess
        let preprocessor = Preprocessor::new(self.preprocess.clone());
        let result = preprocessor.process(pixels, width, height);

        // 2. Dither using unified kernel dispatch.
        //
        // There used to be a greyscale override raising error_clamp to 0.6
        // here. It compensated for the old clamp semantics, which bounded the
        // resulting value rather than the error: a grey ramp lives near the
        // channel extremes, so it was starved of headroom exactly where it
        // needed it. The clamp now bounds the error itself and defaults to
        // 1.0 for every palette, so the override would only ever *lower* it.
        let dither_opts = self.dither_opts.clone();

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
            pin_map.as_deref(),
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

    // Shared geometry for the pinning tests below: a 2 px vertical `line`
    // running through a `field` that is saturated and far from every ink in
    // `test_palette()`, so it diffuses hard into the line. Single source of
    // truth for both the frame and the region every test measures, so the
    // scenario can't silently drift apart between call sites.
    const HOSTILE_W: usize = 32;
    const HOSTILE_H: usize = 32;
    const HOSTILE_LINE_COLS: std::ops::Range<usize> = 15..17;

    /// Build the hostile-field frame: `field` everywhere except a 2 px
    /// vertical line of `line` at `HOSTILE_LINE_COLS`.
    fn hostile_line_field(field: Srgb, line: Srgb) -> Vec<Srgb> {
        let mut px = vec![field; HOSTILE_W * HOSTILE_H];
        for y in 0..HOSTILE_H {
            for x in HOSTILE_LINE_COLS {
                px[y * HOSTILE_W + x] = line;
            }
        }
        px
    }

    /// Fraction of the line's pixels (see `hostile_line_field`) that
    /// quantized to `ink` in `img`.
    fn line_ink_share(img: &DitheredImage, ink: u8) -> f64 {
        let mut n = 0usize;
        for y in 0..HOSTILE_H {
            for x in HOSTILE_LINE_COLS {
                if img.indices()[y * HOSTILE_W + x] == ink {
                    n += 1;
                }
            }
        }
        n as f64 / (HOSTILE_H * HOSTILE_LINE_COLS.len()) as f64
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
        let ditherer =
            EinkDitherer::new(palette.clone()).algorithm(DitherAlgorithm::FloydSteinberg);
        assert!((ditherer.dither_opts.error_clamp - 1.0).abs() < f32::EPSILON);
        assert!((ditherer.dither_opts.noise_scale - 8.0).abs() < f32::EPSILON);
        assert!(!ditherer.dither_opts.hybrid_propagation);

        let ditherer = EinkDitherer::new(palette).algorithm(DitherAlgorithm::AtkinsonHybrid);
        assert!((ditherer.dither_opts.error_clamp - 1.0).abs() < f32::EPSILON);
        assert!((ditherer.dither_opts.noise_scale - 8.0).abs() < f32::EPSILON);
        assert!(ditherer.dither_opts.hybrid_propagation);
    }

    #[test]
    fn test_builder_strength() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).strength(0.5);
        assert!((ditherer.dither_opts.strength - 0.5).abs() < f32::EPSILON);
    }

    /// Eligibility gates pinning. A 2 px pure-black line between saturated
    /// fields is pinned where the caller allows it and dithered normally
    /// where it does not.
    ///
    /// The brief's original version of this test used a single 4x1 row with
    /// one vivid pixel ahead of three pure blacks. Measured: even with
    /// pinning fully disabled (`ineligible`), the blacks came back all-black
    /// — the vivid pixel's residual error was too small, and Atkinson's
    /// down-row taps had no second row to land in, so the "ineligible" mask
    /// and the "eligible" mask produced identical output and the test passed
    /// against a mutant that ignores the mask entirely. Replaced with the
    /// geometry the real defect has: a 2 px pinned line wide enough, and long
    /// enough, for diffused error from a genuinely hostile saturated field to
    /// erode it when pinning is off.
    #[test]
    fn eligibility_decides_where_pinning_applies() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        let field = Srgb::from_u8(192, 96, 32);
        let black = Srgb::from_u8(0, 0, 0);
        let px = hostile_line_field(field, black);
        let (w, h) = (HOSTILE_W, HOSTILE_H);

        let eligible = vec![true; w * h];
        let ineligible = vec![false; w * h];

        let pinned = ditherer.dither_with_pinning(&px, w, h, Some(&eligible));
        let unpinned = ditherer.dither_with_pinning(&px, w, h, Some(&ineligible));

        let unpinned_share = line_ink_share(&unpinned, 0);
        let pinned_share = line_ink_share(&pinned, 0);

        // The scenario must actually be hostile, or the pinned result below
        // proves nothing.
        assert!(
            unpinned_share < 0.99,
            "the ineligible line came back {:.1}% black — this scenario is \
             not hostile, so the pinned result below would prove nothing",
            unpinned_share * 100.0
        );
        assert_eq!(
            pinned_share,
            1.0,
            "pinned line came back only {:.1}% black (ineligible: {:.1}%)",
            pinned_share * 100.0,
            unpinned_share * 100.0
        );
    }

    /// The match is against the NOMINAL palette entry, not the measured ink.
    /// test_palette()'s red is official (255,0,0) / actual (200,50,50).
    ///
    /// The brief's original version dithered a flat 2 px row of official red
    /// with no neighbours and asserted the output was ink 2. Measured: it is
    /// ink 2 even completely unpinned (`dither_with_pinning(..., None)`),
    /// because red is still the nearest palette entry to (255,0,0) by plain
    /// distance — matching against `actual(i)` instead of `official(i)`
    /// would make the exact-match check fail to fire for every pixel in this
    /// scene, and the test could not tell, since ordinary nearest-match
    /// dithering lands on the same ink anyway. Replaced with a 2 px line of
    /// official red between a hostile green field — far enough from every
    /// ink in `test_palette()` to diffuse hard. Since official red's bytes
    /// never equal its *actual* bytes, a match against `actual(i)` never
    /// pins this line at all, so it is exactly as eroded as the unpinned
    /// baseline; matching against `official(i)` holds it exactly.
    #[test]
    fn the_exact_match_is_against_the_nominal_entry() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        let field = Srgb::from_u8(0, 255, 0);
        let red = Srgb::from_u8(255, 0, 0);
        let px = hostile_line_field(field, red);
        let (w, h) = (HOSTILE_W, HOSTILE_H);
        let eligible = vec![true; w * h];

        let pinned = ditherer.dither_with_pinning(&px, w, h, Some(&eligible));
        let unpinned = ditherer.dither_with_pinning(&px, w, h, None);

        let unpinned_share = line_ink_share(&unpinned, 2);
        let pinned_share = line_ink_share(&pinned, 2);

        // The scenario must actually be hostile, or the pinned result below
        // proves nothing.
        assert!(
            unpinned_share < 0.99,
            "the unpinned red line came back {:.1}% red — this scenario is \
             not hostile, so the pinned result below would prove nothing",
            unpinned_share * 100.0
        );
        assert_eq!(
            pinned_share,
            1.0,
            "an author-written nominal red line came back only {:.1}% \
             recognised as ink 2 (unpinned: {:.1}%)",
            pinned_share * 100.0,
            unpinned_share * 100.0
        );
    }

    /// A pixel that is not exactly an ink is never pinned, however eligible.
    ///
    /// The brief's original version used a flat, isolated `(1,0,0)` row with
    /// no neighbours. Measured: with a correct exact match this row never
    /// pins (as intended), so `with == without` trivially — but a "compare
    /// with a tolerance instead of `==`" mutant makes the same assertion
    /// pass too, because normal nearest-match dithering *also* quantizes an
    /// isolated near-black pixel to black; there was no error in flight for
    /// a wrongly-tolerant pin to change. Replaced with the same hostile 2 px
    /// line geometry used above: near-black surrounded by a saturated field
    /// that erodes an unpinned line. A tolerant match would pin the
    /// near-miss line to 100% black — visibly different from the erosion an
    /// exact match leaves it at — so this version actually exercises the
    /// exactness of the comparison rather than just its wiring.
    ///
    /// The `without` erosion is asserted directly (not just documented):
    /// a mutant that disables the pin path entirely, or a future change
    /// that stops this scene being hostile, must fail loudly here rather
    /// than pass a now-tautological `with == without`.
    #[test]
    fn a_near_miss_is_not_pinned() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        let field = Srgb::from_u8(192, 96, 32);
        // One byte off black in one channel.
        let near_black = Srgb::from_u8(1, 0, 0);
        let px = hostile_line_field(field, near_black);
        let (w, h) = (HOSTILE_W, HOSTILE_H);
        let eligible = vec![true; w * h];

        let with = ditherer.dither_with_pinning(&px, w, h, Some(&eligible));
        let without = ditherer.dither_with_pinning(&px, w, h, None);

        let without_share = line_ink_share(&without, 0);
        assert!(
            without_share < 0.99,
            "the unpinned near-miss line came back {:.1}% black — this \
             scenario is not hostile, so the equality below would prove \
             nothing",
            without_share * 100.0
        );
        assert_eq!(
            with.indices(),
            without.indices(),
            "a near-miss pixel was pinned; the match is not exact"
        );
    }

    /// dither() is dither_with_pinning(None) and neither pins.
    ///
    /// The brief's original version used a smooth (i*4, 128, 255-i*4)
    /// gradient, which never lands exactly on any test_palette() ink — so a
    /// mutant that has `dither()` build an all-true mask internally has
    /// nothing to pin either, and the two calls agree by accident. Reuse the
    /// hostile black-line-in-a-saturated-field geometry so an
    /// internally-fabricated mask would visibly rescue the line and this
    /// test would catch it. The erosion of `b` (the definitely-unpinned
    /// baseline) is asserted directly, not just documented, so a mutant
    /// that removes the pin path entirely can't collapse this into a
    /// tautology without being caught.
    #[test]
    fn plain_dither_is_unchanged_by_this_feature() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);
        let field = Srgb::from_u8(192, 96, 32);
        let black = Srgb::from_u8(0, 0, 0);
        let px = hostile_line_field(field, black);
        let (w, h) = (HOSTILE_W, HOSTILE_H);

        let a = ditherer.dither(&px, w, h);
        let b = ditherer.dither_with_pinning(&px, w, h, None);

        let b_share = line_ink_share(&b, 0);
        assert!(
            b_share < 0.99,
            "the unpinned baseline came back {:.1}% black — this scenario \
             is not hostile, so the equality below would prove nothing",
            b_share * 100.0
        );
        assert_eq!(a.indices(), b.indices());
    }

    /// Pinning is refused whenever a resize is *configured*, not only when
    /// it would actually misalign indices.
    ///
    /// The brief's original version used `.resize(2, 2)` on a 4x4 all-black
    /// input — a real dimension change. In this vendored build the `image`
    /// crate is removed and `resize_lanczos` unconditionally panics when
    /// target dimensions differ from the input's (see
    /// `preprocess::resize::resize_lanczos`), so that call never reached the
    /// pinning guard at all: it panicked in preprocessing before comparing
    /// `with`/`without`, for a reason that has nothing to do with this
    /// guard. It was also all-black, so even had it run, an "ignore the
    /// `!resizing` guard" mutant could not have been told apart from the
    /// guard firing correctly — an all-black frame dithers to all-index-0
    /// either way.
    ///
    /// Fixed the "never reaches the guard" and "content can't distinguish
    /// the mutant" problems: `.resize(w, h)` here equals the input's own
    /// dimensions, so `resize_lanczos` takes its no-op branch and does not
    /// panic, while `target_width`/`target_height` are still `Some` and the
    /// guard is still exercised purely from that configuration. The content
    /// is the same hostile 2 px black line used in
    /// `eligibility_decides_where_pinning_applies`, and its erosion is
    /// asserted directly below rather than only documented.
    ///
    /// What this test does NOT cover, and cannot in this vendored build:
    /// the brief's other stated reason for the guard is that resampling
    /// breaks *index correspondence* between the caller's pixels and the
    /// preprocessed frame — a real dimension change reindexes the frame out
    /// from under a pin map built against the original size. Because
    /// `resize_lanczos` has no resampling backend here and panics on any
    /// actual dimension change, that misalignment scenario cannot be
    /// exercised at all in this build. A mutant that drops the `!resizing`
    /// guard is still caught below (refusal-by-configuration is verified),
    /// but a reader should not take this green test as evidence that
    /// index misalignment across a real resize is also covered — it isn't.
    #[test]
    fn pinning_is_refused_when_resizing() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false)
            .resize(HOSTILE_W as u32, HOSTILE_H as u32);
        let field = Srgb::from_u8(192, 96, 32);
        let black = Srgb::from_u8(0, 0, 0);
        let px = hostile_line_field(field, black);
        let (w, h) = (HOSTILE_W, HOSTILE_H);
        let eligible = vec![true; w * h];

        let with = ditherer.dither_with_pinning(&px, w, h, Some(&eligible));
        let without = ditherer.dither_with_pinning(&px, w, h, None);

        let without_share = line_ink_share(&without, 0);
        assert!(
            without_share < 0.99,
            "the unpinned line came back {:.1}% black — this scenario is \
             not hostile, so the equality below would prove nothing",
            without_share * 100.0
        );
        assert_eq!(
            with.indices(),
            without.indices(),
            "pinning was applied across a configured resize"
        );
    }
}
