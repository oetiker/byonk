//! The gamut mapper: hull, chroma-limit table, content adaptation and knee,
//! assembled into a per-frame operation.
//!
//! Correction happens on the rasterized frame, immediately before dithering —
//! the only point where the mapping sees the pixels as they will actually be
//! dithered, after scaling, compositing and any SVG filters.
//!
//! This deliberately trades colorimetric accuracy for discriminability. Mean
//! dE against the original is *expected to get worse*; what improves is the
//! preservation of differences — gradients that used to band, hues that used
//! to collapse onto one ink, and hue ordering that used to invert.
//!
//! # Which way colours are compressed
//!
//! Along a ray converging on **mid-grey** on the neutral axis — lightness and
//! chroma give way together — rather than horizontally at fixed lightness.
//!
//! An earlier revision compressed chroma alone, reasoning that because a
//! six-ink palette contains both black and white, every `(L, h)` has a
//! non-empty `[0, Cmax]`, so a fixed-lightness move always lands in gamut.
//! That is true and insufficient: landing *somewhere* in gamut is not the same
//! as the palette's own inks surviving. The hull's constant-`L` slice pinches,
//! and where it does, the horizontal ray leaves the hull long before it reaches
//! the ink. The panel's yellow was the proof — at its own lightness the
//! reachable chroma is 0.073 against the ink's 0.197, so saturated yellow
//! washed out to cream and **the panel could not render its own ink**, coming
//! back at 42% of its chroma while red, green and blue managed 82%.
//!
//! Anchoring at each hue's cusp is the textbook answer and was measured at 40%:
//! the cusps sit within 0.012 of the inks' own lightness, so the ray still
//! climbs into the pinched region. Mid-grey restores all four panel inks to the
//! knee's design point exactly (`t_max = 1.000`).
//!
//! The cost is that lightness now moves. For a colour that is genuinely out of
//! gamut that is the point. For an in-gamut one it is a liability, and it is
//! bounded by the knee: a high-`L`, low-chroma colour's ray exits the hull at
//! the *white point*, so it reads as boundary-saturated even though its chroma
//! was never out of gamut. Every colour with `t_max > 1/knee` is returned
//! untouched, so the higher the knee the thinner that shell — at `knee = 0.8` a
//! faintly warm near-white darkens by up to 0.10 in `L`, at `0.99` by 0.003.
//! `ray_geometry_diagnostic` measures this; `a_tinted_near_white_keeps_its_lightness`
//! guards it.
//!
//! Lightness is clamped into the hull's achievable range before mapping, which
//! also covers palettes lacking a near-black or near-white.

use super::adapt::adaptation_factor;
use super::cmax::CmaxTable;
use super::hull::Hull;
use super::knee::compress_chroma;
use crate::{LinearRgb, Oklab, Oklch, Palette, Srgb};

