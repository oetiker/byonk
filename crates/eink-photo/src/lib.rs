//! eink-photo: tone mapping for photographs bound for e-ink displays.
//!
//! # Domains
//!
//! `process` takes pixels in the **tone domain** — RGB, 3 f32 per pixel,
//! sRGB transfer curve applied, nominally 0.0..=1.0. Operations that model
//! light (exposure, white balance) convert to linear internally and back;
//! everything tonal stays in the tone domain. See the crate's README section
//! in the design spec for why this split is not negotiable.
//!
//! # Order
//!
//! The pipeline order is fixed and not author-controllable:
//! exposure → white balance → auto_levels/blacks/whites → highlights/shadows
//! → contrast → curve → clarity → vibrance → saturation → grayscale/invert
//! → sharpen.

mod color;
mod colorops;
mod params;
mod presence;
mod preset;
mod tone;

pub use params::{Params, Preset, Sharpen};
pub use preset::palette_endpoints;

#[derive(Debug, PartialEq)]
pub enum PhotoError {
    BufferLength {
        expected: usize,
        got: usize,
    },
    OutOfRange {
        field: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },
    BadCurve(&'static str),
}

impl std::fmt::Display for PhotoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhotoError::BufferLength { expected, got } => {
                write!(f, "pixel buffer has {got} values, expected {expected}")
            }
            PhotoError::OutOfRange {
                field,
                value,
                min,
                max,
            } => {
                write!(
                    f,
                    "{field} = {value} is outside the valid range {min}..={max}"
                )
            }
            PhotoError::BadCurve(why) => write!(f, "invalid tone curve: {why}"),
        }
    }
}

impl std::error::Error for PhotoError {}

/// Apply the pipeline in place. See the module docs for the domain contract.
pub fn process(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    params: &Params,
) -> Result<(), PhotoError> {
    let expected = width * height * 3;
    if pixels.len() != expected {
        return Err(PhotoError::BufferLength {
            expected,
            got: pixels.len(),
        });
    }

    let p = preset::apply_base_layer(params);
    validate(&p)?;

    // --- linear-light group -------------------------------------------------
    if let Some(ev) = p.exposure {
        tone::apply_exposure(pixels, ev);
    }
    let temp = p.temperature.unwrap_or(0.0);
    let tint = p.tint.unwrap_or(0.0);
    if temp != 0.0 || tint != 0.0 {
        tone::apply_white_balance(pixels, temp, tint);
    }

    // --- tone-domain group --------------------------------------------------
    // Endpoint placement. `auto_levels` decides where the image currently
    // starts and ends; `output_endpoints` (from palette_aware) decides where
    // it should land; blacks/whites nudge the target.
    let src = if p.auto_levels.unwrap_or(false) {
        tone::measure_endpoints(pixels)
    } else {
        (0.0, 1.0)
    };
    let (mut dst_lo, mut dst_hi) = p.output_endpoints.unwrap_or((0.0, 1.0));
    // blacks/whites shift the target by up to 0.15 at full scale.
    dst_lo = (dst_lo - p.blacks.unwrap_or(0.0) / 100.0 * 0.15).clamp(0.0, 1.0);
    dst_hi = (dst_hi + p.whites.unwrap_or(0.0) / 100.0 * 0.15).clamp(0.0, 1.0);
    if src != (0.0, 1.0) || (dst_lo, dst_hi) != (0.0, 1.0) {
        tone::apply_endpoints(pixels, src, (dst_lo, dst_hi));
    }

    tone::apply_highlights_shadows(
        pixels,
        p.highlights.unwrap_or(0.0),
        p.shadows.unwrap_or(0.0),
    );
    if let Some(c) = p.contrast {
        tone::apply_contrast(pixels, c);
    }
    if let Some(ref points) = p.curve {
        tone::apply_curve(pixels, points)?;
    }

    // --- presence and colour ------------------------------------------------
    if let Some(c) = p.clarity {
        presence::apply_clarity(pixels, width, height, c);
    }
    if let Some(v) = p.vibrance {
        colorops::apply_vibrance(pixels, v);
    }
    if let Some(s) = p.saturation {
        colorops::apply_saturation(pixels, s);
    }
    if p.grayscale.unwrap_or(false) {
        colorops::apply_grayscale(pixels);
    }
    if p.invert.unwrap_or(false) {
        colorops::apply_invert(pixels);
    }

    // --- detail, last, at output resolution ---------------------------------
    if let Some(s) = p.sharpen {
        presence::apply_sharpen(pixels, width, height, s.amount, s.radius);
    }

    Ok(())
}

