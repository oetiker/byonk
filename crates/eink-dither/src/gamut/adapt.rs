//! Deriving the compression factor from the content.
//!
//! For each pixel in an adaptation group, `rho = C / Cmax(h, L)` says how far
//! out of gamut it is, 1.0 being exactly on the boundary. `R` is a high
//! percentile of those values, not the maximum, so one stray neon pixel cannot
//! crush the whole image.
//!
//! Three guards, in increasing order of practical importance:
//!
//! - The **percentile** handles literal outliers completely. On an 800x480
//!   frame the discarded top 1% is 3,840 pixels.
//! - It weakens as the marked region shrinks — in a 50x50 photo the top 1% is
//!   25 pixels, and three bad ones are 12% of the discarded tail. Hence the
//!   **absolute floor** on the discard count.
//! - The **cap** bounds the damage from a small-but-not-tiny vivid region (a
//!   neon sign filling 2% of the frame sits above the percentile cut). It does
//!   not eliminate it.
//!
//! Content beyond the cap is deliberately **not** clipped: normalising by the
//! capped `R` leaves it above `Cmax` going into the knee, whose asymptotic
//! shoulder maps any input to just under the limit while staying strictly
//! increasing.

/// Fraction of the distribution kept below the cut.
pub const PERCENTILE: f32 = 0.99;
/// Minimum number of samples discarded from the top, whatever the region size.
pub const MIN_DISCARD: usize = 32;

/// Compute the adaptation factor `R` for one adaptation group.
///
/// `rhos` is reordered in place (select-nth). Returns `1.0` when the content
/// already fits, in which case the caller must skip mapping entirely rather
/// than needlessly desaturating.
pub fn adaptation_factor(rhos: &mut [f32], max_compression: f32) -> f32 {
    let n = rhos.len();
    if n == 0 {
        return 1.0;
    }

    let discard = MIN_DISCARD
        .max((n as f32 * (1.0 - PERCENTILE)).ceil() as usize)
        .min(n - 1);
    let idx = n - 1 - discard;

    // `total_cmp` is a genuine total order over every f32, including NaN, so
    // the selection cannot violate its comparator contract. It also makes the
    // degenerate cases coherent with the design: NaN and infinity sort above
    // every real value, so they are discarded by the same percentile guard
    // that discards any other outlier, and only reach `R` when more than
    // `1 - PERCENTILE` of the region is contaminated.
    //
    // Selection, not a sort: one order statistic is read, so this is O(n)
    // expected rather than O(n log n) over an adaptation group that can be
    // the whole frame.
    rhos.select_nth_unstable_by(idx, |a, b| a.total_cmp(b));
    let r = rhos[idx];

    if !r.is_finite() {
        return max_compression.max(1.0);
    }
    r.clamp(1.0, max_compression.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_gamut_content_returns_identity() {
        let mut rhos: Vec<f32> = (0..1000).map(|i| i as f32 / 2000.0).collect();
        assert_eq!(adaptation_factor(&mut rhos, 2.5), 1.0);
    }

    #[test]
    fn empty_input_returns_identity() {
        assert_eq!(adaptation_factor(&mut [], 2.5), 1.0);
    }

    #[test]
    fn a_handful_of_outliers_cannot_move_r() {
        // 10_000 pixels at rho = 1.0, plus 5 neon pixels at rho = 40.
        let mut rhos = vec![1.0f32; 10_000];
        rhos.extend_from_slice(&[40.0; 5]);
        let r = adaptation_factor(&mut rhos, 2.5);
        assert!((r - 1.0).abs() < 1e-6, "outliers hijacked R: {r}");
    }

    #[test]
    fn small_regions_still_discard_an_absolute_minimum() {
        // 100 pixels: 1% is one pixel, so a percentage-only rule would let
        // three bad pixels set R. The absolute floor must discard more.
        let mut rhos = vec![1.0f32; 100];
        for v in rhos.iter_mut().take(3) {
            *v = 50.0;
        }
        let r = adaptation_factor(&mut rhos, 2.5);
        assert!((r - 1.0).abs() < 1e-6, "small-region floor failed: {r}");
    }

    #[test]
    fn a_genuinely_vivid_image_sets_r() {
        let mut rhos = vec![1.8f32; 10_000];
        let r = adaptation_factor(&mut rhos, 2.5);
        assert!((r - 1.8).abs() < 0.05, "expected R near 1.8, got {r}");
    }

    #[test]
    fn r_is_capped_at_max_compression() {
        let mut rhos = vec![9.0f32; 10_000];
        assert_eq!(adaptation_factor(&mut rhos, 2.5), 2.5);
    }

    #[test]
    fn infinite_rho_from_a_zero_limit_is_handled() {
        let mut rhos = vec![f32::INFINITY; 10_000];
        assert_eq!(adaptation_factor(&mut rhos, 2.5), 2.5);
    }
}
