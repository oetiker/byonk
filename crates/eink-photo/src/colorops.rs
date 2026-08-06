//! Colour operations, all in the tone domain.

use crate::color::luminance;

/// Scale every pixel's distance from its own luminance by the same factor.
/// `amount` is -100..=100; -100 is fully grey, +100 doubles the distance.
pub fn apply_saturation(pixels: &mut [f32], amount: f32) {
    if amount == 0.0 {
        return;
    }
    let k = 1.0 + amount / 100.0;
    for px in pixels.chunks_exact_mut(3) {
        let l = luminance(px[0], px[1], px[2]);
        for c in px.iter_mut() {
            *c = (l + (*c - l) * k).clamp(0.0, 1.0);
        }
    }
}

/// Saturation weighted toward the already-dull pixels.
///
/// The weight is `1 - current_saturation`, so a grey pixel gets the full
/// adjustment and an already-vivid one gets almost none. On a six-colour
/// panel this is the difference between muted colours reaching a chromatic
/// palette entry and the whole image dithering into greys.
pub fn apply_vibrance(pixels: &mut [f32], amount: f32) {
    if amount == 0.0 {
        return;
    }
    let base = amount / 100.0;
    for px in pixels.chunks_exact_mut(3) {
        let l = luminance(px[0], px[1], px[2]);
        // Current saturation as a 0..1 distance from grey.
        let current =
            (((px[0] - l).abs() + (px[1] - l).abs() + (px[2] - l).abs()) / 3.0).clamp(0.0, 1.0);
        let k = 1.0 + base * (1.0 - current);
        for c in px.iter_mut() {
            *c = (l + (*c - l) * k).clamp(0.0, 1.0);
        }
    }
}

/// Flatten to Rec. 709 luminance.
pub fn apply_grayscale(pixels: &mut [f32]) {
    for px in pixels.chunks_exact_mut(3) {
        let l = luminance(px[0], px[1], px[2]);
        px[0] = l;
        px[1] = l;
        px[2] = l;
    }
}

