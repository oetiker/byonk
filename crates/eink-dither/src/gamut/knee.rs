//! The chroma compression curve.
//!
//! Below `knee * c_max` the input passes through untouched, so low-chroma
//! content — the bulk of most images — is never desaturated. Above it, a
//! power shoulder maps the whole remaining half-line into
//! `[knee * c_max, c_max)`:
//!
//! ```text
//! C <= k*Cmax :  C' = C
//! C >  k*Cmax :  C' = k*Cmax + (1-k)*Cmax * t/(1+t^p)^(1/p),
//!                t  = (C - k*Cmax) / ((R-k)*Cmax)
//! ```
//!
//! `R` — the content adaptation factor, always `>= 1` — appears **only in the
//! input span of the tail**. That placement is the whole point: adaptation
//! decides how hard the out-of-gamut tail is squeezed, and cannot reach the
//! identity region below the knee. At `R = 1` the two spans coincide and this
//! is the unadapted curve exactly.
//!
//! An earlier revision instead divided `C` by `R` before consulting the knee.
//! That applied adaptation to *every* pixel unconditionally, so on saturated
//! content — where `R` pins at its cap — the palette's own inks, which sit
//! exactly on the boundary and need no compression at all, came out with 40%
//! of their chroma. It also made adaptation contagious: one vivid element
//! desaturated everything measured alongside it. Both are ruled out by the
//! form above, and both are guarded by tests in `mapper.rs`.
//!
//! This is the `powerP` curve of the ACES 1.3 Reference Gamut Compression,
//! at its default exponent `p = 1.2`. The exponent controls how sharply the
//! shoulder rolls off: `p = 1` is the classic Reinhard form, and `p → ∞`
//! degenerates to a hard clip.
//!
//! The shoulder is a power curve rather than an exponential for a measured
//! reason. `1 - exp(-t)` reaches 1.0 in `f32` at `t ≈ 10.2`, and the reachable
//! input domain extends to `t ≈ 11.05` — so an exponential shoulder returns
//! *exactly* `c_max` for real pixels, silently becoming the clipping this
//! design rejects. The power form decays polynomially: at `p = 1.2` it stays
//! strictly below 1.0 out to `t ≈ 85.9`, roughly eight times beyond anything
//! reachable.
//!
//! **The reachable domain is measured, not assumed.** Sweeping every sRGB
//! colour with non-zero chroma against this crate's `CmaxTable` for the
//! six-ink palette, `rho = C / Cmax` peaks at **5.02** (median 0.91, p99.9
//! 4.23) — `Cmax` shrinks toward black and white, but so does the chroma any
//! sRGB colour can have there, so the ratio stays bounded. `t = (rho - k) /
//! (R - k)` is largest at `R = 1`, so the worst case is `k = 0.6`, `R = 1`,
//! `rho = 5.02`, giving `t = 11.05`; every larger `R` only shrinks it. The
//! monotonicity test therefore covers `t` to about 36, three times the
//! reachable maximum, rather than an arbitrary range.
//!
//! The curve is continuous at the knee and **strictly increasing everywhere**,
//! approaching `c_max` asymptotically without reaching it. That property is
//! the formal statement of the design's goal: two colours that differed before
//! still differ after. Nothing collapses onto a shared value — which is
//! exactly what a clipping approach (HPMINDE) would do, and why it was
//! rejected.
//!
//! Because the shoulder accepts any input however large, content beyond the
//! adaptation cap is compressed very hard but never clipped.

/// Exponent of the shoulder roll-off — the ACES 1.3 Reference Gamut
/// Compression default. Higher values protect near-boundary chroma but
/// crowd far-out-of-gamut values together; lower values do the reverse.
const SHOULDER_POWER: f32 = 1.2;

