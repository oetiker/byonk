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

mod params;

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

    pub(crate) fn assert_close(a: f32, b: f32, what: &str) {
        assert!(
            (a - b).abs() < 1e-4,
            "{what}: expected {b}, got {a} (delta {})",
            (a - b).abs()
        );
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
