//! EinkDitherer builder -- the primary ergonomic entry point for the crate.
//!
//! [`EinkDitherer`] wraps the dithering pipeline with fluent configuration
//! and optional preprocessing overrides.

use crate::color::Srgb;
use crate::dither::{dither_with_kernel_noise, DitherAlgorithm, DitherOptions, RegionMap};
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
        self.dither_with_regions(pixels, width, height, None)
    }

    /// Dither, honouring per-pixel continuous-tone regions.
    ///
    /// `continuous`, when supplied, is one `bool` per input pixel: `true` where
    /// the content is continuous-tone. That bit selects three behaviours at
    /// once (owner rulings 22 and 23):
    ///
    /// - **Colour model.** Unmarked content is matched against the NOMINAL
    ///   palette entries and is taken to BE them; marked content is matched
    ///   against the measured inks.
    /// - **Pinning.** Unmarked pixels whose bytes equal a nominal entry exactly
    ///   are pinned; marked pixels never are. A pinned pixel renders as that ink
    ///   and hands `DitherOptions::pin_carry` of the error diffused into it on
    ///   to its neighbours.
    /// - **Error diffusion.** No error crosses between marked and unmarked
    ///   pixels, in either direction, exactly as none crosses the frame edge.
    ///
    /// `None` means none of the three: measured model everywhere, no pinning,
    /// no boundary stops — identical output to [`Self::dither`].
    ///
    /// NOTE the polarity: this is the tone mask as rasterized, not the
    /// pin-eligibility mask. An all-`true` slice means "everything is
    /// continuous-tone", which disables pinning.
    ///
    /// The pin match is resolved on these `Srgb` bytes, BEFORE preprocessing:
    /// saturation or contrast at anything but identity would move a pure ink off
    /// its palette entry and the match would silently never fire. A pinned pixel
    /// is therefore not enhanced — it renders the colour the author wrote, which
    /// is the right answer for the structural content pinning exists for.
    ///
    /// The whole region map is refused when a resize is configured: resampling
    /// destroys exact matches and breaks the index correspondence between
    /// `pixels` and the preprocessed frame. A resizing call therefore behaves
    /// exactly like `None`, colour model included.
    ///
    /// # Panics
    ///
    /// Debug builds panic if `continuous.len() != pixels.len()`.
    ///
    /// **In release builds a wrong-length mask is silently ignored** and the
    /// call degrades to `None`. Under ruling 22 that is not a small fallback:
    /// it reverts the COLOUR MODEL for the entire frame, not just the pinning,
    /// so a caller that gets the length wrong gets a plausible-looking image
    /// rendered against the wrong palette with nothing in the output to say so.
    /// Callers must validate the length themselves; byonk's own call site in
    /// `rendering/svg_to_png.rs` hard-errors before reaching here.
    pub fn dither_with_regions(
        &self,
        pixels: &[Srgb],
        width: usize,
        height: usize,
        continuous: Option<&[bool]>,
    ) -> DitheredImage {
        let resizing =
            self.preprocess.target_width.is_some() || self.preprocess.target_height.is_some();

        // The tone mask is borrowed as-is; only the derived pin map is
        // allocated. (Deviation from the brief, which cloned the mask into a
        // `Vec<bool>` for no reason the borrow checker requires: `continuous`
        // outlives every use of `regions` below.)
        let maps: Option<(&[bool], Vec<Option<u8>>)> = match continuous {
            Some(mask) if !resizing && mask.len() == pixels.len() => {
                let inks: Vec<[u8; 3]> = (0..self.palette.len())
                    .map(|i| self.palette.official(i).to_bytes())
                    .collect();
                let pinned: Vec<Option<u8>> = pixels
                    .iter()
                    .zip(mask.iter())
                    .map(|(px, &is_continuous)| {
                        if is_continuous {
                            return None;
                        }
                        let bytes = px.to_bytes();
                        inks.iter().position(|ink| *ink == bytes).map(|i| i as u8)
                    })
                    .collect();
                Some((mask, pinned))
            }
            Some(mask) => {
                debug_assert!(
                    resizing || mask.len() == pixels.len(),
                    "continuous mask length ({}) does not match pixel count ({}) — regions silently disabled",
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

        // 2. Resolve dither options.
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
        let regions = maps
            .as_ref()
            .map(|(continuous, pinned)| RegionMap { continuous, pinned });
        let indices = dither_with_kernel_noise(
            &result.pixels,
            result.width,
            result.height,
            &photo_palette,
            kernel,
            &dither_opts,
            regions.as_ref(),
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

    /// One row of `img`, left to right.
    fn row(img: &DitheredImage, y: usize) -> &[u8] {
        &img.indices()[y * HOSTILE_W..(y + 1) * HOSTILE_W]
    }

    /// Fraction of the line's pixels (see `hostile_line_field`) in rows
    /// `rows` that quantized to `ink` in `img`.
    fn line_ink_share_in_rows(img: &DitheredImage, rows: std::ops::Range<usize>, ink: u8) -> f64 {
        let total = rows.len() * HOSTILE_LINE_COLS.len();
        let mut n = 0usize;
        for y in rows {
            for x in HOSTILE_LINE_COLS {
                if img.indices()[y * HOSTILE_W + x] == ink {
                    n += 1;
                }
            }
        }
        n as f64 / total as f64
    }

    /// Fraction of the line's pixels (see `hostile_line_field`) that
    /// quantized to `ink` in `img`.
    fn line_ink_share(img: &DitheredImage, ink: u8) -> f64 {
        line_ink_share_in_rows(img, 0..HOSTILE_H, ink)
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
    ///
    /// Task 5 changed the hostile field from (192,96,32) to (255,128,0), and
    /// added the `nominal_no_pin` control. Both because the two arms now differ
    /// in COLOUR MODEL as well as in pinning (ruling 22: unmarked implies
    /// nominal). Measured at (192,96,32) with the nominal model: a line of
    /// (1,0,0), which cannot pin, still comes back 100% black — the nominal
    /// model alone holds it, so `pinned_share == 1.0` proved nothing about
    /// pinning and the test passed against an implementation that never pins.
    /// At (255,128,0) the same unpinnable line measures 85.9%, so the control
    /// below is a real discriminator: 100% for exact black there IS the pin.
    #[test]
    fn eligibility_decides_where_pinning_applies() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        let field = Srgb::from_u8(255, 128, 0);
        let black = Srgb::from_u8(0, 0, 0);
        let px = hostile_line_field(field, black);
        let (w, h) = (HOSTILE_W, HOSTILE_H);

        // Polarity: `false` = not continuous-tone = pin-eligible.
        let eligible = vec![false; w * h];
        let ineligible = vec![true; w * h];

        let pinned = ditherer.dither_with_regions(&px, w, h, Some(&eligible));
        let unpinned = ditherer.dither_with_regions(&px, w, h, Some(&ineligible));
        // Same mask, same colour model as `pinned`; one byte off black, so it
        // cannot pin. Isolates the pin from the model flip.
        let nominal_no_pin = ditherer.dither_with_regions(
            &hostile_line_field(field, Srgb::from_u8(1, 0, 0)),
            w,
            h,
            Some(&eligible),
        );

        let unpinned_share = line_ink_share(&unpinned, 0);
        let pinned_share = line_ink_share(&pinned, 0);
        let control_share = line_ink_share(&nominal_no_pin, 0);

        // The scenario must actually be hostile, or the pinned result below
        // proves nothing.
        assert!(
            unpinned_share < 0.99,
            "the ineligible line came back {:.1}% black — this scenario is \
             not hostile, so the pinned result below would prove nothing",
            unpinned_share * 100.0
        );
        // ...and hostile under the *same* colour model the pinned arm runs in,
        // or `pinned_share == 1.0` would be the nominal model's doing.
        assert!(
            control_share < 0.99,
            "an unpinnable line came back {:.1}% black under the very mask \
             the pinned arm uses — the nominal colour model alone is holding \
             this line, so the pinned result below is not attributable to \
             pinning",
            control_share * 100.0
        );
        assert_eq!(
            pinned_share,
            1.0,
            "pinned line came back only {:.1}% black (ineligible: {:.1}%, \
             same-model unpinnable control: {:.1}%)",
            pinned_share * 100.0,
            unpinned_share * 100.0,
            control_share * 100.0
        );
    }

    /// The polarity guard. `continuous` is the tone mask, NOT its inverse: an
    /// all-true mask means "all continuous-tone", which must disable pinning.
    /// Inverting this is silent and produces a plausible image either way.
    ///
    /// Field changed from the brief's (0xC0,0x60,0x20) to (255,128,0). Measured
    /// at the brief's field: an unpinnable (1,0,0) line comes back 100% black
    /// under the nominal model, so `unmarked_share > 0.99` was satisfied by the
    /// colour model rather than by the pin, and both assertions passed against
    /// an implementation that pins nothing at all. At (255,128,0) that same
    /// unpinnable line measures 85.9%, so `unmarked_share > 0.99` can only be
    /// met by the pin actually firing. See
    /// `eligibility_decides_where_pinning_applies` for the explicit control.
    #[test]
    fn an_all_continuous_mask_disables_pinning() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);
        let px = hostile_line_field(Srgb::from_u8(255, 128, 0), Srgb::from_u8(0, 0, 0));

        let all_continuous = vec![true; px.len()];
        let none_continuous = vec![false; px.len()];

        let marked = ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, Some(&all_continuous));
        let unmarked =
            ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, Some(&none_continuous));

        let marked_share = line_ink_share(&marked, 0);
        let unmarked_share = line_ink_share(&unmarked, 0);

        assert!(
            unmarked_share > 0.99,
            "an unmarked line was not pinned ({:.1}%) — polarity may be inverted",
            unmarked_share * 100.0
        );
        assert!(
            marked_share < 0.99,
            "an all-continuous mask still pinned the line ({:.1}%) — the mask is \
             being read as pin_eligible rather than as the tone mask",
            marked_share * 100.0
        );
    }

    /// The mask is read per pixel, not once for the frame.
    ///
    /// Added in Task 5, not in the brief. Every other test in this module
    /// passes a UNIFORM mask, so all of them — the polarity guard included —
    /// are satisfied by an implementation that reads the mask once and applies
    /// it frame-wide. Measured: a mutant handing `RegionMap.continuous` a
    /// uniform all-false slice, and a mutant zipping the pin map against
    /// `mask[0]` instead of the mask, both survived the whole module (15/15
    /// green) before this test existed. The first of those two mutants is
    /// exactly the Task-4 placeholder this task removes, so nothing would have
    /// caught its return; and the per-pixel claims in `dither_with_regions`'
    /// own doc comment were, until this test, unverified at this level.
    ///
    /// The split is HORIZONTAL, not vertical. Measured with a vertical split:
    /// containment shields the pinned column outright — no error can cross the
    /// boundary into it, so it comes back solid black whether or not it was
    /// pinned, and the pin assertion could not be attributed to pinning. With a
    /// horizontal split, the pinned pixels in the bottom half still receive
    /// error from the hostile field of their own region.
    ///
    /// Two claims, one per behaviour the mask drives here:
    ///
    /// - Colour model: with the top half marked continuous, a row deep in the
    ///   top half must dither exactly as it does under an all-marked frame, and
    ///   NOT as under an all-unmarked one. The kernel reaches at most two rows
    ///   down and `serpentine(false)` keeps the scan forward, so row 2 depends
    ///   only on rows 0..=2 — all marked — and the equality is exact.
    /// - Pinning: the same call gives the line's top half a marked bit and its
    ///   bottom half an unmarked one. The bottom half is pinned solid; the top
    ///   half is not.
    ///
    /// Note what the model claim can and cannot attribute: a mutant that
    /// flattens the model for EVERY call also flattens the two reference runs,
    /// so it trips the non-degeneracy assertion rather than the equality. That
    /// is still a catch, and the reference runs are the only baseline available
    /// in-process, but the failure message will name degeneracy, not the model.
    ///
    /// The third behaviour, error containment at the boundary, is Task 4's and
    /// is tested there against the dither loop directly; this test does not
    /// re-assert it.
    #[test]
    fn the_mask_is_applied_per_pixel_not_frame_wide() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        let field = Srgb::from_u8(255, 128, 0);
        let px = hostile_line_field(field, Srgb::from_u8(0, 0, 0));
        let (w, h) = (HOSTILE_W, HOSTILE_H);
        let split = HOSTILE_H / 2;

        // Top half continuous-tone, bottom half not.
        let mixed: Vec<bool> = (0..w * h).map(|i| i / w < split).collect();
        let all_marked = vec![true; w * h];
        let all_unmarked = vec![false; w * h];

        let mixed_img = ditherer.dither_with_regions(&px, w, h, Some(&mixed));
        let marked_img = ditherer.dither_with_regions(&px, w, h, Some(&all_marked));
        let unmarked_img = ditherer.dither_with_regions(&px, w, h, Some(&all_unmarked));

        // Non-degeneracy: the two colour models must actually disagree about
        // this row, or the equality below is vacuous.
        assert_ne!(
            row(&marked_img, 2),
            row(&unmarked_img, 2),
            "the measured and nominal models agree on row 2 of this field, so \
             the model comparison below proves nothing"
        );
        assert_eq!(
            row(&mixed_img, 2),
            row(&marked_img, 2),
            "a row inside the marked half did not dither as marked content — \
             the mask is not being read per pixel"
        );

        let unmarked_half = line_ink_share_in_rows(&mixed_img, split..h, 0);
        let marked_half = line_ink_share_in_rows(&mixed_img, 0..split, 0);
        assert_eq!(
            unmarked_half,
            1.0,
            "the unmarked half of the line came back only {:.1}% black in the \
             same call whose marked half measured {:.1}%",
            unmarked_half * 100.0,
            marked_half * 100.0
        );
        assert!(
            marked_half < 0.99,
            "the marked half of the line was held at {:.1}% black — the mask's \
             per-pixel structure is not reaching the pin map",
            marked_half * 100.0
        );
    }

    /// The match is against the NOMINAL palette entry, not the measured ink.
    /// test_palette()'s red is official (255,0,0) / actual (200,50,50).
    ///
    /// The brief's original version dithered a flat 2 px row of official red
    /// with no neighbours and asserted the output was ink 2. Measured: it is
    /// ink 2 even completely unpinned, because red is still the nearest
    /// palette entry to (255,0,0) by plain distance — matching against
    /// `actual(i)` instead of `official(i)` would make the exact-match check
    /// fail to fire for every pixel in this scene, and the test could not
    /// tell, since ordinary nearest-match dithering lands on the same ink
    /// anyway. Replaced with a 2 px line of official red between a hostile
    /// green field — far enough from every ink in `test_palette()` to
    /// diffuse hard.
    ///
    /// Ruling 22 revives that exact hazard, because the eligible arm is now
    /// also matched against the NOMINAL palette, in which (255,0,0) IS entry 2
    /// at distance zero. So the baseline is no longer `None` (that would be
    /// the *measured* model, a different scene). It is a line of (254,0,0)
    /// under the SAME all-unmarked mask: visually the same red, nominally
    /// still nearest to entry 2, but one byte off, so it cannot pin. Measured:
    /// 53.1% ink 2. An `actual(i)` mutant makes the exact line behave exactly
    /// like that control; `official(i)` holds it at 100%.
    #[test]
    fn the_exact_match_is_against_the_nominal_entry() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        let field = Srgb::from_u8(0, 255, 0);
        let red = Srgb::from_u8(255, 0, 0);
        // One byte off official red: same colour model, same mask, no pin.
        let near_red = Srgb::from_u8(254, 0, 0);
        let (w, h) = (HOSTILE_W, HOSTILE_H);
        let eligible = vec![false; w * h];

        let pinned =
            ditherer.dither_with_regions(&hostile_line_field(field, red), w, h, Some(&eligible));
        let unpinnable = ditherer.dither_with_regions(
            &hostile_line_field(field, near_red),
            w,
            h,
            Some(&eligible),
        );

        let control_share = line_ink_share(&unpinnable, 2);
        let pinned_share = line_ink_share(&pinned, 2);

        // The scenario must be hostile under the very colour model the pinned
        // arm runs in, or the pinned result below proves nothing: nominal red
        // is entry 2 at distance zero, so nearest-match alone could hold this
        // line at 100% with no pin involved.
        assert!(
            control_share < 0.99,
            "a one-byte-off red line came back {:.1}% ink 2 under the pinned \
             arm's own mask — nearest-match alone is holding this line, so \
             the pinned result below is not attributable to the nominal \
             exact match",
            control_share * 100.0
        );
        assert_eq!(
            pinned_share,
            1.0,
            "an author-written nominal red line came back only {:.1}% \
             recognised as ink 2 (same-model unpinnable control: {:.1}%)",
            pinned_share * 100.0,
            control_share * 100.0
        );
    }

    /// A pixel that is not exactly an ink is never pinned, however eligible.
    ///
    /// Task 2's history, still relevant: the brief's original version used a
    /// flat, isolated `(1,0,0)` row with no neighbours. Measured then: with a
    /// correct exact match that row never pins (as intended), so the assertion
    /// held trivially — and a "compare with a tolerance instead of `==`" mutant
    /// made it pass too, because normal nearest-match dithering *also*
    /// quantizes an isolated near-black pixel to black; there was no error in
    /// flight for a wrongly-tolerant pin to change. Hence the hostile 2 px line
    /// geometry, kept here: a tolerant match pins the near-miss line to 100%
    /// black, visibly different from the erosion an exact match leaves.
    ///
    /// Task 5 restructure (deviation, declared in the report). Task 2's shape
    /// was `dither_with_regions(Some(eligible)) == dither_with_regions(None)`.
    /// Under ruling 22 the eligible arm is ALSO the nominal colour model while
    /// the `None` arm is the measured one, so the two arms now differ for a
    /// reason that has nothing to do with pinning: measured, the field
    /// (192,96,32) sits near actual red (200,50,50); nominal, its nearest ink
    /// is (255,0,0), which is much further away and mixes far more black into
    /// the field. The equality failed on the very first run with the two arms
    /// differing across the whole frame, field included. There is no longer any
    /// way to spell "same colour model, pinning off" — unmarked implies nominal
    /// implies pin-eligible, by design — so no same-model baseline exists.
    ///
    /// Replaced with a within-arm comparison at fixed colour model: both scenes
    /// run under the SAME all-unmarked mask, differing only in the line colour.
    /// The exact-black line is the control and must be held at exactly 100%;
    /// the one-byte-off line must not be. That control is what lets this test
    /// attribute the near-miss erosion to the inexactness of the match rather
    /// than to pinning being off, mis-wired, or the scene not being hostile —
    /// it is the non-degeneracy assertion in its strongest available form.
    ///
    /// Consequently this version DOES catch a pinning-disabled mutant too (the
    /// control fails), which Task 2's version explicitly could not.
    #[test]
    fn a_near_miss_is_not_pinned() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette)
            .noise_scale(0.0)
            .serpentine(false);

        // (255,128,0), not Task 2's (192,96,32): under the nominal colour
        // model that older field holds even an unpinnable (1,0,0) line at
        // 100% black, so a tolerant match would have been indistinguishable
        // from an exact one. Measured at (255,128,0): 85.9%.
        let field = Srgb::from_u8(255, 128, 0);
        let black = Srgb::from_u8(0, 0, 0);
        // One byte off black in one channel.
        let near_black = Srgb::from_u8(1, 0, 0);
        let (w, h) = (HOSTILE_W, HOSTILE_H);
        // All unmarked: nominal model, pin-eligible everywhere. Identical for
        // both scenes, so the only variable is the line colour.
        let unmarked = vec![false; w * h];

        let exact =
            ditherer.dither_with_regions(&hostile_line_field(field, black), w, h, Some(&unmarked));
        let near = ditherer.dither_with_regions(
            &hostile_line_field(field, near_black),
            w,
            h,
            Some(&unmarked),
        );

        let exact_share = line_ink_share(&exact, 0);
        let near_share = line_ink_share(&near, 0);

        // Control: at this exact model and mask, a line that IS the ink is
        // held completely. Without this the assertion below cannot tell
        // "correctly refused to pin a near-miss" from "pinned nothing at all".
        assert_eq!(
            exact_share,
            1.0,
            "the exact-black control line came back only {:.1}% black — \
             pinning is not firing here, so the near-miss result below \
             would prove nothing",
            exact_share * 100.0
        );
        assert!(
            near_share < 0.99,
            "a line one byte off black was held at {:.1}% black under the \
             same mask that holds exact black at 100% — the match is not \
             exact",
            near_share * 100.0
        );
    }

    /// A wrong-length mask trips the debug guard rather than passing quietly.
    ///
    /// The release-build fallback (silently degrade to `None`, reverting the
    /// colour model for the whole frame) is deliberately NOT tested here,
    /// because it cannot be: `cargo test` builds with debug assertions on, so
    /// the guard fires before the fallback is reachable. This test pins the
    /// half that is observable, and the documented `# Panics` section carries
    /// the half that is not. That asymmetry is the reason the fallback is
    /// worth documenting loudly.
    #[test]
    #[should_panic(expected = "does not match pixel count")]
    fn a_wrong_length_mask_trips_the_debug_guard() {
        let ditherer = EinkDitherer::new(test_palette()).noise_scale(0.0);
        let px = hostile_line_field(Srgb::from_u8(192, 96, 32), Srgb::from_u8(0, 0, 0));
        let short = vec![false; px.len() - 1];
        let _ = ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, Some(&short));
    }

    /// The matching-length case is the other direction: the mask is honoured.
    ///
    /// Without this, the guard test above is compatible with a mutant that
    /// refuses EVERY mask — length-checked or not — and the feature would be
    /// entirely off with both tests green.
    #[test]
    fn a_correct_length_mask_is_honoured() {
        let ditherer = EinkDitherer::new(test_palette()).noise_scale(0.0);
        let field = Srgb::from_u8(192, 96, 32);
        let px = hostile_line_field(field, Srgb::from_u8(0, 0, 0));
        let unmarked = vec![false; px.len()];

        let with_mask = ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, Some(&unmarked));
        let without = ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, None);

        // All-unmarked pins the black line; None does not. If a correct-length
        // mask were being dropped, these two would agree.
        assert_eq!(
            line_ink_share(&with_mask, 0),
            1.0,
            "an all-unmarked mask of the right length did not pin the line"
        );
        assert!(
            line_ink_share(&without, 0) < 0.99,
            "the None baseline pinned the line too, so this comparison cannot \
             tell an honoured mask from a dropped one"
        );
    }

    /// `dither()` is `dither_with_regions(.., None)` and neither pins.
    ///
    /// Today that equality is trivially true — `dither()` is a one-line
    /// delegation (see its definition above). The test exists to catch a
    /// future change that has `dither()` fabricate a mask of its own.
    ///
    /// It catches exactly one such mutant: an all-**false** (all-structure)
    /// mask, which turns pinning on everywhere and rescues the line. It
    /// CANNOT catch an all-**true** mask, because an all-continuous mask is
    /// bit-identical to `None` by construction — asserted separately by
    /// `an_all_continuous_mask_is_bit_identical_to_no_mask`. Do not read this
    /// test as covering that case.
    ///
    /// The brief's original version used a smooth (i*4, 128, 255-i*4)
    /// gradient, which never lands exactly on any test_palette() ink — so
    /// even the catchable mutant would have had nothing to pin, and the two
    /// calls would agree by accident. Reuse the hostile
    /// black-line-in-a-saturated-field geometry so a fabricated mask visibly
    /// rescues the line. The erosion of `b` (the definitely-unpinned
    /// baseline) is asserted directly, not just documented, so a future
    /// change that stops this scene being hostile must fail loudly here
    /// rather than pass a now-tautological equality. This does NOT catch a
    /// mutant that removes the pin path entirely — `dither()` not pinning
    /// is exactly what's expected here, so it can't tell that apart from
    /// correct behaviour. That mutant is caught by
    /// `eligibility_decides_where_pinning_applies` and
    /// `the_exact_match_is_against_the_nominal_entry` instead, which assert
    /// pinning DOES fire.
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
        let b = ditherer.dither_with_regions(&px, w, h, None);

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
        let eligible = vec![false; w * h];

        let with = ditherer.dither_with_regions(&px, w, h, Some(&eligible));
        let without = ditherer.dither_with_regions(&px, w, h, None);

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
