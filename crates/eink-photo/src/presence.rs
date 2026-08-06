//! Local-contrast operations. Clarity and sharpening are the same unsharp
//! mask at radii an order of magnitude apart; authors think of them as
//! different controls, so they get different names.
//!
//! Both operate on tone-domain pixels (see the crate docs): the unsharp
//! mask amplifies differences between a pixel and a blurred version of
//! itself, and doing that in the perceptually-scaled tone domain is what
//! gives "clarity" and "sharpen" their expected visual effect. There is no
//! linear-light round trip here.

/// Three separable box-blur passes, which converges close enough to a
/// Gaussian for an unsharp mask and costs O(n) per pass with a running sum.
/// Edges are handled by clamping the sample coordinate, which is what keeps
/// a border from darkening.
pub fn box_blur(pixels: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 || width == 0 || height == 0 {
        return pixels.to_vec();
    }
    let mut buf = pixels.to_vec();
    let mut tmp = vec![0.0f32; pixels.len()];
    for _ in 0..3 {
        blur_horizontal(&buf, &mut tmp, width, height, radius);
        blur_vertical(&tmp, &mut buf, width, height, radius);
    }
    buf
}

fn blur_horizontal(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let mut sum = 0.0f32;
                let mut n = 0.0f32;
                for k in -(radius as isize)..=(radius as isize) {
                    let sx = (x as isize + k).clamp(0, width as isize - 1) as usize;
                    sum += src[(y * width + sx) * 3 + c];
                    n += 1.0;
                }
                dst[(y * width + x) * 3 + c] = sum / n;
            }
        }
    }
}

fn blur_vertical(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let mut sum = 0.0f32;
                let mut n = 0.0f32;
                for k in -(radius as isize)..=(radius as isize) {
                    let sy = (y as isize + k).clamp(0, height as isize - 1) as usize;
                    sum += src[(sy * width + x) * 3 + c];
                    n += 1.0;
                }
                dst[(y * width + x) * 3 + c] = sum / n;
            }
        }
    }
}

/// Unsharp mask with a radius scaled to the image, so "clarity 25" means the
/// same thing on an 800x480 preview and a 4000px source.
pub fn apply_clarity(pixels: &mut [f32], width: usize, height: usize, amount: f32) {
    if amount == 0.0 {
        return;
    }
    let radius = ((width.min(height) as f32 * 0.02).round() as usize).clamp(1, 40);
    unsharp(pixels, width, height, radius, amount / 100.0);
}

/// Output sharpening. `radius` is in pixels and is not scaled with the image:
/// this runs last, at output resolution, where a pixel is a pixel.
pub fn apply_sharpen(pixels: &mut [f32], width: usize, height: usize, amount: f32, radius: f32) {
    if amount == 0.0 {
        return;
    }
    let r = (radius.round() as usize).clamp(1, 20);
    unsharp(pixels, width, height, r, amount / 100.0);
}

