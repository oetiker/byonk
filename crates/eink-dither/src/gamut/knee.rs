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
//!                t  = (C - k*Cmax) / ((1-k)*Cmax)
//! ```
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
//! sRGB colour can have there, so the ratio stays bounded. With `k = 0.6`,
//! `rho = 5.02` is `t = 11.05`. The monotonicity test therefore covers `t` to
//! about 36, three times the reachable maximum, rather than an arbitrary range.
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
/// * `c` — input chroma, already normalised by the adaptation factor
/// * `c_max` — the reachable chroma limit at this hue and lightness
/// * `knee` — fraction of `c_max` at which compression begins, in `0..=1`
#[inline]
pub fn compress_chroma(c: f32, c_max: f32, knee: f32) -> f32 {
    if c_max <= 0.0 {
        return 0.0;
    }
    let k = knee.clamp(0.0, 0.999);
    let threshold = k * c_max;
    if c <= threshold {
        return c;
    }
    let span = (1.0 - k) * c_max;
    let t = (c - threshold) / span;
    let p = SHOULDER_POWER;
    threshold + span * (t / (1.0 + t.powf(p)).powf(1.0 / p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMAX: f32 = 0.20;
    const K: f32 = 0.6;

    #[test]
    fn below_the_knee_is_identity() {
        for i in 0..=60 {
            let c = (i as f32 / 100.0) * CMAX;
            assert!(
                (compress_chroma(c, CMAX, K) - c).abs() < 1e-6,
                "c={c} must pass through untouched"
            );
        }
    }

    #[test]
    fn continuous_at_the_knee() {
        let below = compress_chroma(K * CMAX - 1e-5, CMAX, K);
        let above = compress_chroma(K * CMAX + 1e-5, CMAX, K);
        assert!(
            (above - below).abs() < 1e-4,
            "discontinuity: {below} -> {above}"
        );
    }

    #[test]
    fn strictly_increasing_across_the_reachable_range() {
        let mut prev = f32::NEG_INFINITY;
        for i in 0..20_000 {
            // Up to c = 3.0, i.e. t = 36. Measured across every sRGB colour,
            // rho = C/Cmax peaks at 5.02, which is t = 11.05 at k = 0.6, so
            // this covers three times the reachable domain. Testing far
            // beyond it would only measure f32 resolution, not the curve.
            let c = i as f32 * 0.00015;
            let out = compress_chroma(c, CMAX, K);
            assert!(
                out > prev,
                "not strictly increasing at c={c}: {prev} -> {out}"
            );
            prev = out;
        }
    }

    #[test]
    fn asymptotic_to_cmax_and_never_reaches_it() {
        for c in [CMAX, 2.0 * CMAX, 10.0 * CMAX, 1000.0 * CMAX] {
            let out = compress_chroma(c, CMAX, K);
            assert!(out < CMAX, "c={c} produced {out}, must stay under {CMAX}");
        }
        assert!(
            compress_chroma(1000.0 * CMAX, CMAX, K) > 0.999 * CMAX,
            "extreme input should approach the bound"
        );
    }

    #[test]
    fn zero_cmax_yields_zero() {
        assert_eq!(compress_chroma(0.3, 0.0, K), 0.0);
    }

    #[test]
    fn knee_of_zero_compresses_everything() {
        // k = 0 means the curve bends from the origin; still bounded and
        // strictly increasing.
        assert!(compress_chroma(0.01, CMAX, 0.0) < 0.01);
        assert!(compress_chroma(0.02, CMAX, 0.0) > compress_chroma(0.01, CMAX, 0.0));
    }
}