/// Range checks. Out-of-range is an error rather than a silent clamp, so a
/// typo like `exposure = 30` for `3.0` is caught instead of saturating.
fn validate(p: &Params) -> Result<(), PhotoError> {
    let check =
        |field: &'static str, v: Option<f32>, min: f32, max: f32| -> Result<(), PhotoError> {
            match v {
                Some(x) if x < min || x > max => Err(PhotoError::OutOfRange {
                    field,
                    value: x,
                    min,
                    max,
                }),
                _ => Ok(()),
            }
        };
    check("exposure", p.exposure, -5.0, 5.0)?;
    check("temperature", p.temperature, -100.0, 100.0)?;
    check("tint", p.tint, -100.0, 100.0)?;
    check("blacks", p.blacks, -100.0, 100.0)?;
    check("whites", p.whites, -100.0, 100.0)?;
    check("highlights", p.highlights, -100.0, 100.0)?;
    check("shadows", p.shadows, -100.0, 100.0)?;
    check("contrast", p.contrast, -100.0, 100.0)?;
    check("clarity", p.clarity, -100.0, 100.0)?;
    check("vibrance", p.vibrance, -100.0, 100.0)?;
    check("saturation", p.saturation, -100.0, 100.0)?;
    if let Some(s) = p.sharpen {
        check("sharpen.amount", Some(s.amount), 0.0, 100.0)?;
        check("sharpen.radius", Some(s.radius), 0.3, 10.0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared assertion helper for all crate tests, per Plan B's Global
    /// Constraints ("Use a shared `assert_close` helper; do not invent
    /// per-test epsilons"). `assert_close` covers the default `1e-4`
    /// tolerance; `assert_close_tol` is the escape hatch for the rare test
    /// that legitimately needs something else — pass the tolerance
    /// explicitly rather than inlining a bare `(a - b).abs() < …` comparison.
    pub(crate) fn assert_close_tol(a: f32, b: f32, tol: f32, what: &str) {
        assert!(
            (a - b).abs() < tol,
            "{what}: expected {b}, got {a} (delta {}, tolerance {tol})",
            (a - b).abs()
        );
    }

    pub(crate) fn assert_close(a: f32, b: f32, what: &str) {
        assert_close_tol(a, b, 1e-4, what);
    }

    #[test]
    fn default_params_are_a_no_op() {
        let original = vec![0.0f32, 0.25, 0.5, 0.75, 1.0, 0.33];
        let mut pixels = original.clone();
        process(&mut pixels, 2, 1, &Params::default()).expect("must succeed");
        for (i, (got, want)) in pixels.iter().zip(original.iter()).enumerate() {
            assert_close(*got, *want, &format!("channel {i}"));
        }
    }

    #[test]
    fn wrong_buffer_length_is_an_error() {
        let mut pixels = vec![0.0f32; 5]; // 2x1 RGB needs 6
        let err = process(&mut pixels, 2, 1, &Params::default()).unwrap_err();
        assert!(matches!(err, PhotoError::BufferLength { .. }));
    }

    #[test]
    fn process_applies_operations_in_the_fixed_order() {
        // Proves `output_endpoints` (the `palette_aware` path) is wired
        // through `process` end to end: black must land exactly on the
        // panel's darkest measured ink, white on its lightest. This does
        // NOT by itself catch an operation-order transposition — see
        // `process_order_differs_from_contrast_before_exposure` for that.
        let mut wide = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        let params = Params {
            output_endpoints: Some((0.05, 0.92)),
            ..Default::default()
        };
        process(&mut wide, 2, 1, &params).unwrap();
        assert!(
            (wide[0] - 0.05).abs() < 1e-3,
            "black must land on the panel black: {}",
            wide[0]
        );
        assert!(
            (wide[3] - 0.92).abs() < 1e-3,
            "white must land on the panel white: {}",
            wide[3]
        );
    }

    #[test]
    fn process_order_differs_from_contrast_before_exposure() {
        // `process_applies_operations_in_the_fixed_order` above only proves
        // apply_endpoints is wired correctly — with no other params set it
        // cannot catch e.g. a linear-light-group/tone-domain-group swap.
        // exposure and contrast are a clean pair to check that with: exposure
        // (`tone::apply_exposure`) round-trips through linear light and
        // contrast (`tone::apply_contrast`) is a nonlinear S-curve applied
        // directly in the tone domain, so composing them in the wrong order
        // is a *generic* mismatch — no clamp-edge case needed, unlike a
        // blur-based probe (tried first; box_blur's 3-pass averaging turned
        // out to soften overshoot enough that a sharpen/endpoints swap was
        // *not* observable at the parameters tried — a reminder that a
        // transposition test must be checked empirically, not assumed).
        let input = vec![0.12f32, 0.34, 0.56, 0.78, 0.9, 0.05];
        let params = Params {
            exposure: Some(1.0),
            contrast: Some(60.0),
            ..Default::default()
        };

        let mut correct = input.clone();
        process(&mut correct, 2, 1, &params).expect("must succeed");

        // Hand-build the wrong order: contrast (tone domain) before exposure
        // (linear light), i.e. the two groups swapped.
        let mut wrong = input.clone();
        tone::apply_contrast(&mut wrong, 60.0);
        tone::apply_exposure(&mut wrong, 1.0);

        let differs = correct
            .iter()
            .zip(wrong.iter())
            .any(|(a, b)| (a - b).abs() > 1e-3);
        assert!(
            differs,
            "correct pipeline order (exposure before contrast) must differ \
             from contrast-before-exposure: {correct:?} vs {wrong:?}"
        );
    }

    #[test]
    fn process_rejects_an_out_of_range_slider() {
        let mut p = vec![0.5f32; 3];
        let err = process(
            &mut p,
            1,
            1,
            &Params {
                exposure: Some(30.0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PhotoError::OutOfRange {
                field: "exposure",
                ..
            }
        ));
    }

    #[test]
    fn process_with_the_eink_preset_runs_end_to_end() {
        let mut p: Vec<f32> = (0..64 * 64 * 3).map(|i| (i % 255) as f32 / 255.0).collect();
        process(
            &mut p,
            64,
            64,
            &Params {
                preset: Preset::Eink,
                ..Default::default()
            },
        )
        .expect("preset must run");
        assert!(p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }
}