fn unsharp(pixels: &mut [f32], width: usize, height: usize, radius: usize, k: f32) {
    let blurred = box_blur(pixels, width, height, radius);
    for (v, b) in pixels.iter_mut().zip(blurred.iter()) {
        *v = (*v + (*v - *b) * k).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_close_tol;

    fn checkerboard(w: usize, h: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let c = if (x + y) % 2 == 0 { 0.2 } else { 0.8 };
                v.extend_from_slice(&[c, c, c]);
            }
        }
        v
    }

    /// A single vertical step edge: left half low, right half high,
    /// uniform along y. Used to measure how far an unsharp mask's effect
    /// reaches from an isolated edge, as opposed to a checkerboard's
    /// wall-to-wall periodic structure.
    fn step_edge(w: usize, h: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(w * h * 3);
        for _y in 0..h {
            for x in 0..w {
                let c = if x < w / 2 { 0.2 } else { 0.8 };
                v.extend_from_slice(&[c, c, c]);
            }
        }
        v
    }

    fn mean(p: &[f32]) -> f32 {
        p.iter().sum::<f32>() / p.len() as f32
    }

    fn variance(p: &[f32]) -> f32 {
        let m = mean(p);
        p.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / p.len() as f32
    }

    #[test]
    fn blur_of_a_flat_image_changes_nothing() {
        // A flat field is the one input where "changes nothing" is the
        // *correct* answer, not just what a no-op would produce; paired
        // with `blur_reduces_variance` below, which a no-op cannot pass.
        let flat = vec![0.42f32; 8 * 8 * 3];
        let out = box_blur(&flat, 8, 8, 2);
        for (i, v) in out.iter().enumerate() {
            assert_close_tol(*v, 0.42, 1e-4, &format!("flat image moved at index {i}"));
        }
    }

    #[test]
    fn blur_reduces_variance() {
        // A checkerboard has maximal high-frequency structure; averaging
        // neighbours must smooth it. A no-op leaves variance unchanged and
        // fails the `< 0.5x` bound.
        let src = checkerboard(16, 16);
        let out = box_blur(&src, 16, 16, 2);
        assert!(variance(&out) < variance(&src) * 0.5, "blur must smooth");
    }

    #[test]
    fn blur_preserves_the_mean() {
        let src = checkerboard(16, 16);
        let out = box_blur(&src, 16, 16, 2);
        assert_close_tol(
            mean(&out),
            mean(&src),
            1e-3,
            "blur must preserve the global mean",
        );
    }

    #[test]
    fn clarity_raises_local_variance_without_moving_the_global_mean() {
        // The defining property. A brightness or contrast change would move
        // the mean; local contrast must not. A no-op leaves variance
        // unchanged and fails the `> 1.05x` bound.
        let src = checkerboard(32, 32);
        let mut out = src.clone();
        apply_clarity(&mut out, 32, 32, 60.0);

        assert!(
            variance(&out) > variance(&src) * 1.05,
            "clarity must raise local contrast: {} -> {}",
            variance(&src),
            variance(&out)
        );
        assert_close_tol(
            mean(&out),
            mean(&src),
            0.02,
            "global mean must hold under clarity",
        );
    }

    #[test]
    fn clarity_of_zero_is_a_no_op() {
        let src = checkerboard(8, 8);
        let mut out = src.clone();
        apply_clarity(&mut out, 8, 8, 0.0);
        for (i, (a, b)) in out.iter().zip(src.iter()).enumerate() {
            assert_close_tol(
                *a,
                *b,
                1e-6,
                &format!("clarity 0 must be an exact no-op, index {i}"),
            );
        }
    }

    #[test]
    fn negative_clarity_softens() {
        // Same checkerboard as the positive-clarity test; a no-op leaves
        // variance unchanged and fails the strict `<` bound.
        let src = checkerboard(32, 32);
        let mut out = src.clone();
        apply_clarity(&mut out, 32, 32, -60.0);
        assert!(
            variance(&out) < variance(&src),
            "negative clarity must soften"
        );
    }

    #[test]
    fn sharpen_affects_a_narrower_footprint_than_clarity_at_the_same_amount() {
        // Same operation, different radius: sharpen's 1px radius confines
        // its effect to a narrow band around an edge; clarity's much larger
        // radius (image-scaled, 3px on a 128px image) spreads it wider.
        //
        // DEVIATES FROM THE BRIEF. The brief's literal test asserted
        // `variance(sharp) > variance(clear)` on a checkerboard at a fixed
        // radius collision it flagged itself (32x32 -> both radius 1) and
        // told us to fix by growing the image to 128x128. Doing that
        // surfaces a deeper, separate problem: at 128x128 the radii differ
        // (clarity=3, sharpen=1) but `variance(sharp) > variance(clear)`
        // is *still* false (0.23023 vs 0.23040) — clarity wins.
        //
        // This is not an implementation bug; it is a property of box-blur
        // unsharp masking at equal `amount`. A box filter's response
        // magnitude at a fixed spatial frequency generally *decreases* as
        // the kernel widens (until a null), so a *larger* radius attenuates
        // fixed-frequency content *more*, leaving a *bigger* residual
        // (v - blurred) to amplify — the opposite of the brief's
        // assumption that the smaller radius (sharpen) would dominate.
        // Verified numerically: checkerboard periods 2/3/4/6 (clarity wins
        // or ties in all but an unstable near-tie at period 3), an isolated
        // step edge's variance (0.0956 clarity vs 0.0922 sharpen) and peak
        // overshoot (0.961 vs 0.933), all with the brief's own reference
        // algorithm, unmodified.
        //
        // The qualitative difference the brief's title names — sharpen
        // targets pixel-scale detail, clarity a broader neighbourhood — is
        // real, but it shows up as the *spatial extent* of the change, not
        // its *magnitude*. This test asserts that instead: on an isolated
        // edge, sharpen changes strictly fewer pixels than clarity does.
        // Height must be large enough that apply_clarity's
        // width.min(height)-based radius formula still resolves to the
        // image's *width* scale (128 -> radius 3); a short image would
        // make height the limiting dimension and collapse both radii to 1,
        // same as the brief's original 32x32 collision.
        let src = step_edge(128, 128);
        let mut sharp = src.clone();
        let mut clear = src.clone();
        apply_sharpen(&mut sharp, 128, 128, 60.0, 1.0);
        apply_clarity(&mut clear, 128, 128, 60.0);

        let footprint = |out: &[f32]| -> usize {
            out.iter()
                .zip(src.iter())
                .filter(|(a, b)| (**a - **b).abs() > 1e-4)
                .count()
        };
        let (fs, fc) = (footprint(&sharp), footprint(&clear));
        assert!(
            fs < fc,
            "sharpen's footprint ({fs}) must be narrower than clarity's ({fc})"
        );
    }

    #[test]
    fn sharpen_stays_in_range() {
        let src = checkerboard(16, 16);
        let mut out = src.clone();
        apply_sharpen(&mut out, 16, 16, 100.0, 1.0);
        for v in &out {
            assert!((0.0..=1.0).contains(v), "escaped range: {v}");
        }
    }

    #[test]
    fn a_one_pixel_image_does_not_panic() {
        let mut p = vec![0.5f32; 3];
        apply_clarity(&mut p, 1, 1, 50.0);
        apply_sharpen(&mut p, 1, 1, 50.0, 1.0);
        assert!(p.iter().all(|v| v.is_finite()));
    }
}
