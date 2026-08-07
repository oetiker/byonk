//! Parameters for the tone pipeline.
//!
//! Every field is `Option<T>` so that "the author did not set this" is
//! distinguishable from "the author set this to zero" — the distinction that
//! makes `Preset` work as a base layer (see `preset.rs`).

/// Which fixed base layer to start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// No base layer.
    #[default]
    None,
    /// Tuned for a low-dynamic-range, few-colour e-ink panel.
    Eink,
}

/// Small-radius output sharpening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sharpen {
    /// 0..=100
    pub amount: f32,
    /// Blur radius in pixels, 0.3..=10.0
    pub radius: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params {
    pub preset: Preset,

    // Tone — applied in the tone domain unless noted.
    /// Stops of exposure, -5..=5. Applied in LINEAR light.
    pub exposure: Option<f32>,
    /// -100..=100. Applied in LINEAR light.
    pub temperature: Option<f32>,
    /// -100..=100. Applied in LINEAR light.
    pub tint: Option<f32>,
    /// Stretch the histogram to the full range before the other tone ops.
    pub auto_levels: Option<bool>,
    /// -100..=100
    pub blacks: Option<f32>,
    /// -100..=100
    pub whites: Option<f32>,
    /// -100..=100
    pub highlights: Option<f32>,
    /// -100..=100
    pub shadows: Option<f32>,
    /// -100..=100
    pub contrast: Option<f32>,
    /// Point tone curve as (input, output) pairs in 0..=1, sorted by input.
    pub curve: Option<Vec<(f32, f32)>>,

    // Presence
    /// -100..=100, large-radius local contrast.
    pub clarity: Option<f32>,
    /// -100..=100, weighted toward less-saturated pixels.
    pub vibrance: Option<f32>,
    /// -100..=100, global.
    pub saturation: Option<f32>,

    // Colour
    pub grayscale: Option<bool>,
    pub invert: Option<bool>,

    // Detail
    pub sharpen: Option<Sharpen>,

    /// Black and white points, in tone-domain luminance, derived from the
    /// output device's real palette. Set by the caller when the author asked
    /// for `palette_aware`; see `preset::palette_endpoints`.
    pub output_endpoints: Option<(f32, f32)>,
}
