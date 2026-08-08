//! Precomputed chroma limit as a function of hue and lightness.
//!
//! Querying the hull per pixel is far too slow. Instead, when the palette
//! resolves, binary-search the largest in-hull chroma for each (hue,
//! lightness) bin and store the result. Render-time lookup is a bilinear
//! sample, with hue wrapping around the circle.
//!
//! The table is built once per palette, not per frame.

use super::hull::{Hull, HullShape};
use crate::{LinearRgb, Oklab, Oklch};

/// Hue bins around the full circle.
pub const HUE_BINS: usize = 128;
/// Lightness bins across 0..=1.
pub const LIGHTNESS_BINS: usize = 64;

/// Upper bound for the chroma search. Oklab chroma inside the sRGB cube peaks
/// around 0.33; 0.5 leaves generous headroom without wasting iterations.
const C_SEARCH_HI: f32 = 0.5;
/// Bisection steps — 24 resolves chroma to well under 1e-6.
const SEARCH_STEPS: usize = 24;

/// `Cmax(hue, lightness)` for one palette.
#[derive(Debug, Clone)]
pub struct CmaxTable {
    /// Row-major `[hue][lightness]`, length `HUE_BINS * LIGHTNESS_BINS`.
    data: Vec<f32>,
    l_min: f32,
    l_max: f32,
    achromatic: bool,
    unmappable: bool,
}

impl CmaxTable {
    /// Build the table by bisecting the hull boundary in each bin.
    ///
    /// Two degenerate cases are deliberately distinguished, because they call
    /// for opposite behaviour:
    ///
    /// - **Greyscale palette** (`HullShape::Line`) — no chroma is reachable, so
    ///   every limit is zero and marked content desaturates to grey. That is
    ///   the correct result on a four-level panel, not a bug.
    /// - **Unmappable hull** — coplanar, or a full volume whose grey axis lies
    ///   entirely outside it (`!Hull::is_mappable()`). Chroma compression has
    ///   no meaningful target here, so the mapper declines and leaves the
    ///   content untouched rather than crushing it onto a lightness the panel
    ///   cannot render.
    pub fn build(hull: &Hull) -> Self {
        let (l_min, l_max) = hull.lightness_range();
        let achromatic = hull.shape() == HullShape::Line;
        let unmappable = !achromatic && !hull.is_mappable();

        let mut data = vec![0.0f32; HUE_BINS * LIGHTNESS_BINS];
        if !achromatic && !unmappable {
            for hi in 0..HUE_BINS {
                let h = hue_of_bin(hi);
                for li in 0..LIGHTNESS_BINS {
                    let l = li as f32 / (LIGHTNESS_BINS - 1) as f32;
                    data[hi * LIGHTNESS_BINS + li] = max_chroma(hull, h, l);
                }
            }
        }

        Self {
            data,
            l_min,
            l_max,
            achromatic,
            unmappable,
        }
    }

    /// Sample the limit, bilinearly, wrapping hue and clamping lightness.
    pub fn sample(&self, h: f32, l: f32) -> f32 {
        if self.achromatic || self.unmappable {
            return 0.0;
        }

        let tau = std::f32::consts::TAU;
        // Map h into [0, 1) around the circle, matching `hue_of_bin`.
        let hn = ((h + std::f32::consts::PI).rem_euclid(tau)) / tau;
        let hf = hn * HUE_BINS as f32;
        let h0 = hf.floor() as usize % HUE_BINS;
        let h1 = (h0 + 1) % HUE_BINS;
        let ht = hf - hf.floor();

        let lf = l.clamp(0.0, 1.0) * (LIGHTNESS_BINS - 1) as f32;
        let l0 = lf.floor() as usize;
        let l1 = (l0 + 1).min(LIGHTNESS_BINS - 1);
        let lt = lf - lf.floor();

        let at = |hb: usize, lb: usize| self.data[hb * LIGHTNESS_BINS + lb];
        let a = at(h0, l0) * (1.0 - lt) + at(h0, l1) * lt;
        let b = at(h1, l0) * (1.0 - lt) + at(h1, l1) * lt;
        (a * (1.0 - ht) + b * ht).max(0.0)
    }

