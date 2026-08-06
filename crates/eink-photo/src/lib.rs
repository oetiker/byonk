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

// Wired into `process`'s pipeline by a later task; not dead code, just not
// yet called from here.
#[allow(dead_code)]
mod color;
mod params;
#[allow(dead_code)]
mod tone;

pub use params::{Params, Preset, Sharpen};

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
    let _ = params;
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
}