/// Tone-domain inversion.
pub fn apply_invert(pixels: &mut [f32]) {
    for v in pixels.iter_mut() {
        *v = (1.0 - *v).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{assert_close, assert_close_tol};

    /// Distance from grey, the working definition of saturation here.
    fn sat(px: &[f32]) -> f32 {
        let l = crate::color::luminance(px[0], px[1], px[2]);
        ((px[0] - l).abs() + (px[1] - l).abs() + (px[2] - l).abs()) / 3.0
    }

    #[test]
    fn saturation_scales_every_pixel_equally() {
        // Two pixels with different chroma (dull, muted vs. vivid orange).
        // A no-op would leave both ratios at 1.0, which trivially satisfies
        // "equal ratios" — this test's job is not to catch a no-op (that's
        // covered by `full_negative_saturation_is_grey`, which does fail
        // against a no-op `apply_saturation`), but to fail if the scale
        // factor is *not* uniform (e.g. if it were secretly weighted by
        // current saturation, as vibrance is). That mutant is why the
        // tolerance is 0.005 and not something looser: substituting
        // `apply_vibrance`'s body (current-weighted k) into
        // `apply_saturation` at amount=15 gives dull_ratio=1.1446 vs
        // vivid_ratio=1.1072, |Δ|=0.0375 — comfortably outside 0.005. The
        // real implementation's two ratios are identical up to float
        // rounding (|Δ|~1e-6), comfortably inside it.
        //
        // Deviation from the brief: the brief used amount=50.0 for these
        // pixels, but at k=1.5 the vivid pixel's blue channel goes negative
        // and clamps (0.9/0.5/0.1 -> R and B both hit a rail), which breaks
        // uniformity for reasons that have nothing to do with the
        // saturation formula and makes the brief's own reference
        // implementation fail this exact assertion (verified: ratios
        // 1.500 vs 1.259, |Δ|=0.241, outside even the brief's own 0.05
        // tolerance). amount=15.0 keeps both pixels comfortably inside
        // 0..=1 (closest channel to a rail ends at 0.0316, not 0), so the
        // ratio-uniformity claim is tested cleanly instead of being an
        // artefact of clamping.
        let mut dull = vec![0.55f32, 0.5, 0.45];
        let mut vivid = vec![0.9f32, 0.5, 0.1];
        let dull_before = sat(&dull);
        let vivid_before = sat(&vivid);

        apply_saturation(&mut dull, 15.0);
        apply_saturation(&mut vivid, 15.0);

        let dull_ratio = sat(&dull) / dull_before;
        let vivid_ratio = sat(&vivid) / vivid_before;
        assert!(
            (dull_ratio - vivid_ratio).abs() < 0.005,
            "global saturation must be uniform: {dull_ratio} vs {vivid_ratio}"
        );
    }

    #[test]
    fn vibrance_favours_the_less_saturated_pixel() {
        // This is the property that distinguishes vibrance from saturation,
        // and the reason it matters on a six-colour panel: dull colours are
        // the ones that never reach a chromatic palette entry. Both pixels
        // have real, asymmetric chroma (neither is grey), so a no-op
        // `apply_vibrance` would leave both ratios at 1.0 and fail the
        // margin assertion below.
        //
        // Deviation from the brief: the brief used amount=50.0 and margin
        // 1.2. At amount=50.0 the vivid pixel clamps on both rails (as in
        // the test above); a plain-saturation "fake" vibrance stand-in
        // (uniform k=1.5, ignoring `current`) gets the *same* clamping and
        // scores a ratio-of-ratios of 1.1911 there — which does not even
        // clear the brief's own 1.2x margin (misses by 0.9%), but more
        // importantly *outscores* the real weighted implementation, which
        // only reaches 1.1794 at that amount. In other words the brief's
        // signal at amount=50 is essentially all clamping asymmetry, and
        // vibrance's current-weighting actually *reduces* it there — the
        // brief's test discriminates backwards, not just by too small a
        // margin. amount=25.0 is chosen so the real implementation's
        // pixels stay unclamped (vivid's closest channel to a rail is
        // 0.0185, not 0), while the plain-saturation stand-in — which
        // uses a uniform k=1.25 instead of the weighted, smaller k the
        // real vivid pixel gets — still clamps its vivid pixel's blue
        // channel (raw value -0.0140) and lands at ratio-of-ratios 1.0123.
        // The real implementation reaches 1.0530. Both are measured
        // values, not the "1.25 == 1.25 exactly" symmetric case a naive
        // reading might expect: the mutant clamps, the real one doesn't.
        // 1.0123 fails the 1.03x margin, 1.0530 clears it with 2.2%
        // headroom — well outside f32's ~1e-7 rounding error, so the
        // tighter margin is not float-fragile.
        let mut dull = vec![0.55f32, 0.5, 0.45];
        let mut vivid = vec![0.9f32, 0.5, 0.1];
        let dull_before = sat(&dull);
        let vivid_before = sat(&vivid);

        apply_vibrance(&mut dull, 25.0);
        apply_vibrance(&mut vivid, 25.0);

        let dull_ratio = sat(&dull) / dull_before;
        let vivid_ratio = sat(&vivid) / vivid_before;
        assert!(
            dull_ratio > vivid_ratio * 1.03,
            "vibrance must favour the dull pixel: {dull_ratio} vs {vivid_ratio}"
        );
    }

    #[test]
    fn vibrance_and_saturation_of_zero_are_no_ops() {
        // A pixel with real chroma: if amount=0 were accidentally treated
        // as "apply with k=1" instead of an early return, floating-point
        // rounding in the luminance/recompose round trip would still show
        // up here (chroma is non-zero, so there's something to perturb).
        let before = vec![0.9f32, 0.5, 0.1];
        let mut a = before.clone();
        let mut b = before.clone();
        apply_vibrance(&mut a, 0.0);
        apply_saturation(&mut b, 0.0);
        for i in 0..3 {
            assert_close(a[i], before[i], &format!("vibrance(0) channel {i}"));
            assert_close(b[i], before[i], &format!("saturation(0) channel {i}"));
        }
    }

    #[test]
    fn full_negative_saturation_is_grey() {
        // Vivid, asymmetric input (not already grey) — a no-op would leave
        // channels 0.9/0.5/0.1 apart, failing the equality checks below.
        let mut p = vec![0.9f32, 0.5, 0.1];
        apply_saturation(&mut p, -100.0);
        assert_close_tol(p[0], p[1], 1e-4, "R vs G after -100 saturation");
        assert_close_tol(p[1], p[2], 1e-4, "G vs B after -100 saturation");
    }

    #[test]
    fn saturation_preserves_luminance() {
        // Real chroma again; a no-op trivially preserves luminance too, but
        // this test's purpose is to catch an implementation that scales
        // luminance itself (e.g. multiplying the whole pixel by k instead
        // of the distance-from-grey), which saturation_scales_every_pixel_
        // equally would not necessarily catch.
        //
        // Deviation from the brief: the brief used amount=60.0, which
        // clamps both rails for this pixel (raw values [1.1063, 0.4881,
        // -0.1738]), so roughly half the measured 0.0101 drift against a
        // 0.02 tolerance was clamp error rather than the property under
        // test — the third instance of the same problem flagged in
        // `saturation_scales_every_pixel_equally` and
        // `vibrance_favours_the_less_saturated_pixel` above. amount=15.0
        // keeps both channels inside 0..=1 (closest to a rail: 0.0316),
        // so luminance is preserved exactly up to float rounding and the
        // default `assert_close` tolerance (1e-4) is the real bound, not
        // a budget shared with clamping.
        let p0 = vec![0.9f32, 0.5, 0.1];
        let mut p = p0.clone();
        let before = crate::color::luminance(p0[0], p0[1], p0[2]);
        apply_saturation(&mut p, 15.0);
        let after = crate::color::luminance(p[0], p[1], p[2]);
        assert_close(after, before, "luminance drifted");
    }

    #[test]
    fn grayscale_flattens_channels_to_luminance() {
        // Asymmetric input: a no-op leaves 0.9/0.5/0.1, all far from the
        // computed luminance, so each per-channel check below would fail.
        let mut p = vec![0.9f32, 0.5, 0.1];
        let l = crate::color::luminance(0.9, 0.5, 0.1);
        apply_grayscale(&mut p);
        for (i, c) in p.iter().enumerate() {
            assert_close_tol(*c, l, 1e-5, &format!("grayscale channel {i}"));
        }
    }

    #[test]
    fn invert_is_its_own_inverse() {
        // A no-op invert would actually *pass* this — a no-op composed
        // with itself is still a no-op, so it trivially round-trips. This
        // test's job is a different one: catching a broken-but-not-no-op
        // invert (e.g. one that clamps asymmetrically or forgets a
        // channel), which would show up as a mismatch after two passes.
        // `invert_actually_inverts` below is what closes the no-op gap.
        let before = vec![0.9f32, 0.5, 0.1];
        let mut p = before.clone();
        apply_invert(&mut p);
        apply_invert(&mut p);
        for (i, (a, b)) in p.iter().zip(before.iter()).enumerate() {
            assert_close(*a, *b, &format!("double-invert channel {i}"));
        }
    }

    #[test]
    fn invert_actually_inverts() {
        // Guards against a no-op or identity `apply_invert`: the test above
        // (double-invert) passes for `apply_invert = |_| {}` too, since a
        // no-op composed with itself is still a no-op. This checks the
        // single-pass result directly.
        let mut p = vec![0.9f32, 0.5, 0.1];
        apply_invert(&mut p);
        assert_close(p[0], 0.1, "invert channel 0");
        assert_close(p[1], 0.5, "invert channel 1");
        assert_close(p[2], 0.9, "invert channel 2");
    }

    #[test]
    fn grayscale_channel_weights_are_not_swapped() {
        // `luminance_of_white_is_one` (color.rs) only proves the three
        // weights sum to 1; it would not catch red and blue weights being
        // swapped. Grayscale depends on the *individual* weights being
        // right (Rec. 709: R=0.2126, G=0.7152, B=0.0722), so pin it here
        // with a pure-red and a pure-blue pixel — under a red/blue swap
        // these two results would trade places instead of matching the
        // expected values.
        let mut red = vec![1.0f32, 0.0, 0.0];
        let mut blue = vec![0.0f32, 0.0, 1.0];
        apply_grayscale(&mut red);
        apply_grayscale(&mut blue);
        assert_close_tol(red[0], 0.2126, 1e-4, "grayscale(red)");
        assert_close_tol(blue[0], 0.0722, 1e-4, "grayscale(blue)");
    }
}