    /// The lightness range reachable on the achromatic axis.
    pub fn lightness_range(&self) -> (f32, f32) {
        (self.l_min, self.l_max)
    }

    /// True when the palette admits no chroma at all — a greyscale panel.
    /// Marked content desaturates to grey, which is the correct result.
    pub fn is_achromatic(&self) -> bool {
        self.achromatic
    }

    /// True when chroma compression has no meaningful target: a coplanar hull,
    /// or a volume whose grey axis lies entirely outside it. The mapper leaves
    /// such content untouched rather than guessing.
    pub fn is_unmappable(&self) -> bool {
        self.unmappable
    }
}

/// Bin centre hue, in radians, matching `sample`'s inverse mapping.
fn hue_of_bin(hi: usize) -> f32 {
    let tau = std::f32::consts::TAU;
    (hi as f32 / HUE_BINS as f32) * tau - std::f32::consts::PI
}

/// Largest chroma at this (hue, lightness) whose Oklch point is still inside
/// the hull. Zero when even the neutral at this lightness is unreachable.
fn max_chroma(hull: &Hull, h: f32, l: f32) -> f32 {
    // At pure black (l ≈ 0) and pure white (l ≈ 1), chroma is always zero.
    // These extreme points have numerical precision issues in color space
    // conversions, so handle them explicitly.
    if l <= 0.001 || l >= 0.999 {
        return 0.0;
    }

    let inside = |c: f32| hull.contains(LinearRgb::from(Oklab::from(Oklch { l, c, h })));

    if !inside(0.0) {
        return 0.0;
    }
    if inside(C_SEARCH_HI) {
        return C_SEARCH_HI;
    }
    let (mut lo, mut hi) = (0.0f32, C_SEARCH_HI);
    for _ in 0..SEARCH_STEPS {
        let mid = 0.5 * (lo + hi);
        if inside(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    // Apply a safety margin to avoid numerical precision issues at
    // the boundary. Bilinear sampling can overshoot slightly due to
    // convexity changes between bins.
    (lo * 0.95).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::{CmaxTable};
    use crate::gamut::hull::Hull;
    use crate::gamut::test_support::{four_grey, six_colour};
    use crate::{LinearRgb, Oklab, Oklch};

    #[test]
    fn sampled_chroma_is_inside_the_hull_everywhere() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        for hi in 0..64 {
            let h = -std::f32::consts::PI + (hi as f32 / 64.0) * std::f32::consts::TAU;
            for li in 1..64 {
                let l = li as f32 / 64.0;
                let c = table.sample(h, l);
                if c <= 0.0 {
                    continue;
                }
                // Sit just inside the reported limit; bilinear sampling can
                // overshoot the true boundary slightly between bins, so allow
                // a small margin.
                let probe = Oklch { l, c: c * 0.92, h };
                assert!(
                    hull.contains(LinearRgb::from(Oklab::from(probe))),
                    "sample(h={h:.3}, l={l:.3}) = {c:.4} is not reachable"
                );
            }
        }
    }

    #[test]
    fn chroma_limit_is_zero_at_the_lightness_extremes() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        assert!(table.sample(0.0, 0.0) < 0.02, "black admits no chroma");
        assert!(table.sample(0.0, 1.0) < 0.02, "white admits no chroma");
    }

    #[test]
    fn mid_lightness_admits_real_chroma() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        // Red sits near h = 0.5 rad in Oklab.
        let c = table.sample(0.5, 0.55);
        assert!(c > 0.05, "mid-lightness warm hue should reach chroma, got {c}");
    }

    #[test]
    fn hue_wraps_continuously() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        let a = table.sample(std::f32::consts::PI - 1e-4, 0.5);
        let b = table.sample(-std::f32::consts::PI + 1e-4, 0.5);
        assert!((a - b).abs() < 0.01, "hue must wrap: {a} vs {b}");
    }

    #[test]
    fn greyscale_palette_reports_achromatic_and_zero_chroma() {
        let table = CmaxTable::build(&Hull::from_palette(&four_grey()));
        assert!(table.is_achromatic());
        assert_eq!(table.sample(1.0, 0.5), 0.0);
    }
}
