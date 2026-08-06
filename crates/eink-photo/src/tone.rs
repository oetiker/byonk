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
}
