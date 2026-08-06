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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::srgb_to_linear;
    use crate::tests::{assert_close, assert_close_tol};

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
