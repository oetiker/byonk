//! Tonal operations. All take tone-domain pixels (see the crate docs) and
//! mutate in place. Operations that model light convert to linear internally.

use crate::color::{linear_to_srgb, srgb_to_linear};

/// Exposure in stops. Physically a multiplication of light, so it happens in
/// linear space regardless of the buffer's domain.
pub fn apply_exposure(pixels: &mut [f32], ev: f32) {
    if ev == 0.0 {
        return;
    }
    let gain = 2.0f32.powf(ev);
    for v in pixels.iter_mut() {
        *v = linear_to_srgb(srgb_to_linear(*v) * gain);
    }
}

/// White balance. A crude but predictable channel-gain model rather than a
/// full chromatic adaptation transform: the output is six colours, so the
/// extra fidelity would not survive dithering. Physical, so it runs in
/// linear light.
///
/// `temperature` and `tint` are -100..=100. Positive temperature is warmer
/// (more red, less blue); positive tint is greener.
pub fn apply_white_balance(pixels: &mut [f32], temperature: f32, tint: f32) {
    if temperature == 0.0 && tint == 0.0 {
        return;
    }
    // 100 units moves a channel by 30%. Chosen so a full-scale slider is a
    // strong but not destructive shift.
    let t = temperature / 100.0 * 0.30;
    let g = tint / 100.0 * 0.30;
    let gains = [1.0 + t, 1.0 + g, 1.0 - t];

    for px in pixels.chunks_exact_mut(3) {
        for (c, gain) in px.iter_mut().zip(gains.iter()) {
            *c = linear_to_srgb(srgb_to_linear(*c) * gain);
        }
    }
}

/// The 0.5th and 99.5th percentiles of tone-domain luminance.
///
/// Percentiles rather than min/max: a single hot pixel or a speck of sensor
/// noise would otherwise define the whole range and make `auto_levels` a
/// no-op on exactly the images that need it most.
pub fn measure_endpoints(pixels: &[f32]) -> (f32, f32) {
    let mut lums: Vec<f32> = pixels
        .chunks_exact(3)
        .map(|px| crate::color::luminance(px[0], px[1], px[2]))
        .collect();
    if lums.is_empty() {
        return (0.0, 1.0);
    }
    lums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = |q: f32| -> usize { (((lums.len() - 1) as f32) * q).round() as usize };
    (lums[idx(0.005)], lums[idx(0.995)])
}

/// Linearly remap the tone domain from `from` to `to`, clamped to 0..=1.
///
/// Applied per channel rather than to luminance so the remap cannot shift hue.
/// A degenerate `from` range (a flat image) is left untouched rather than
/// producing infinities.
pub fn apply_endpoints(pixels: &mut [f32], from: (f32, f32), to: (f32, f32)) {
    let (src_lo, src_hi) = from;
    let (dst_lo, dst_hi) = to;
    let span = src_hi - src_lo;
    if span.abs() < 1e-6 {
        return;
    }
    let scale = (dst_hi - dst_lo) / span;
    for v in pixels.iter_mut() {
        *v = (dst_lo + (*v - src_lo) * scale).clamp(0.0, 1.0);
    }
}

/// Highlight recovery and shadow lift.
///
/// Each acts through a weighting mask so the adjustment is confined to its
/// end of the range: the shadow mask is `(1 - t)^2` and the highlight mask
/// `t^2`, where `t` is the pixel's tone-domain luminance. Squaring is what
/// keeps the opposite end still — a linear mask would visibly move it.
///
/// Both sliders are -100..=100.
pub fn apply_highlights_shadows(pixels: &mut [f32], highlights: f32, shadows: f32) {
    if highlights == 0.0 && shadows == 0.0 {
        return;
    }
    // 100 units moves a fully-masked pixel by 0.35 in the tone domain.
    let h = highlights / 100.0 * 0.35;
    let s = shadows / 100.0 * 0.35;

    for px in pixels.chunks_exact_mut(3) {
        let t = crate::color::luminance(px[0], px[1], px[2]).clamp(0.0, 1.0);
        let shadow_mask = (1.0 - t) * (1.0 - t);
        let highlight_mask = t * t;
        let delta = s * shadow_mask + h * highlight_mask;
        for c in px.iter_mut() {
            *c = (*c + delta).clamp(0.0, 1.0);
        }
    }
}