/// Tuning knobs. Frame-level, not per adaptation group: groups change only
/// which pixels are measured together to derive `R`, not the curve's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GamutOptions {
    /// Where compression begins, as a fraction of the distance to the gamut
    /// boundary along the compression ray.
    ///
    /// The default is deliberately close to 1.0, which is unusual — the ACES
    /// 1.3 Reference Gamut Compression thresholds sit at `0.815`, `0.803`,
    /// `0.880` in the same normalised units. The reason is structural: the
    /// shoulder above the knee is asymptotic to the boundary, so **no input can
    /// ever be mapped onto the boundary** — and a palette ink *is* the
    /// boundary. Whatever the knee is set to is, near enough, the fraction of
    /// its own chroma an ink comes back with.
    ///
    /// The knee's usual justification is that its headroom keeps out-of-gamut
    /// colours distinguishable. Measured over 24 hues x 5 lightnesses, that is
    /// not what it buys here — the tail is already sized by `R`, and raising
    /// the knee *improves* the banding metric:
    ///
    /// ```text
    /// knee   inks keep   tail span   distinct outputs
    /// 0.80         82%      0.0222              76.4%
    /// 0.90         91%      0.0220              78.8%
    /// 0.95         95%      0.0219              80.6%
    /// 0.99         99%      0.0218              81.3%
    /// ```
    ///
    /// The whole tail span is about one JND (0.02 in Oklab chroma), so the
    /// 0.0004 given up is a difference the panel could not show; the 5 points
    /// of surviving distinct 8-bit outputs are real. An earlier draft chose 0.6
    /// on the reasoning that this gamut is small enough that a high knee would
    /// crush the vivid range into a sliver near the boundary; that is not what
    /// the measurement shows.
    ///
    /// The knee also bounds the ray geometry's one liability — see the module
    /// docs. At 0.8 a faintly warm near-white darkens by up to 0.10 in `L`;
    /// at 0.99, by 0.003.
    pub knee: f32,
    /// Interpolation between input and mapped chroma:
    /// `C_out = C + amount * (C' - C)`.
    ///
    /// Clamped to `[0.0, 1.0]` at the point of use, as `knee` and
    /// `max_compression` are. Outside that range the expression stops being an
    /// interpolation: a negative `amount` inverts the correction into a chroma
    /// *boost*, and `amount > 1` desaturates past the mapped target, towards
    /// grey. Neither is a gamut mapping, so neither is reachable.
    ///
    /// At `1.0` the output is the mapped chroma; at `0.0` the region is
    /// untouched, which makes it a clean A/B switch for judging the effect on
    /// a real panel. **Only `amount = 1.0` guarantees in-gamut output** —
    /// lower values can leave chroma above `Cmax`, which the ditherer then
    /// clips as it does today. It is a comparison and taste control, not a
    /// correctness one.
    pub amount: f32,
    /// Cap on `R` — how far out of gamut the curve is willing to stretch its
    /// tail to accommodate a region's most extreme content.
    ///
    /// Raising it lets an extremely vivid image spread its tail across a wider
    /// input range, at the cost of compressing everything *above the knee*
    /// harder; lowering it protects near-boundary chroma and pushes the
    /// extremes further into the asymptotic tail, where they stay
    /// distinguishable but heavily compressed. It has **no effect at all**
    /// below the knee — that region is identity at every `R`, which is the
    /// property `sub_knee_chroma_is_untouched_however_large_r_is` guards.
    pub max_compression: f32,
}

impl Default for GamutOptions {
    fn default() -> Self {
        Self {
            knee: 0.99,
            amount: 1.0,
            max_compression: 2.5,
        }
    }
}

/// Where on the neutral axis the compression lines converge (ruling 16).
///
/// Mid-grey, not the source lightness and not the hue's cusp. Anchoring at the
/// cusp is the textbook answer and was measured at 40% survival on the panel's
/// yellow against mid-grey's 82%: the cusps sit within 0.012 of the inks' own
/// lightness, so the ray still climbs into the pinched region.
const ANCHOR_L: f32 = 0.5;

/// How far past the source the boundary search looks. The source sits at
/// `t = 1`, so this admits boundaries up to 6x further out than the colour.
const T_HI: f32 = 6.0;

/// Bisection steps for the boundary search. 24 halvings of `[0, 6]` resolve
/// `t_max` to about 4e-7 — far below 8-bit output quantisation.
const T_STEPS: usize = 24;

/// Below this chroma a colour is treated as neutral and left alone. Its
/// compression ray would be the neutral axis itself, whose "boundary" is the
/// white or black point — a colour that is perfectly renderable and must not
/// be compressed toward grey.
const ACHROMATIC_C: f32 = 1e-6;

/// Maps colours into a palette's reachable hull. Build once per palette.
#[derive(Debug, Clone)]
pub struct GamutMapper {
    hull: Hull,
    table: CmaxTable,
    l_min: f32,
    l_max: f32,
}

impl GamutMapper {
    /// Build from the colours the ditherer targets — measured when they
    /// resolve, official otherwise. The hull and the dither target must not
    /// diverge.
    pub fn new(palette: &Palette) -> Self {
        let hull = Hull::from_palette(palette);
        let table = CmaxTable::build(&hull);
        let (l_min, l_max) = table.lightness_range();
        Self {
            hull,
            table,
            l_min,
            l_max,
        }
    }

    /// Where this colour's compression ray converges, clamped into the hull's
    /// achievable lightness range so the anchor is itself renderable.
    fn anchor_l(&self) -> f32 {
        ANCHOR_L.clamp(self.l_min, self.l_max)
    }

    fn inside(&self, l: f32, c: f32, h: f32) -> bool {
        self.hull.contains(LinearRgb::from(Oklab::from(Oklch {
            l,
            c: c.max(0.0),
            h,
        })))
    }

