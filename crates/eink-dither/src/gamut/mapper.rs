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
//! # Why chroma-only suffices
//!
//! Because a six-ink palette contains both pure black and pure white, every
//! `(L, h)` has a non-empty achievable range `[0, Cmax]`, so compressing
//! chroma at fixed lightness always lands in gamut. For a palette lacking a
//! near-black or near-white, lightness is first clamped into the hull's
//! achievable range.

use super::adapt::adaptation_factor;
use super::cmax::CmaxTable;
use super::hull::Hull;
use super::knee::compress_chroma;
use crate::{LinearRgb, Oklab, Oklch, Palette, Srgb};

/// Tuning knobs. Frame-level, not per adaptation group: groups change only
/// which pixels are measured together to derive `R`, not the curve's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GamutOptions {
    /// Where compression begins, as a fraction of `Cmax`.
    ///
    /// The default sits in the same band as the ACES 1.3 Reference Gamut
    /// Compression thresholds (`0.815`, `0.803`, `0.880`), which are expressed
    /// in the same normalised units.
    ///
    /// An earlier draft chose 0.6, reasoning that this gamut is small enough
    /// that almost everything falls outside it, so a high knee would crush the
    /// vivid range into a sliver near `Cmax`. Measurement does not support
    /// either half of that. Across every sRGB colour with non-zero chroma,
    /// `rho = C/Cmax` has a median of 0.91 and a p90 of 1.30 — about half the
    /// cube is outside the hull, not almost all of it. And because `map_frame`
    /// normalises by `R`, the 99th percentile of `rho`, the "sliver" only ever
    /// holds the top ~1% of a region's pixels.
    ///
    /// Measured against that, a low knee is a bad trade. At `knee = 0.6` the
    /// frame's vivid end (`rho/R = 1`) renders at 82.4% of the achievable
    /// chroma; at 0.8 it renders at 91.2%. What the lower knee buys back is
    /// separation in the out-of-gamut tail of 0.005 in Oklab chroma and below
    /// — against roughly 0.02 for one JND, on a panel that dithers six inks.
    /// It spends visible chroma to preserve differences nothing can render.
    pub knee: f32,
    /// Interpolation between input and mapped chroma:
    /// `C_out = C + amount * (C' - C)`.
    ///
    /// At `1.0` the output is the mapped chroma; at `0.0` the region is
    /// untouched, which makes it a clean A/B switch for judging the effect on
    /// a real panel. **Only `amount = 1.0` guarantees in-gamut output** —
    /// lower values can leave chroma above `Cmax`, which the ditherer then
    /// clips as it does today. It is a comparison and taste control, not a
    /// correctness one.
    pub amount: f32,
    /// Cap on `R` — literally "never compress chroma by more than this".
    ///
    /// Raising it lets an extremely vivid image adapt further, at the cost of
    /// flattening everything else; lowering it protects the bulk of the image
    /// and pushes the extremes into the knee's asymptotic tail instead, where
    /// they stay distinguishable but heavily compressed.
    pub max_compression: f32,
}

impl Default for GamutOptions {
    fn default() -> Self {
        Self {
            knee: 0.8,
            amount: 1.0,
            max_compression: 2.5,
        }
    }
}

/// Maps colours into a palette's reachable hull. Build once per palette.
#[derive(Debug, Clone)]
pub struct GamutMapper {
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
            table,
            l_min,
            l_max,
        }
    }

    /// `C / Cmax(h, L)` — how far out of gamut this colour is, 1.0 being
    /// exactly on the boundary. Infinite when the palette admits no chroma at
    /// this hue and lightness but the colour has some.
    pub fn rho(&self, c: Srgb) -> f32 {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(c)));
        let l = lch.l.clamp(self.l_min, self.l_max);
        let c_max = self.table.sample(lch.h, l);
        if c_max <= 0.0 {
            if lch.c <= 0.0 {
                0.0
            } else {
                f32::INFINITY
            }
        } else {
            lch.c / c_max
        }
    }

    /// The chroma the mapper would produce, in float. Separate from
    /// [`GamutMapper::map_color`] so monotonicity can be asserted without 8-bit
    /// quantisation in the way.
    pub(crate) fn mapped_chroma(&self, c: f32, h: f32, l: f32, r: f32, opts: GamutOptions) -> f32 {
        let l = l.clamp(self.l_min, self.l_max);
        let c_max = self.table.sample(h, l);
        let compressed = compress_chroma(c / r.max(1.0), c_max, opts.knee);
        c + opts.amount * (compressed - c)
    }

    /// Map one colour with an explicit adaptation factor.
    pub fn map_color(&self, color: Srgb, r: f32, opts: GamutOptions) -> Srgb {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(color)));
        let l = lch.l.clamp(self.l_min, self.l_max);
        let c_out = self.mapped_chroma(lch.c, lch.h, l, r, opts);
        let linear = LinearRgb::from(Oklab::from(Oklch {
            l,
            c: c_out.max(0.0),
            h: lch.h,
        }));
        // Clamp to [0, 1] to handle floating-point precision errors from color space conversions
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
        if opts.amount == 0.0 {
            return;
        }
        // No meaningful compression target: leave the content alone rather
        // than crushing it onto a lightness the panel cannot render. See
        // `CmaxTable::is_unmappable`.
        if self.table.is_unmappable() {
            return;
        }

        let mut rhos: Vec<f32> = pixels
            .iter()
            .zip(mask.iter())
            .filter(|(_, &m)| m)
            .map(|(p, _)| self.rho(*p))
            .collect();
        if rhos.is_empty() {
            return;
        }

        let r = adaptation_factor(&mut rhos, opts.max_compression);
        // Identity: content already fits. Skip rather than desaturate.
        if r <= 1.0 && !self.table.is_achromatic() {
            return;
        }

        for (p, &m) in pixels.iter_mut().zip(mask.iter()) {
            if m {
                *p = self.map_color(*p, r, opts);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamut::hull::Hull;
    use crate::gamut::test_support::{four_grey, six_colour};
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
    fn chroma_map_is_strictly_monotonic() {
        // Asserted on the float chroma function, not on bytes: 8-bit output
        // quantisation legitimately collapses adjacent values.
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let (h, l) = (0.7f32, 0.55f32);
        let mut prev = f32::NEG_INFINITY;
        for i in 0..5000 {
            let c = i as f32 * 0.0002;
            let out = m.mapped_chroma(c, h, l, 2.0, opts);
            assert!(
                out > prev,
                "chroma map not increasing at c={c}: {prev} -> {out}"
            );
            prev = out;
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