/// S-curve contrast about mid-grey, in the TONE domain.
///
/// `amount` is -100..=100. Positive uses a smoothstep-weighted blend toward a
/// steeper slope; negative blends toward the identity flattened about 0.5.
/// The construction guarantees two properties the tests pin: 0.5 is a fixed
/// point, and the mapping is monotonic for every amount in range.
pub fn apply_contrast(pixels: &mut [f32], amount: f32) {
    if amount == 0.0 {
        return;
    }
    let k = amount / 100.0;
    for v in pixels.iter_mut() {
        let x = v.clamp(0.0, 1.0);
        let curved = if k > 0.0 {
            // Smoothstep is the canonical monotonic S about 0.5.
            let s = x * x * (3.0 - 2.0 * x);
            x + (s - x) * k
        } else {
            // Blend toward a flat mid-grey; never past it, so no inversion.
            x + (0.5 - x) * (-k) * 0.5
        };
        *v = curved.clamp(0.0, 1.0);
    }
}

/// Piecewise-linear point tone curve in the tone domain.
///
/// Requires at least two points, strictly increasing in the input coordinate.
/// Inputs below the first point or above the last are clamped to that point's
/// output, so a curve that does not start at 0 or end at 1 still behaves
/// predictably.
pub fn apply_curve(pixels: &mut [f32], points: &[(f32, f32)]) -> Result<(), crate::PhotoError> {
    if points.len() < 2 {
        return Err(crate::PhotoError::BadCurve(
            "a curve needs at least two points",
        ));
    }
    for w in points.windows(2) {
        if w[1].0 <= w[0].0 {
            return Err(crate::PhotoError::BadCurve(
                "points must be strictly increasing in the input coordinate",
            ));
        }
    }

    for v in pixels.iter_mut() {
        let x = v.clamp(0.0, 1.0);
        *v = if x <= points[0].0 {
            points[0].1
        } else if x >= points[points.len() - 1].0 {
            points[points.len() - 1].1
        } else {
            let i = points
                .windows(2)
                .position(|w| x >= w[0].0 && x <= w[1].0)
                .unwrap_or(0);
            let (x0, y0) = points[i];
            let (x1, y1) = points[i + 1];
            let f = (x - x0) / (x1 - x0);
            y0 + (y1 - y0) * f
        }
        .clamp(0.0, 1.0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::srgb_to_linear;
    use crate::tests::{assert_close, assert_close_tol};

    /// Build a linear 0..1 greyscale ramp of `n` pixels.
    fn ramp(n: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(n * 3);
        for i in 0..n {
            let t = i as f32 / (n - 1) as f32;
            v.extend_from_slice(&[t, t, t]);
        }
        v
    }

    fn mean_of(pixels: &[f32], lo: usize, hi: usize) -> f32 {
        let slice = &pixels[lo * 3..hi * 3];
        slice.iter().sum::<f32>() / slice.len() as f32
    }

    #[test]
    fn shadows_lift_the_low_quartile_and_leave_the_top_decile_alone() {
        // The defining property of a shadow recovery: it must be local to the
        // shadows. A naive brightness add would move the top decile too.
        let before = ramp(100);
        let mut after = before.clone();
        apply_highlights_shadows(&mut after, 0.0, 50.0);

        let q_before = mean_of(&before, 0, 25);
        let q_after = mean_of(&after, 0, 25);
        assert!(
            q_after > q_before + 0.02,
            "shadows must lift: {q_before} -> {q_after}"
        );

        let t_before = mean_of(&before, 90, 100);
        let t_after = mean_of(&after, 90, 100);
        assert_close_tol(
            t_after,
            t_before,
            0.02,
            "top decile must stay put under shadow lift",
        );
    }

    #[test]
    fn highlights_recover_the_top_and_leave_the_bottom_decile_alone() {
        let before = ramp(100);
        let mut after = before.clone();
        apply_highlights_shadows(&mut after, -50.0, 0.0);

        let t_before = mean_of(&before, 75, 100);
        let t_after = mean_of(&after, 75, 100);
        assert!(
            t_after < t_before - 0.02,
            "highlights must pull down: {t_before} -> {t_after}"
        );

        let b_before = mean_of(&before, 0, 10);
        let b_after = mean_of(&after, 0, 10);
        assert_close_tol(
            b_after,
            b_before,
            0.02,
            "bottom decile must stay put under highlight recovery",
        );
    }

    #[test]
    fn highlights_and_shadows_of_zero_are_a_no_op() {
        let before = ramp(50);
        let mut after = before.clone();
        apply_highlights_shadows(&mut after, 0.0, 0.0);
        for (i, (a, b)) in after.iter().zip(before.iter()).enumerate() {
            assert_close_tol(
                *a,
                *b,
                1e-5,
                &format!("0/0 highlights/shadows no-op, channel {i}"),
            );
        }
    }

    #[test]
    fn contrast_pivots_about_mid_grey_in_the_tone_domain() {
        // This is the assertion that catches an S-curve applied in linear
        // light: there, tone-domain 0.5 would NOT be the fixed point.
        let mut p = vec![0.5f32; 3];
        apply_contrast(&mut p, 60.0);
        assert_close(p[0], 0.5, "mid grey must be the fixed point");
    }

    #[test]
    fn positive_contrast_pushes_away_from_mid_grey() {
        let mut p = vec![0.25f32, 0.25, 0.25, 0.75, 0.75, 0.75];
        apply_contrast(&mut p, 50.0);
        assert!(p[0] < 0.25, "quarter tone must darken, got {}", p[0]);
        assert!(
            p[3] > 0.75,
            "three-quarter tone must brighten, got {}",
            p[3]
        );
    }

    #[test]
    fn negative_contrast_pulls_toward_mid_grey() {
        let mut p = vec![0.25f32, 0.25, 0.25];
        apply_contrast(&mut p, -50.0);
        assert!(
            p[0] > 0.25,
            "flattening must raise the quarter tone, got {}",
            p[0]
        );
    }

    #[test]
    fn contrast_is_monotonic() {
        // A tone operation that reorders brightness is broken, however
        // pleasing any single sample looks.
        let mut p = ramp(64);
        apply_contrast(&mut p, 80.0);
        for w in p.chunks_exact(3).collect::<Vec<_>>().windows(2) {
            assert!(
                w[1][0] >= w[0][0] - 1e-6,
                "ordering inverted: {} then {}",
                w[0][0],
                w[1][0]
            );
        }
    }

    #[test]
    fn curve_interpolates_between_its_points() {
        let mut p = vec![0.5f32; 3];
        apply_curve(&mut p, &[(0.0, 0.0), (0.5, 0.8), (1.0, 1.0)]).unwrap();
        assert_close(p[0], 0.8, "curve midpoint interpolation");
    }

    #[test]
    fn curve_endpoints_are_honoured() {
        let mut p = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        apply_curve(&mut p, &[(0.0, 0.1), (1.0, 0.9)]).unwrap();
        assert_close(p[0], 0.1, "curve low endpoint");
        assert_close(p[3], 0.9, "curve high endpoint");
    }

    #[test]
    fn curve_rejects_unsorted_or_too_short_input() {
        let mut p = vec![0.5f32; 3];
        assert!(
            apply_curve(&mut p, &[(0.5, 0.5)]).is_err(),
            "one point is not a curve"
        );
        assert!(
            apply_curve(&mut p, &[(1.0, 1.0), (0.0, 0.0)]).is_err(),
            "unsorted input must be rejected, not silently reordered"
        );
    }

    #[test]
    fn curve_interpolates_at_a_genuinely_interior_point() {
        // x=0.5 is itself a control point in the other interpolation test,
        // which makes f collapse to exactly 1.0 and hides shape bugs like
        // `f*f` or swapped y0/y1. Query an x that is not a control point,
        // and use a non-identity curve: on the line (0,0)-(1,1), an input
        // of 0.25 would return 0.25 under a wholly no-op `apply_curve` too,
        // so identity can't distinguish "interpolated correctly" from "did
        // nothing". (0,0)-(1,0.5) at x=0.25 expects 0.125 — a no-op would
        // return 0.25 (wrong), f*f would give y=0.5*0.0625=0.03125 (wrong),
        // and swapped y0/y1 would give 0.5 + (0-0.5)*0.25 = 0.375 (wrong).
        let mut p = vec![0.25f32; 3];
        apply_curve(&mut p, &[(0.0, 0.0), (1.0, 0.5)]).unwrap();
        assert_close(p[0], 0.125, "interior fractional interpolation");
    }

    #[test]
    fn curve_clamps_input_outside_its_own_domain() {
        // The existing endpoint test only checks x equal to the first/last
        // control point, which is boundary-inclusive, not out-of-range.
        // Use a curve whose domain is narrower than [0,1] and probe both
        // sides of it.
        let mut p = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        apply_curve(&mut p, &[(0.2, 0.3), (0.8, 0.9)]).unwrap();
        assert_close(
            p[0],
            0.3,
            "input below the curve's domain clamps to the first point",
        );
        assert_close(
            p[3],
            0.9,
            "input above the curve's domain clamps to the last point",
        );
    }

    #[test]
    fn curve_rejects_duplicate_x_points() {
        // Same code path as `curve_rejects_unsorted_or_too_short_input`
        // (w[1].0 <= w[0].0), but exact duplicates are a distinct case from
        // strict reversal and deserve an explicit assertion.
        let mut p = vec![0.5f32; 3];
        assert!(
            apply_curve(&mut p, &[(0.5, 0.2), (0.5, 0.8)]).is_err(),
            "duplicate x coordinates must be rejected"
        );
    }

    #[test]
    fn exposure_of_one_ev_doubles_linear_light() {
        // The defining property. Start from a tone-domain value, expect the
        // LINEAR value to double.
        let start_tone = 0.25f32;
        let mut pixels = vec![start_tone; 3];
        apply_exposure(&mut pixels, 1.0);
        let got_linear = srgb_to_linear(pixels[0]);
        let want_linear = srgb_to_linear(start_tone) * 2.0;
        assert_close(got_linear, want_linear, "1EV doubles linear light");
    }

    #[test]
    fn exposure_of_zero_is_a_no_op() {
        let mut pixels = vec![0.1f32, 0.5, 0.9];
        let before = pixels.clone();
        apply_exposure(&mut pixels, 0.0);
        for (i, (a, b)) in pixels.iter().zip(before.iter()).enumerate() {
            // Tighter than the shared 1e-4: a true no-op should be near bit-
            // exact, per Plan B's "unless stated" carve-out.
            assert_close_tol(*a, *b, 1e-5, &format!("0EV no-op, channel {i}"));
        }
    }

    #[test]
    fn exposure_clamps_at_white_not_beyond() {
        let mut pixels = vec![0.9f32; 3];
        apply_exposure(&mut pixels, 3.0);
        for v in &pixels {
            assert!(*v <= 1.0 + 1e-6, "value escaped the range: {v}");
            assert_close(*v, 1.0, "+3EV push from 0.9 must reach white");
        }
    }

    #[test]
    fn negative_exposure_darkens_monotonically() {
        let mut a = vec![0.5f32; 3];
        let mut b = vec![0.5f32; 3];
        apply_exposure(&mut a, -1.0);
        apply_exposure(&mut b, -2.0);
        assert!(b[0] < a[0] && a[0] < 0.5);
    }

    #[test]
    fn white_balance_warms_by_raising_red_and_lowering_blue() {
        let mut p = vec![0.5f32, 0.5, 0.5];
        apply_white_balance(&mut p, 50.0, 0.0);
        assert!(p[0] > 0.5, "red must rise, got {}", p[0]);
        assert!(p[2] < 0.5, "blue must fall, got {}", p[2]);
        assert_close_tol(
            p[1],
            0.5,
            1e-4,
            "green must not move with temperature alone",
        );
    }

    #[test]
    fn white_balance_of_zero_is_a_no_op() {
        let mut p = vec![0.2f32, 0.5, 0.8];
        let before = p.clone();
        apply_white_balance(&mut p, 0.0, 0.0);
        for (i, (a, b)) in p.iter().zip(before.iter()).enumerate() {
            assert_close_tol(
                *a,
                *b,
                1e-5,
                &format!("0/0 white balance no-op, channel {i}"),
            );
        }
    }

    #[test]
    fn tint_moves_green_against_the_others() {
        let mut p = vec![0.5f32, 0.5, 0.5];
        apply_white_balance(&mut p, 0.0, 50.0);
        assert!(p[1] > 0.5, "positive tint must raise green, got {}", p[1]);
    }

    #[test]
    fn measure_endpoints_ignores_outliers() {
        // 1000 pixels at 0.4-0.6, two extreme outliers. The percentile
        // measurement must not be dragged to 0.0 and 1.0 by two pixels.
        let mut pixels: Vec<f32> = Vec::new();
        for i in 0..1000 {
            let v = 0.4 + 0.2 * (i as f32 / 999.0);
            pixels.extend_from_slice(&[v, v, v]);
        }
        pixels.extend_from_slice(&[0.0, 0.0, 0.0]);
        pixels.extend_from_slice(&[1.0, 1.0, 1.0]);

        let (lo, hi) = measure_endpoints(&pixels);
        assert!(
            lo > 0.35 && lo < 0.45,
            "low endpoint dragged by outlier: {lo}"
        );
        assert!(
            hi > 0.55 && hi < 0.65,
            "high endpoint dragged by outlier: {hi}"
        );
    }

    #[test]
    fn apply_endpoints_stretches_to_the_target_range() {
        let mut p = vec![0.4f32, 0.4, 0.4, 0.6, 0.6, 0.6];
        apply_endpoints(&mut p, (0.4, 0.6), (0.0, 1.0));
        assert_close_tol(p[0], 0.0, 1e-4, "low end must land at 0.0");
        assert_close_tol(p[3], 1.0, 1e-4, "high end must land at 1.0");
    }

    #[test]
    fn apply_endpoints_can_compress_into_a_narrower_range() {
        // This is what palette_aware does: refuse to spend range the panel
        // cannot show.
        let mut p = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        apply_endpoints(&mut p, (0.0, 1.0), (0.05, 0.92));
        assert_close_tol(p[0], 0.05, 1e-4, "black must land at the panel's black");
        assert_close_tol(p[3], 0.92, 1e-4, "white must land at the panel's white");
    }

    #[test]
    fn apply_endpoints_with_a_degenerate_source_does_not_divide_by_zero() {
        let mut p = vec![0.5f32; 6];
        apply_endpoints(&mut p, (0.5, 0.5), (0.0, 1.0));
        for v in &p {
            assert!(v.is_finite(), "produced a non-finite value: {v}");
        }
    }
}