/// Compress `c` into `[0, c_max)`, leaving everything below the knee alone.
///
/// # Arguments
/// * `c` — input chroma, in the same units as `c_max`. **Not** pre-divided by
///   the adaptation factor; pass it raw and let `r` shape the tail.
/// * `c_max` — the reachable chroma limit at this hue and lightness
/// * `knee` — fraction of `c_max` at which compression begins, in `0..=1`
/// * `r` — the adaptation factor, `>= 1`. Sets how wide an input range the
///   tail spreads across the output shoulder; `r = 1` is the unadapted curve.
#[inline]
pub fn compress_chroma(c: f32, c_max: f32, knee: f32, r: f32) -> f32 {
    if c_max <= 0.0 {
        return 0.0;
    }
    let k = knee.clamp(0.0, 0.999);
    let threshold = k * c_max;
    if c <= threshold {
        return c;
    }
    // `r >= 1 > k`, so the input span is strictly positive.
    let r = r.max(1.0);
    let out_span = (1.0 - k) * c_max;
    let in_span = (r - k) * c_max;
    let t = (c - threshold) / in_span;
    let p = SHOULDER_POWER;
    threshold + out_span * (t / (1.0 + t.powf(p)).powf(1.0 / p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMAX: f32 = 0.20;
    const K: f32 = 0.6;
    /// The unadapted curve. Every property below must hold at every `R`, so
    /// where a test does not sweep `R` it pins this one.
    const R1: f32 = 1.0;

    /// Every `R` a caller can produce: `adaptation_factor` clamps to
    /// `[1, max_compression]` and the shipped cap is 2.5.
    const RS: [f32; 5] = [1.0, 1.25, 1.5, 2.0, 2.5];

    #[test]
    fn below_the_knee_is_identity_at_every_adaptation_factor() {
        for r in RS {
            for i in 0..=60 {
                let c = (i as f32 / 100.0) * CMAX;
                assert!(
                    (compress_chroma(c, CMAX, K, r) - c).abs() < 1e-6,
                    "c={c} must pass through untouched at R={r}"
                );
            }
        }
    }

    #[test]
    fn continuous_at_the_knee_at_every_adaptation_factor() {
        for r in RS {
            let below = compress_chroma(K * CMAX - 1e-5, CMAX, K, r);
            let above = compress_chroma(K * CMAX + 1e-5, CMAX, K, r);
            assert!(
                (above - below).abs() < 1e-4,
                "discontinuity at R={r}: {below} -> {above}"
            );
        }
    }

    #[test]
    fn strictly_increasing_across_the_reachable_range() {
        for r in RS {
            let mut prev = f32::NEG_INFINITY;
            for i in 0..20_000 {
                // Up to c = 3.0. `t = (rho - k)/(R - k)` is largest at R = 1,
                // where c = 3.0 is t = 36. Measured across every sRGB colour,
                // rho = C/Cmax peaks at 5.02, which is t = 11.05 at k = 0.6,
                // so this covers three times the reachable domain. Testing far
                // beyond it would only measure f32 resolution, not the curve.
                let c = i as f32 * 0.00015;
                let out = compress_chroma(c, CMAX, K, r);
                assert!(
                    out > prev,
                    "not strictly increasing at c={c}, R={r}: {prev} -> {out}"
                );
                prev = out;
            }
        }
    }

    #[test]
    fn asymptotic_to_cmax_and_never_reaches_it() {
        for r in RS {
            for c in [CMAX, 2.0 * CMAX, 10.0 * CMAX, 1000.0 * CMAX] {
                let out = compress_chroma(c, CMAX, K, r);
                assert!(
                    out < CMAX,
                    "c={c} at R={r} produced {out}, must stay under {CMAX}"
                );
            }
            assert!(
                compress_chroma(1000.0 * CMAX, CMAX, K, r) > 0.999 * CMAX,
                "extreme input should approach the bound at R={r}"
            );
        }
    }

    #[test]
    fn a_larger_adaptation_factor_squeezes_the_tail_harder() {
        // The one thing R is for. Above the knee, raising R must compress
        // more; below it, R does nothing (asserted separately above).
        let c = 1.5 * CMAX;
        let mut prev = f32::INFINITY;
        for r in RS {
            let out = compress_chroma(c, CMAX, K, r);
            assert!(
                out < prev,
                "R={r} did not compress further than the previous factor: {prev} -> {out}"
            );
            prev = out;
        }
    }

    #[test]
    fn an_adaptation_factor_below_one_is_treated_as_one() {
        // `adaptation_factor` never returns below 1.0, but the curve must not
        // invert if a caller passes 0, a negative, or NaN.
        let c = 1.5 * CMAX;
        let expected = compress_chroma(c, CMAX, K, R1);
        for r in [1.0f32, 0.5, 0.0, -3.0, f32::NAN] {
            let out = compress_chroma(c, CMAX, K, r);
            assert!(
                (out - expected).abs() < 1e-6,
                "R={r} should behave as R=1: {expected} vs {out}"
            );
        }
    }

    #[test]
    fn zero_cmax_yields_zero() {
        assert_eq!(compress_chroma(0.3, 0.0, K, R1), 0.0);
    }

    #[test]
    fn knee_of_zero_compresses_everything() {
        // k = 0 means the curve bends from the origin; still bounded and
        // strictly increasing.
        assert!(compress_chroma(0.01, CMAX, 0.0, R1) < 0.01);
        assert!(compress_chroma(0.02, CMAX, 0.0, R1) > compress_chroma(0.01, CMAX, 0.0, R1));
    }

    #[test]
    fn an_extreme_knee_keeps_the_input_span_positive() {
        // k is clamped to 0.999, so at R = 1 the input span is 0.001*Cmax —
        // the narrowest it can be. It must stay finite and bounded.
        let out = compress_chroma(2.0 * CMAX, CMAX, 1.0, R1);
        assert!(out.is_finite(), "degenerate knee produced {out}");
        assert!(
            out < CMAX,
            "degenerate knee produced {out}, must stay under {CMAX}"
        );
    }
}