    /// Largest `t` for which `anchor + t * (source - anchor)` is still in the
    /// hull, with the source at `t = 1`.
    ///
    /// Bisection, so it finds the *first* exit. Where the locus leaves and
    /// re-enters this returns the conservative answer rather than jumping the
    /// gap — the constant-`L` version of exactly that re-entry is what stranded
    /// the panel's yellow.
    ///
    /// `f32::INFINITY` for an achromatic input: it needs no mapping, and the
    /// degenerate ray straight up the neutral axis would otherwise report a
    /// boundary at the white point.
    fn t_max(&self, lch: Oklch) -> f32 {
        if lch.c <= ACHROMATIC_C {
            return f32::INFINITY;
        }
        let a_l = self.anchor_l();
        let dl = lch.l - a_l;
        let at = |t: f32| self.inside(a_l + t * dl, t * lch.c, lch.h);
        if !at(0.0) {
            return 0.0;
        }
        if at(T_HI) {
            return T_HI;
        }
        let (mut lo, mut hi) = (0.0f32, T_HI);
        for _ in 0..T_STEPS {
            let mid = 0.5 * (lo + hi);
            if at(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// How far out of gamut this colour is along its compression ray, 1.0 being
    /// exactly on the boundary. The source sits at `t = 1`, so this is
    /// `1 / t_max`. Infinite when the ray leaves the hull immediately.
    pub fn rho(&self, c: Srgb) -> f32 {
        let mut lch = Oklch::from(Oklab::from(LinearRgb::from(c)));
        lch.l = lch.l.clamp(self.l_min, self.l_max);
        Self::rho_from(self.t_max(lch))
    }

    fn rho_from(t_max: f32) -> f32 {
        if t_max <= 0.0 {
            f32::INFINITY
        } else {
            1.0 / t_max
        }
    }

    /// The ray parameter the mapper produces for a source at `t = 1`.
    ///
    /// `compress_chroma` is homogeneous, so it applies to a ray parameter
    /// exactly as it does to a chroma.
    fn mapped_t(&self, t_max: f32, r: f32, opts: GamutOptions) -> f32 {
        let t = compress_chroma(1.0, t_max, opts.knee, r);
        1.0 + opts.amount.clamp(0.0, 1.0) * (t - 1.0)
    }

    /// The chroma the mapper would produce, in float. Separate from
    /// [`GamutMapper::map_color`] so monotonicity can be asserted without 8-bit
    /// quantisation in the way.
    ///
    /// Note this reports only half the operation: under ruling 16 the mapping
    /// moves lightness too. Use [`GamutMapper::mapped_point`] where that
    /// matters.
    #[cfg(test)]
    pub(crate) fn mapped_chroma(&self, c: f32, h: f32, l: f32, r: f32, opts: GamutOptions) -> f32 {
        self.mapped_point(Oklch { l, c, h }, r, opts).c
    }

    /// Map a point along its compression ray. The lightness is clamped into the
    /// hull's achievable range first, as the old fixed-`L` mapper did.
    pub(crate) fn mapped_point(&self, src: Oklch, r: f32, opts: GamutOptions) -> Oklch {
        let l = src.l.clamp(self.l_min, self.l_max);
        let src = Oklch { l, ..src };
        self.mapped_from_cache(src, self.t_max(src), r, opts)
    }

    /// Map one colour with an explicit adaptation factor.
    pub fn map_color(&self, color: Srgb, r: f32, opts: GamutOptions) -> Srgb {
        let src = Oklch::from(Oklab::from(LinearRgb::from(color)));
        Self::to_srgb(self.mapped_point(src, r, opts))
    }

    /// Ruling 5: `linear_to_srgb` carries an epsilon-free `debug_assert!`, so
    /// conversion rounding must be clamped away before it is reached.
    fn to_srgb(c: Oklch) -> Srgb {
        let linear = LinearRgb::from(Oklab::from(c));
        Srgb::from(LinearRgb::new(
            linear.r.clamp(0.0, 1.0),
            linear.g.clamp(0.0, 1.0),
            linear.b.clamp(0.0, 1.0),
        ))
    }

    /// Map every masked pixel in place.
    ///
    /// Derives one adaptation factor from the masked pixels, then applies the
    /// curve. When the masked content is already in gamut (`R <= 1`) this is
    /// the identity and nothing is needlessly desaturated.
    pub fn map_frame(&self, pixels: &mut [Srgb], mask: &[bool], opts: GamutOptions) {
        debug_assert_eq!(pixels.len(), mask.len(), "mask must match the frame");
        if opts.amount <= 0.0 {
            return;
        }
        // No meaningful compression target: leave the content alone rather
        // than crushing it onto a lightness the panel cannot render. See
        // `CmaxTable::is_unmappable`.
        if self.table.is_unmappable() {
            return;
        }

        // The boundary search is the expensive part of the whole operation, so
        // it is done once per masked pixel and reused: the adaptation pass and
        // the mapping pass need the same `t_max`.
        let srcs: Vec<(Oklch, f32)> = pixels
            .iter()
            .zip(mask.iter())
            .filter(|(_, &m)| m)
            .map(|(p, _)| {
                let mut lch = Oklch::from(Oklab::from(LinearRgb::from(*p)));
                lch.l = lch.l.clamp(self.l_min, self.l_max);
                (lch, self.t_max(lch))
            })
            .collect();
        if srcs.is_empty() {
            return;
        }

        // `adaptation_factor` reorders what it is given, so the cache is copied
        // rather than lent.
        let mut rhos: Vec<f32> = srcs.iter().map(|&(_, t)| Self::rho_from(t)).collect();
        let r = adaptation_factor(&mut rhos, opts.max_compression);
        // Identity: content already fits. Skip rather than desaturate.
        if r <= 1.0 && !self.table.is_achromatic() {
            return;
        }

        let mut src = srcs.into_iter();
        for (p, &m) in pixels.iter_mut().zip(mask.iter()) {
            if !m {
                continue;
            }
            let (lch, t_max) = src.next().expect("one cache entry per masked pixel");
            *p = Self::to_srgb(self.mapped_from_cache(lch, t_max, r, opts));
        }
    }

    /// [`GamutMapper::mapped_point`] with the boundary search already done.
    fn mapped_from_cache(&self, src: Oklch, t_max: f32, r: f32, opts: GamutOptions) -> Oklch {
        if src.c <= ACHROMATIC_C {
            return src;
        }
        if t_max <= 0.0 {
            return Oklch { c: 0.0, ..src };
        }
        let a_l = self.anchor_l();
        let t = self.mapped_t(t_max, r, opts);
        Oklch {
            l: a_l + t * (src.l - a_l),
            c: (t * src.c).max(0.0),
            h: src.h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamut::hull::Hull;
    use crate::gamut::test_support::{four_grey, panel_measured, six_colour};
    use crate::{LinearRgb, Oklab, Oklch, Srgb};

    /// A spread of saturated colours, well outside a six-ink gamut.
    fn vivid_frame() -> Vec<Srgb> {
        let mut v = Vec::new();
        for i in 0..64 {
            for j in 0..64 {
                v.push(Srgb::from_u8((i * 4) as u8, (j * 4) as u8, 200));
            }
        }
        v
    }

    #[test]
    fn mapping_twice_equals_mapping_once() {
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let mask = vec![true; 64 * 64];

        let mut once = vivid_frame();
        m.map_frame(&mut once, &mask, opts);

        let mut twice = once.clone();
        m.map_frame(&mut twice, &mask, opts);

        for (i, (a, b)) in once.iter().zip(twice.iter()).enumerate() {
            assert_eq!(
                a.to_bytes(),
                b.to_bytes(),
                "pixel {i} changed on the second pass: {:?} -> {:?}",
                a.to_bytes(),
                b.to_bytes()
            );
        }
    }

    #[test]
    fn in_gamut_content_is_returned_unchanged() {
        let p = six_colour();
        let m = GamutMapper::new(&p);
        // Midpoints between palette entries are inside the hull by convexity.
        let mut pixels: Vec<Srgb> = Vec::new();
        for i in 0..p.len() {
            for j in 0..p.len() {
                let a = p.actual_linear(i);
                let b = p.actual_linear(j);
                pixels.push(Srgb::from(LinearRgb::new(
                    0.5 * (a.r + b.r),
                    0.5 * (a.g + b.g),
                    0.5 * (a.b + b.b),
                )));
            }
        }
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(a.to_bytes(), b.to_bytes(), "in-gamut pixel {i} was altered");
        }
    }

    #[test]
    fn sub_knee_chroma_is_untouched_however_large_r_is() {
        // The spec's stated promise: "compression only bites above k*Cmax, so
        // low-chroma content passes through untouched however large R becomes.
        // A mostly-grey photo with one vivid flower does not go flat."
        //
        // This is the absolute property the suite never asserted, and the one
        // that the adaptation step broke.
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let (h, l) = (0.7f32, 0.55f32);
        let c_max = m.table.sample(h, l);
        assert!(c_max > 0.0, "test fixture must admit chroma here");

        // Half way to the knee — unambiguously in the identity region.
        let c = 0.5 * opts.knee * c_max;
        for r in [1.0f32, 1.5, 2.0, opts.max_compression] {
            let out = m.mapped_chroma(c, h, l, r, opts);
            assert!(
                (out - c).abs() < 1e-6,
                "R = {r} altered sub-knee chroma: {c} -> {out}"
            );
        }
    }

    #[test]
    fn a_colour_on_the_gamut_boundary_keeps_most_of_its_chroma() {
        // A colour at exactly Cmax is perfectly renderable and needs no
        // compression at all. It sits above the knee, so a knee at k < 1 must
        // move it a little — but only within the top (1-k) of its range, not
        // most of the way to grey.
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let (h, l) = (0.7f32, 0.55f32);
        let c_max = m.table.sample(h, l);
        assert!(c_max > 0.0, "test fixture must admit chroma here");

        let out = m.mapped_chroma(c_max, h, l, opts.max_compression, opts);
        assert!(
            out > 0.75 * c_max,
            "boundary colour kept only {:.0}% of its chroma",
            100.0 * out / c_max
        );
    }

    #[test]
    fn every_palette_ink_survives_at_the_knees_design_point() {
        // A palette ink *is* the gamut boundary. Compressing it is compressing
        // a colour that needs no compression, so the only loss it may suffer is
        // the knee's own shoulder — `compress_chroma(1, 1, k, R)`, the value an
        // on-boundary colour is designed to come back at.
        //
        // Fixed-lightness compression cannot honour this: where the hull's
        // constant-`L` slice pinches, the horizontal ray from the neutral axis
        // leaves the hull long before it reaches the ink. Ruling 16 replaces
        // that ray with one anchored at mid-grey.
        let opts = GamutOptions::default();
        let r = opts.max_compression;
        let design = compress_chroma(1.0, 1.0, opts.knee, r);

        // Every ink is reported, not just the first to fail: the spread across
        // inks is the diagnosis. One ink far below the rest means a geometric
        // limitation, not a curve that is uniformly slightly too eager.
        let mut failures = Vec::new();
        for (name, p, floor) in [
            // `six_colour`'s idealised sRGB primaries are not the ruling's
            // subject and cannot meet the design point: its blue vertex sits
            // where the constant-hue OKLch ray bulges outside the linear-RGB
            // hull, so bisection finds the boundary at t_max = 0.861 and the
            // ink reads as out of gamut. That is a measured geometric fact
            // about a palette this project does not ship, not a slack
            // threshold — the shipping palette below gets the real claim.
            ("six_colour", six_colour(), 0.70 * design),
            ("panel_measured", panel_measured(), 0.95 * design),
        ] {
            let m = GamutMapper::new(&p);
            for i in 0..p.len() {
                let ink = Srgb::from(p.actual_linear(i));
                let c_in = Oklch::from(Oklab::from(LinearRgb::from(ink))).c;
                if c_in < 0.01 {
                    continue; // black and white carry no chroma to lose
                }
                let out = m.map_color(ink, r, opts);
                let c_out = Oklch::from(Oklab::from(LinearRgb::from(out))).c;
                let kept = c_out / c_in;
                if kept < floor {
                    failures.push(format!(
                        "{name} ink {i} {:?} kept {:.0}%, floor {:.0}%",
                        ink.to_bytes(),
                        100.0 * kept,
                        100.0 * floor
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "design point is {:.0}%; below it: {}",
            100.0 * design,
            failures.join(", ")
        );
    }

    #[test]
    fn a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee() {
        // The ray geometry's one liability: a high-`L`, low-chroma colour's
        // ray leaves the hull at the white point, so it reads as
        // boundary-saturated and the knee pulls it toward mid-grey — darkening
        // a highlight whose chroma was never out of gamut. Photographs are full
        // of these, and whole-image mean |dL| hides them completely.
        //
        // The knee is what bounds it: everything with `t_max > 1/knee` is
        // returned untouched. This is asserted at 0.99 rather than at whatever
        // `default()` currently says, because it is a statement about the knee
        // we intend to ship. It is not vacuous — the same pixel moves -0.084 at
        // knee 0.8, twenty-four times this tolerance. See
        // `ray_geometry_diagnostic` for the sweep.
        let m = GamutMapper::new(&panel_measured());
        let opts = GamutOptions {
            knee: 0.99,
            ..GamutOptions::default()
        };
        // RGB (250, 246, 246) — a faintly warm near-white, in gamut.
        let src = Oklch::from(Oklab::from(LinearRgb::from(Srgb::from_u8(250, 246, 246))));
        let out = m.mapped_point(src, opts.max_compression, opts);
        let dl = out.l - src.l;
        assert!(
            dl.abs() < 0.01,
            "a tinted near-white moved {dl:+.4} in lightness"
        );
    }

    /// The ray geometry replaced one table lookup per pixel with a bisection of
    /// the hull, which was the port's main unmeasured risk. Run with
    /// `--release`; a debug build is not the shipping cost.
    #[test]
    #[ignore = "benchmark"]
    fn map_frame_cost_on_a_panel_sized_frame() {
        let m = GamutMapper::new(&panel_measured());
        // 800x480, the panel's own resolution, filled with a saturated hue
        // sweep so nothing short-circuits on the achromatic path.
        let (w, h) = (800usize, 480usize);
        let mut pixels: Vec<Srgb> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as u8, (i / w) as u8);
                Srgb::from_u8(x.wrapping_mul(3), y.wrapping_mul(5), 200)
            })
            .collect();
        let mask = vec![true; pixels.len()];

        let start = std::time::Instant::now();
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        let dt = start.elapsed();
        println!(
            "map_frame over {}x{} = {} px: {:?} ({:.2} us/px)",
            w,
            h,
            w * h,
            dt,
            dt.as_secs_f64() * 1e6 / (w * h) as f64
        );
    }

    /// Evidence for ruling 16, not a guard. Prints where each ink's boundary
    /// actually sits along its compression ray, and what the map does to
    /// neutrals — the two populations the ray geometry changes most.
    #[test]
    #[ignore = "diagnostic"]
    fn ray_geometry_diagnostic() {
        let opts = GamutOptions::default();
        let r = opts.max_compression;
        println!(
            "design point for an on-boundary colour: {:.3}",
            compress_chroma(1.0, 1.0, opts.knee, r)
        );

        for (name, p) in [
            ("six_colour", six_colour()),
            ("panel_measured", panel_measured()),
        ] {
            let m = GamutMapper::new(&p);
            println!("\n{name}  (anchor L = {:.3})", m.anchor_l());
            for i in 0..p.len() {
                let ink = Srgb::from(p.actual_linear(i));
                let src = Oklch::from(Oklab::from(LinearRgb::from(ink)));
                if src.c < 0.01 {
                    continue;
                }
                let t_max = m.t_max(src);
                let out = m.mapped_point(src, r, opts);
                println!(
                    "  ink {i} {:?}  L {:.3} C {:.3}  t_max {:.3}  \
                     kept {:.0}%  dL {:+.3}",
                    ink.to_bytes(),
                    src.l,
                    src.c,
                    t_max,
                    100.0 * out.c / src.c,
                    out.l - src.l
                );
            }

            // Neutrals: rho is 0 under fixed-L geometry, but a mid-grey ray
            // sees the white/black point as "the boundary". If that pulls
            // highlights toward grey it is a new defect, not a fix.
            // Near-neutrals are the ray's risk population. A high-`L`, low-`C`
            // colour's ray exits the hull at the *white point*, so `rho ~ 1`
            // and the knee treats it as boundary-saturated — but its chroma was
            // never out of gamut. The knee is what decides whether that costs
            // anything, so sweep it.
            println!("  near-neutrals, dL by knee:");
            println!(
                "    {:>22}  {:>8} {:>8} {:>8}",
                "", "k=0.80", "k=0.95", "k=0.99"
            );
            for v in [16u8, 64, 128, 192, 230, 250] {
                for tint in [0u8, 4, 12] {
                    let g = Srgb::from_u8(v, v.saturating_sub(tint), v.saturating_sub(tint));
                    let src = Oklch::from(Oklab::from(LinearRgb::from(g)));
                    let t_max = m.t_max(src);
                    let dl =
                        |k: f32| m.mapped_point(src, r, GamutOptions { knee: k, ..opts }).l - src.l;
                    println!(
                        "    grey {v:>3} tint {tint:>2} t_max {t_max:>5.3}  \
                         {:>+8.4} {:>+8.4} {:>+8.4}",
                        dl(0.80),
                        dl(0.95),
                        dl(0.99)
                    );
                }
            }
        }
    }

    #[test]
    fn hue_is_preserved() {
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        for c in [
            Srgb::from_u8(255, 0, 128),
            Srgb::from_u8(0, 200, 255),
            Srgb::from_u8(180, 255, 0),
            Srgb::from_u8(120, 0, 255),
        ] {
            let out = m.map_color(c, 2.0, opts);
            let h_in = Oklch::from(Oklab::from(LinearRgb::from(c))).h;
            let h_out = Oklch::from(Oklab::from(LinearRgb::from(out))).h;
            let mut d = (h_out - h_in).abs();
            if d > std::f32::consts::PI {
                d = std::f32::consts::TAU - d;
            }
            // 8-bit output quantisation dominates this tolerance.
            assert!(d < 0.05, "hue moved by {d} rad for {:?}", c.to_bytes());
        }
    }

    #[test]
    fn the_map_is_strictly_monotonic_along_a_compression_ray() {
        // The curve's defining property: two colours that differed before still
        // differ after, nothing collapses onto a shared value.
        //
        // Stated along a **ray**, because that is the direction the map acts
        // in. Sweeping chroma at fixed lightness instead — as this test used to
        // — gives every sample its own ray, its own boundary and its own
        // compression factor, and the product `t * c` is then not monotonic:
        // past the shoulder it asymptotes and drifts back by ~2e-5 as the
        // flattening ray meets a nearer boundary. That drift is geometry, not a
        // collapse, and the fixed-`L` form reported it as one.
        //
        // Asserted on floats, not bytes: 8-bit output quantisation legitimately
        // collapses adjacent values.
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let anchor = m.anchor_l();
        let (h, dir_l, dir_c) = (0.7f32, 0.55f32 - 0.5, 0.20f32);
        // Distance from the anchor along the ray — the quantity the knee
        // actually compresses.
        let mapped_d = |s: f32| {
            let out = m.mapped_point(
                Oklch {
                    l: anchor + s * dir_l,
                    c: s * dir_c,
                    h,
                },
                2.0,
                opts,
            );
            ((out.l - anchor).powi(2) + out.c.powi(2)).sqrt()
        };

        // Swept to s = 2.5, i.e. Oklab chroma 0.50, beyond the ~0.33 any sRGB
        // colour can reach — the whole reachable domain with margin.
        //
        // Two claims, because at a fine step they are different claims. The
        // shoulder is asymptotic, so far out its increments fall below one f32
        // ulp of the limit and adjacent samples tie, or jitter by one, as the
        // bisection's own rounding lands either side. That is a fact about
        // f32, not about the design, and tightening the step only makes it
        // happen sooner. What must never happen is the map turning round.
        //
        // The tolerance is a few ulps of the value. It is not slack: the
        // inversion this guards against — measuring the fixed-`L` chroma
        // instead of the ray — was 1.8e-5, some three orders of magnitude
        // above this.
        let mut prev = f32::NEG_INFINITY;
        for i in 1..5000 {
            let s = i as f32 * 0.0005;
            let d = mapped_d(s);
            let slack = 8.0 * f32::EPSILON * d.abs();
            assert!(
                d >= prev - slack,
                "map went backwards at s={s}: {prev} -> {d}"
            );
            prev = prev.max(d);
        }

        // And at steps coarse enough to be visible, it must genuinely increase
        // — this is the banding claim, and the one a clipping mapper fails.
        let mut prev = f32::NEG_INFINITY;
        for i in 1..=64 {
            let s = i as f32 / 64.0 * 2.5;
            let d = mapped_d(s);
            assert!(d > prev, "map not increasing at s={s}: {prev} -> {d}");
            prev = d;
        }
    }

    #[test]
    fn a_saturation_ramp_never_collapses_two_colours_onto_one() {
        // The fixed-lightness companion to the ray sweep above. Chroma is not
        // monotonic here (see that test), but the mapped *points* must still
        // never coincide — that is what banding would look like.
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let (h, l) = (0.7f32, 0.55f32);
        let mut prev: Option<Oklch> = None;
        for i in 1..5000 {
            let c = i as f32 * 0.0002;
            let out = m.mapped_point(Oklch { l, c, h }, 2.0, opts);
            if let Some(p) = prev {
                let sep = ((out.l - p.l).powi(2) + (out.c - p.c).powi(2)).sqrt();
                assert!(sep > 0.0, "ramp collapsed at c={c}: {p:?} -> {out:?}");
            }
            prev = Some(out);
        }
    }

    #[test]
    fn mapped_output_lands_inside_the_hull() {
        let p = six_colour();
        let m = GamutMapper::new(&p);
        let hull = Hull::from_palette(&p);
        let mut pixels = vivid_frame();
        let mask = vec![true; pixels.len()];
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        let outside = pixels
            .iter()
            .filter(|c| !hull.contains(LinearRgb::from(**c)))
            .count();
        // 8-bit round-tripping can nudge a boundary pixel just outside.
        let ratio = outside as f32 / pixels.len() as f32;
        assert!(
            ratio < 0.02,
            "{:.1}% of mapped pixels left the hull",
            ratio * 100.0
        );
    }

    #[test]
    fn unmasked_pixels_are_never_touched() {
        let m = GamutMapper::new(&six_colour());
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mut mask = vec![false; pixels.len()];
        mask[0] = true;
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        for i in 1..pixels.len() {
            assert_eq!(
                before[i].to_bytes(),
                pixels[i].to_bytes(),
                "unmasked pixel {i} changed"
            );
        }
    }

    #[test]
    fn amount_zero_is_a_no_op() {
        let m = GamutMapper::new(&six_colour());
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(
            &mut pixels,
            &mask,
            GamutOptions {
                amount: 0.0,
                ..GamutOptions::default()
            },
        );
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(a.to_bytes(), b.to_bytes(), "amount=0 altered pixel {i}");
        }
    }

    #[test]
    fn negative_amount_is_clamped_to_a_no_op() {
        // Unclamped, `c + amount * (compressed - c)` with a negative amount
        // inverts the correction into a chroma *boost* — the opposite of
        // gamut mapping.
        let m = GamutMapper::new(&six_colour());
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(
            &mut pixels,
            &mask,
            GamutOptions {
                amount: -1.0,
                ..GamutOptions::default()
            },
        );
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(a.to_bytes(), b.to_bytes(), "amount<0 altered pixel {i}");
        }
    }

    #[test]
    fn amount_above_one_is_clamped_to_full_mapping() {
        // Unclamped, amount > 1 over-desaturates past the mapped target.
        let m = GamutMapper::new(&six_colour());
        let mask = vec![true; 64 * 64];

        let mut full = vivid_frame();
        m.map_frame(&mut full, &mask, GamutOptions::default());

        let mut over = vivid_frame();
        m.map_frame(
            &mut over,
            &mask,
            GamutOptions {
                amount: 4.0,
                ..GamutOptions::default()
            },
        );

        for (i, (a, b)) in full.iter().zip(over.iter()).enumerate() {
            assert_eq!(
                a.to_bytes(),
                b.to_bytes(),
                "amount=4 differs from amount=1 at pixel {i}"
            );
        }
    }

    #[test]
    fn greyscale_palette_desaturates_rather_than_flinging_at_an_ink() {
        let m = GamutMapper::new(&four_grey());
        let mut pixels = vec![Srgb::from_u8(220, 30, 40)];
        m.map_frame(&mut pixels, &[true], GamutOptions::default());
        let (r, g, b) = {
            let v = pixels[0].to_bytes();
            (v[0] as i32, v[1] as i32, v[2] as i32)
        };
        assert!(
            (r - g).abs() <= 2 && (g - b).abs() <= 2,
            "expected a neutral, got {r},{g},{b}"
        );
    }

    #[test]
    fn an_unmappable_hull_leaves_content_untouched() {
        // A full-volume hull whose grey axis lies entirely outside it. There
        // is no meaningful chroma target, so the mapper must decline rather
        // than desaturate — the opposite of the greyscale case above.
        let p = Palette::new(
            &[
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(255, 51, 0),
                Srgb::from_u8(255, 0, 51),
                Srgb::from_u8(204, 26, 26),
            ],
            None,
        )
        .unwrap();
        let m = GamutMapper::new(&p);
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(
                a.to_bytes(),
                b.to_bytes(),
                "unmappable hull must be the identity, pixel {i} changed"
            );
        }
    }
}
