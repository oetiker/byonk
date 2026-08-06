# Plan B — `image_process` for E-Ink

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Lua screen scripts a single `image_process(bytes, opts)` call that tone-maps, crops, resizes and sharpens a photograph so it survives being dithered to a six-colour e-ink panel.

**Architecture:** Three layers with one job each. `crates/eink-photo` is a **zero-dependency** crate holding the tone pipeline (steps 4–16) — pixels in, pixels out, no file formats, no fixtures needed for its tests. `src/services/image_process.rs` owns the `image` dependency and does decode / crop / resize / encode (steps 1–3, 17) plus the size guards. `src/services/lua_runtime.rs` only binds the two together.

**Tech Stack:** Rust 2021, `image` 0.25 (`default-features = false`, features `jpeg`/`png`/`webp`), `mlua` 0.10, `base64`.

**Spec:** `docs/superpowers/specs/2026-08-06-lua-colors-and-image-ops-design.md`, Part 2.

**Depends on Plan A** for one thing only: `palette_aware` reads `device.colors_actual`. Tasks 1–8 are independent of Plan A and can be built without it; Task 9 needs Plan A's Task 1 merged.

## Global Constraints

- **Never `git add -A` or `git add .`** — this repo has untracked local files that must not be swept in. Stage explicit paths and check `git diff --cached` before every commit.
- **Never use `sed -i` to modify files.** macOS BSD `sed` requires a backup-suffix argument and has destroyed work here. Use Edit/Write.
- **Run everything in the foreground.** Do not background `make check`.
- **Cap build/test parallelism at 4** — shared machine.
- `make check` = `cargo fmt` + `cargo clippy -- -D warnings` + `cargo test`. Clippy warnings are errors.
- **`crates/eink-photo` must have zero runtime dependencies.** `[dependencies]` stays empty for the whole plan. If you reach for a crate, you have mis-scoped an operation — say so instead of adding it.
- **Commit after every task**, and incrementally within a task if it runs long.
- **CHANGES.md is user-facing only.**
- Every test must **fail against the current code**. Confirm it before claiming it is meaningful.
- **All tolerances in this plan are `1e-4` on f32 unless stated.** Use a shared `assert_close` helper; do not invent per-test epsilons.
- If a step here is wrong, say so. Disclosure is valued, not penalised.

## Colour-space contract (read this before Task 2)

Two domains, and every operation belongs to exactly one:

| Domain | Range | Used for |
|---|---|---|
| **Linear light** | f32, 0.0–1.0 (values >1.0 permitted mid-pipeline) | exposure, white balance |
| **Tone domain** (sRGB transfer curve applied) | f32, 0.0–1.0 | auto_levels, blacks/whites, highlights/shadows, contrast, curve, clarity, vibrance, saturation, grayscale/invert, sharpen |

Exposure and white balance model light and are only correct as multiplications in linear
light. Everything tonal is defined against a perceptually-spaced scale; an S-curve applied
in linear light crushes midtones. This is the same distinction `eink-dither` already draws
(error diffusion in linear RGB, colour matching in OKLab) — do not collapse it.

## File Structure

| File | Responsibility |
|---|---|
| `crates/eink-photo/Cargo.toml` | Crate manifest. **No dependencies.** |
| `crates/eink-photo/src/lib.rs` | Public API: `Params`, `process`, `Preset`, re-exports |
| `crates/eink-photo/src/color.rs` | sRGB ⇄ linear transfer, luminance, HSL-free saturation helpers |
| `crates/eink-photo/src/tone.rs` | exposure, white balance, endpoints, highlights/shadows, contrast, curve |
| `crates/eink-photo/src/presence.rs` | separable blur, clarity, sharpen |
| `crates/eink-photo/src/colorops.rs` | vibrance, saturation, grayscale, invert |
| `crates/eink-photo/src/preset.rs` | `Preset::Eink` base-layer values, `palette_aware` endpoint derivation |
| `crates/eink-photo/src/params.rs` | `Params` struct, `Default`, validation |
| `src/services/image_process.rs` | decode/crop/fit/encode, size guards, data-URI wrapping |
| `src/services/lua_runtime.rs` | the `image_process` Lua global |
| `Cargo.toml` | workspace member + `image` dependency |
| `screens/examples/gphoto/script.lua` | motivating example, upgraded |
| `docs/src/api/lua-api.md`, `CHANGES.md` | user docs |

> **Line numbers elsewhere in this plan are anchors, not addresses** — read from the tree at commit `6a1caa3`. Locate code by the quoted content.

---

## Task 1: Scaffold `eink-photo` with `Params` and a pass-through `process`

**Files:**
- Create: `crates/eink-photo/Cargo.toml`, `crates/eink-photo/src/lib.rs`, `crates/eink-photo/src/params.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces:
  - `eink_photo::Params` — every field `Option<T>` except `preset`, so "unset" is distinguishable from "set to zero".
  - `eink_photo::process(pixels: &mut [f32], width: usize, height: usize, params: &Params) -> Result<(), PhotoError>` — pixels are **RGB, 3 f32 per pixel, tone domain (sRGB-encoded), 0.0–1.0**, mutated in place. Length must be `width * height * 3`.
  - `eink_photo::PhotoError`

- [ ] **Step 1: Write the failing test**

Create `crates/eink-photo/src/params.rs` tests later; for now put this in `crates/eink-photo/src/lib.rs` at the bottom:

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p eink-photo`
Expected: the package does not exist. That is the failure.

- [ ] **Step 3: Create the manifest and register the workspace member**

`crates/eink-photo/Cargo.toml`:

```toml
[package]
name = "eink-photo"
version = "0.1.0"
edition = "2021"
description = "Tone mapping for photographs bound for e-ink displays"
license = "MIT"

# Intentionally dependency-free: this crate holds only pixel maths, so its
# tests need no fixtures and its behaviour is fully determined by its inputs.
[dependencies]
```

In the root `Cargo.toml`, change:

```toml
members = [".", "crates/eink-dither"]
```

to:

```toml
members = [".", "crates/eink-dither", "crates/eink-photo"]
```

- [ ] **Step 4: Write `Params`**

`crates/eink-photo/src/params.rs`:

```rust
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
```

- [ ] **Step 5: Write `lib.rs` with the error type and a pass-through `process`**

```rust
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
    BufferLength { expected: usize, got: usize },
    OutOfRange { field: &'static str, value: f32, min: f32, max: f32 },
    BadCurve(&'static str),
}

impl std::fmt::Display for PhotoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhotoError::BufferLength { expected, got } => {
                write!(f, "pixel buffer has {got} values, expected {expected}")
            }
            PhotoError::OutOfRange { field, value, min, max } => {
                write!(f, "{field} = {value} is outside the valid range {min}..={max}")
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
        return Err(PhotoError::BufferLength { expected, got: pixels.len() });
    }
    let _ = params;
    Ok(())
}
```

- [ ] **Step 6: Run**

Run: `cargo test -p eink-photo`
Expected: 2 PASS.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/eink-photo/
git diff --cached --stat
git commit -m "feat: scaffold eink-photo, a dependency-free tone pipeline for e-ink photos"
```

---

## Task 2: Colour transfer and exposure

**Files:**
- Create: `crates/eink-photo/src/color.rs`, `crates/eink-photo/src/tone.rs`
- Modify: `crates/eink-photo/src/lib.rs`

**Interfaces:**
- Produces:
  - `color::srgb_to_linear(v: f32) -> f32`, `color::linear_to_srgb(v: f32) -> f32`
  - `color::luminance(r: f32, g: f32, b: f32) -> f32` — Rec. 709 weights, on whatever domain the caller passes
  - `tone::apply_exposure(pixels: &mut [f32], ev: f32)` — converts to linear, multiplies, converts back

- [ ] **Step 1: Write the failing tests**

`crates/eink-photo/src/color.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool { (a - b).abs() < 1e-4 }

    #[test]
    fn transfer_round_trips() {
        for i in 0..=100 {
            let v = i as f32 / 100.0;
            assert!(close(linear_to_srgb(srgb_to_linear(v)), v), "round trip failed at {v}");
        }
    }

    #[test]
    fn transfer_endpoints_are_exact() {
        assert!(close(srgb_to_linear(0.0), 0.0));
        assert!(close(srgb_to_linear(1.0), 1.0));
        assert!(close(linear_to_srgb(0.0), 0.0));
        assert!(close(linear_to_srgb(1.0), 1.0));
    }

    #[test]
    fn mid_grey_is_about_eighteen_percent_linear() {
        // sRGB 0.5 is ~0.214 in linear light — the number that makes the
        // linear-vs-tone-domain distinction matter.
        assert!((srgb_to_linear(0.5) - 0.2140).abs() < 1e-3, "got {}", srgb_to_linear(0.5));
    }

    #[test]
    fn luminance_of_white_is_one() {
        assert!(close(luminance(1.0, 1.0, 1.0), 1.0));
    }
}
```

`crates/eink-photo/src/tone.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{linear_to_srgb, srgb_to_linear};

    #[test]
    fn exposure_of_one_ev_doubles_linear_light() {
        // The defining property. Start from a tone-domain value, expect the
        // LINEAR value to double.
        let start_tone = 0.25f32;
        let mut pixels = vec![start_tone; 3];
        apply_exposure(&mut pixels, 1.0);
        let got_linear = srgb_to_linear(pixels[0]);
        let want_linear = srgb_to_linear(start_tone) * 2.0;
        assert!(
            (got_linear - want_linear).abs() < 1e-4,
            "expected linear {want_linear}, got {got_linear}"
        );
    }

    #[test]
    fn exposure_of_zero_is_a_no_op() {
        let mut pixels = vec![0.1f32, 0.5, 0.9];
        let before = pixels.clone();
        apply_exposure(&mut pixels, 0.0);
        for (a, b) in pixels.iter().zip(before.iter()) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn exposure_clamps_at_white_not_beyond() {
        let mut pixels = vec![0.9f32; 3];
        apply_exposure(&mut pixels, 3.0);
        for v in &pixels {
            assert!(*v <= 1.0 + 1e-6, "value escaped the range: {v}");
            assert!((*v - 1.0).abs() < 1e-4, "a +3EV push from 0.9 must reach white, got {v}");
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p eink-photo`
Expected: compile errors — modules do not exist.

- [ ] **Step 3: Implement `color.rs`**

Above the test module:

```rust
//! sRGB transfer curve and luminance.

/// sRGB-encoded value (0..=1) to linear light. The exact IEC 61966-2-1 curve,
/// matching what `eink-dither` uses — a cheaper 2.2 power approximation would
/// put the two crates fractionally out of step at the shadow end.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to an sRGB-encoded value (0..=1). Values above 1.0 are
/// clamped, which is where an over-exposed highlight becomes paper white.
pub fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Rec. 709 luminance. Applied to whatever domain the caller passes — in the
/// tone domain this is a perceptual lightness proxy, which is what the tonal
/// operations want.
pub fn luminance(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
```

- [ ] **Step 4: Implement `tone.rs`'s exposure**

```rust
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
```

Add `mod color;` and `mod tone;` to `lib.rs` (private modules; only `Params` and `process` are public API).

- [ ] **Step 5: Run**

Run: `cargo test -p eink-photo`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/eink-photo/src/
git commit -m "feat(eink-photo): sRGB transfer, luminance, and exposure"
```

---

## Task 3: White balance, endpoints, and auto-levels

**Files:**
- Modify: `crates/eink-photo/src/tone.rs`

**Interfaces:**
- Consumes: `color::{srgb_to_linear, linear_to_srgb, luminance}`
- Produces:
  - `tone::apply_white_balance(pixels: &mut [f32], temperature: f32, tint: f32)`
  - `tone::measure_endpoints(pixels: &[f32]) -> (f32, f32)` — the 0.5th and 99.5th percentile of tone-domain luminance
  - `tone::apply_endpoints(pixels: &mut [f32], from: (f32, f32), to: (f32, f32))` — linear remap in the tone domain, clamped

- [ ] **Step 1: Write the failing tests**

Append to `tone.rs`'s test module:

```rust
    #[test]
    fn white_balance_warms_by_raising_red_and_lowering_blue() {
        let mut p = vec![0.5f32, 0.5, 0.5];
        apply_white_balance(&mut p, 50.0, 0.0);
        assert!(p[0] > 0.5, "red must rise, got {}", p[0]);
        assert!(p[2] < 0.5, "blue must fall, got {}", p[2]);
        assert!((p[1] - 0.5).abs() < 1e-4, "green must not move with temperature alone");
    }

    #[test]
    fn white_balance_of_zero_is_a_no_op() {
        let mut p = vec![0.2f32, 0.5, 0.8];
        let before = p.clone();
        apply_white_balance(&mut p, 0.0, 0.0);
        for (a, b) in p.iter().zip(before.iter()) {
            assert!((a - b).abs() < 1e-5);
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
        assert!(lo > 0.35 && lo < 0.45, "low endpoint dragged by outlier: {lo}");
        assert!(hi > 0.55 && hi < 0.65, "high endpoint dragged by outlier: {hi}");
    }

    #[test]
    fn apply_endpoints_stretches_to_the_target_range() {
        let mut p = vec![0.4f32, 0.4, 0.4, 0.6, 0.6, 0.6];
        apply_endpoints(&mut p, (0.4, 0.6), (0.0, 1.0));
        assert!(p[0].abs() < 1e-4, "low end must land at 0.0, got {}", p[0]);
        assert!((p[3] - 1.0).abs() < 1e-4, "high end must land at 1.0, got {}", p[3]);
    }

    #[test]
    fn apply_endpoints_can_compress_into_a_narrower_range() {
        // This is what palette_aware does: refuse to spend range the panel
        // cannot show.
        let mut p = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        apply_endpoints(&mut p, (0.0, 1.0), (0.05, 0.92));
        assert!((p[0] - 0.05).abs() < 1e-4, "black must land at the panel's black: {}", p[0]);
        assert!((p[3] - 0.92).abs() < 1e-4, "white must land at the panel's white: {}", p[3]);
    }

    #[test]
    fn apply_endpoints_with_a_degenerate_source_does_not_divide_by_zero() {
        let mut p = vec![0.5f32; 6];
        apply_endpoints(&mut p, (0.5, 0.5), (0.0, 1.0));
        for v in &p {
            assert!(v.is_finite(), "produced a non-finite value: {v}");
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p eink-photo tone`
Expected: compile errors — the three functions do not exist.

- [ ] **Step 3: Implement**

```rust
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
    let idx = |q: f32| -> usize {
        (((lums.len() - 1) as f32) * q).round() as usize
    };
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
```

- [ ] **Step 4: Run**

Run: `cargo test -p eink-photo`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/eink-photo/src/tone.rs
git commit -m "feat(eink-photo): white balance, endpoint measurement, and endpoint remapping"
```

---

## Task 4: Highlights, shadows, contrast, and the tone curve

**Files:**
- Modify: `crates/eink-photo/src/tone.rs`

**Interfaces:**
- Produces:
  - `tone::apply_highlights_shadows(pixels: &mut [f32], highlights: f32, shadows: f32)`
  - `tone::apply_contrast(pixels: &mut [f32], amount: f32)`
  - `tone::apply_curve(pixels: &mut [f32], points: &[(f32, f32)]) -> Result<(), PhotoError>`

- [ ] **Step 1: Write the failing tests**

```rust
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
        assert!(q_after > q_before + 0.02, "shadows must lift: {q_before} -> {q_after}");

        let t_before = mean_of(&before, 90, 100);
        let t_after = mean_of(&after, 90, 100);
        assert!(
            (t_after - t_before).abs() < 0.02,
            "top decile must stay put: {t_before} -> {t_after}"
        );
    }

    #[test]
    fn highlights_recover_the_top_and_leave_the_bottom_decile_alone() {
        let before = ramp(100);
        let mut after = before.clone();
        apply_highlights_shadows(&mut after, -50.0, 0.0);

        let t_before = mean_of(&before, 75, 100);
        let t_after = mean_of(&after, 75, 100);
        assert!(t_after < t_before - 0.02, "highlights must pull down: {t_before} -> {t_after}");

        let b_before = mean_of(&before, 0, 10);
        let b_after = mean_of(&after, 0, 10);
        assert!(
            (b_after - b_before).abs() < 0.02,
            "bottom decile must stay put: {b_before} -> {b_after}"
        );
    }

    #[test]
    fn highlights_and_shadows_of_zero_are_a_no_op() {
        let before = ramp(50);
        let mut after = before.clone();
        apply_highlights_shadows(&mut after, 0.0, 0.0);
        for (a, b) in after.iter().zip(before.iter()) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn contrast_pivots_about_mid_grey_in_the_tone_domain() {
        // This is the assertion that catches an S-curve applied in linear
        // light: there, tone-domain 0.5 would NOT be the fixed point.
        let mut p = vec![0.5f32; 3];
        apply_contrast(&mut p, 60.0);
        assert!((p[0] - 0.5).abs() < 1e-4, "mid grey must be the fixed point, got {}", p[0]);
    }

    #[test]
    fn positive_contrast_pushes_away_from_mid_grey() {
        let mut p = vec![0.25f32, 0.25, 0.25, 0.75, 0.75, 0.75];
        apply_contrast(&mut p, 50.0);
        assert!(p[0] < 0.25, "quarter tone must darken, got {}", p[0]);
        assert!(p[3] > 0.75, "three-quarter tone must brighten, got {}", p[3]);
    }

    #[test]
    fn negative_contrast_pulls_toward_mid_grey() {
        let mut p = vec![0.25f32, 0.25, 0.25];
        apply_contrast(&mut p, -50.0);
        assert!(p[0] > 0.25, "flattening must raise the quarter tone, got {}", p[0]);
    }

    #[test]
    fn contrast_is_monotonic() {
        // A tone operation that reorders brightness is broken, however
        // pleasing any single sample looks.
        let mut p = ramp(64);
        apply_contrast(&mut p, 80.0);
        for w in p.chunks_exact(3).collect::<Vec<_>>().windows(2) {
            assert!(w[1][0] >= w[0][0] - 1e-6, "ordering inverted: {} then {}", w[0][0], w[1][0]);
        }
    }

    #[test]
    fn curve_interpolates_between_its_points() {
        let mut p = vec![0.5f32; 3];
        apply_curve(&mut p, &[(0.0, 0.0), (0.5, 0.8), (1.0, 1.0)]).unwrap();
        assert!((p[0] - 0.8).abs() < 1e-4, "got {}", p[0]);
    }

    #[test]
    fn curve_endpoints_are_honoured() {
        let mut p = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        apply_curve(&mut p, &[(0.0, 0.1), (1.0, 0.9)]).unwrap();
        assert!((p[0] - 0.1).abs() < 1e-4);
        assert!((p[3] - 0.9).abs() < 1e-4);
    }

    #[test]
    fn curve_rejects_unsorted_or_too_short_input() {
        let mut p = vec![0.5f32; 3];
        assert!(apply_curve(&mut p, &[(0.5, 0.5)]).is_err(), "one point is not a curve");
        assert!(
            apply_curve(&mut p, &[(1.0, 1.0), (0.0, 0.0)]).is_err(),
            "unsorted input must be rejected, not silently reordered"
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p eink-photo tone`
Expected: compile errors.

- [ ] **Step 3: Implement**

```rust
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
        return Err(crate::PhotoError::BadCurve("a curve needs at least two points"));
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
            let i = points.windows(2).position(|w| x >= w[0].0 && x <= w[1].0).unwrap_or(0);
            let (x0, y0) = points[i];
            let (x1, y1) = points[i + 1];
            let f = (x - x0) / (x1 - x0);
            y0 + (y1 - y0) * f
        }
        .clamp(0.0, 1.0);
    }
    Ok(())
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p eink-photo`
Expected: all PASS. If `contrast_is_monotonic` fails at high amounts, that is a real defect in the blend — fix the implementation, do not relax the test.

- [ ] **Step 5: Commit**

```bash
git add crates/eink-photo/src/tone.rs
git commit -m "feat(eink-photo): highlight/shadow recovery, contrast, and point tone curve"
```

---

## Task 5: Clarity and sharpening

**Files:**
- Create: `crates/eink-photo/src/presence.rs`
- Modify: `crates/eink-photo/src/lib.rs` (add `mod presence;`)

**Interfaces:**
- Produces:
  - `presence::box_blur(pixels: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32>` — three separable passes, which approximates a Gaussian closely enough for an unsharp mask
  - `presence::apply_clarity(pixels: &mut [f32], width: usize, height: usize, amount: f32)`
  - `presence::apply_sharpen(pixels: &mut [f32], width: usize, height: usize, amount: f32, radius: f32)`

Clarity and sharpening are the same operation (unsharp mask) at different radii; the split exists because the *radii* differ by an order of magnitude and authors think of them as different controls.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

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

    fn mean(p: &[f32]) -> f32 { p.iter().sum::<f32>() / p.len() as f32 }

    fn variance(p: &[f32]) -> f32 {
        let m = mean(p);
        p.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / p.len() as f32
    }

    #[test]
    fn blur_of_a_flat_image_changes_nothing() {
        let mut flat = vec![0.42f32; 8 * 8 * 3];
        let out = box_blur(&flat, 8, 8, 2);
        for v in &out {
            assert!((v - 0.42).abs() < 1e-4, "flat image moved: {v}");
        }
        flat[0] = 0.42; // silence unused_mut in some toolchains
    }

    #[test]
    fn blur_reduces_variance() {
        let src = checkerboard(16, 16);
        let out = box_blur(&src, 16, 16, 2);
        assert!(variance(&out) < variance(&src) * 0.5, "blur must smooth");
    }

    #[test]
    fn blur_preserves_the_mean() {
        let src = checkerboard(16, 16);
        let out = box_blur(&src, 16, 16, 2);
        assert!((mean(&out) - mean(&src)).abs() < 1e-3);
    }

    #[test]
    fn clarity_raises_local_variance_without_moving_the_global_mean() {
        // The defining property. A brightness or contrast change would move
        // the mean; local contrast must not.
        let src = checkerboard(32, 32);
        let mut out = src.clone();
        apply_clarity(&mut out, 32, 32, 60.0);

        assert!(
            variance(&out) > variance(&src) * 1.05,
            "clarity must raise local contrast: {} -> {}",
            variance(&src),
            variance(&out)
        );
        assert!(
            (mean(&out) - mean(&src)).abs() < 0.02,
            "global mean must hold: {} -> {}",
            mean(&src),
            mean(&out)
        );
    }

    #[test]
    fn clarity_of_zero_is_a_no_op() {
        let src = checkerboard(8, 8);
        let mut out = src.clone();
        apply_clarity(&mut out, 8, 8, 0.0);
        for (a, b) in out.iter().zip(src.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn negative_clarity_softens() {
        let src = checkerboard(32, 32);
        let mut out = src.clone();
        apply_clarity(&mut out, 32, 32, -60.0);
        assert!(variance(&out) < variance(&src), "negative clarity must soften");
    }

    #[test]
    fn sharpen_raises_edge_contrast_more_than_clarity_at_the_same_amount() {
        // Same operation, different radius: sharpening acts on the
        // pixel-scale detail a checkerboard is made of.
        let src = checkerboard(32, 32);
        let mut sharp = src.clone();
        let mut clear = src.clone();
        apply_sharpen(&mut sharp, 32, 32, 60.0, 1.0);
        apply_clarity(&mut clear, 32, 32, 60.0);
        assert!(
            variance(&sharp) > variance(&clear),
            "sharpen {} must exceed clarity {} on pixel-scale detail",
            variance(&sharp),
            variance(&clear)
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p eink-photo presence`
Expected: module does not exist.

- [ ] **Step 3: Implement**

```rust
//! Local-contrast operations. Clarity and sharpening are the same unsharp
//! mask at radii an order of magnitude apart; authors think of them as
//! different controls, so they get different names.

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
```

- [ ] **Step 4: Run**

Run: `cargo test -p eink-photo`
Expected: all PASS.

> If `sharpen_raises_edge_contrast_more_than_clarity_at_the_same_amount` fails, check the clarity radius: on a 32x32 test image, `32 * 0.02 = 0.64` rounds to 0 and is clamped to 1, making the two identical. If so, raise the test image to 128x128 rather than changing the radius formula — and **report that you did**, because a test that had to be adjusted to pass deserves scrutiny.

- [ ] **Step 5: Commit**

```bash
git add crates/eink-photo/src/presence.rs crates/eink-photo/src/lib.rs
git commit -m "feat(eink-photo): separable blur, clarity, and output sharpening"
```

---

## Task 6: Vibrance, saturation, grayscale, invert

**Files:**
- Create: `crates/eink-photo/src/colorops.rs`
- Modify: `crates/eink-photo/src/lib.rs` (add `mod colorops;`)

**Interfaces:**
- Produces:
  - `colorops::apply_vibrance(pixels: &mut [f32], amount: f32)`
  - `colorops::apply_saturation(pixels: &mut [f32], amount: f32)`
  - `colorops::apply_grayscale(pixels: &mut [f32])`
  - `colorops::apply_invert(pixels: &mut [f32])`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Distance from grey, the working definition of saturation here.
    fn sat(px: &[f32]) -> f32 {
        let l = crate::color::luminance(px[0], px[1], px[2]);
        ((px[0] - l).abs() + (px[1] - l).abs() + (px[2] - l).abs()) / 3.0
    }

    #[test]
    fn saturation_scales_every_pixel_equally() {
        let mut dull = vec![0.55f32, 0.5, 0.45];
        let mut vivid = vec![0.9f32, 0.5, 0.1];
        let dull_before = sat(&dull);
        let vivid_before = sat(&vivid);

        apply_saturation(&mut dull, 50.0);
        apply_saturation(&mut vivid, 50.0);

        let dull_ratio = sat(&dull) / dull_before;
        let vivid_ratio = sat(&vivid) / vivid_before;
        assert!(
            (dull_ratio - vivid_ratio).abs() < 0.05,
            "global saturation must be uniform: {dull_ratio} vs {vivid_ratio}"
        );
    }

    #[test]
    fn vibrance_favours_the_less_saturated_pixel() {
        // This is the property that distinguishes vibrance from saturation,
        // and the reason it matters on a six-colour panel: dull colours are
        // the ones that never reach a chromatic palette entry.
        let mut dull = vec![0.55f32, 0.5, 0.45];
        let mut vivid = vec![0.9f32, 0.5, 0.1];
        let dull_before = sat(&dull);
        let vivid_before = sat(&vivid);

        apply_vibrance(&mut dull, 50.0);
        apply_vibrance(&mut vivid, 50.0);

        let dull_ratio = sat(&dull) / dull_before;
        let vivid_ratio = sat(&vivid) / vivid_before;
        assert!(
            dull_ratio > vivid_ratio * 1.2,
            "vibrance must favour the dull pixel: {dull_ratio} vs {vivid_ratio}"
        );
    }

    #[test]
    fn vibrance_and_saturation_of_zero_are_no_ops() {
        let before = vec![0.9f32, 0.5, 0.1];
        let mut a = before.clone();
        let mut b = before.clone();
        apply_vibrance(&mut a, 0.0);
        apply_saturation(&mut b, 0.0);
        for i in 0..3 {
            assert!((a[i] - before[i]).abs() < 1e-6);
            assert!((b[i] - before[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn full_negative_saturation_is_grey() {
        let mut p = vec![0.9f32, 0.5, 0.1];
        apply_saturation(&mut p, -100.0);
        assert!((p[0] - p[1]).abs() < 1e-4 && (p[1] - p[2]).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn saturation_preserves_luminance() {
        let p0 = vec![0.9f32, 0.5, 0.1];
        let mut p = p0.clone();
        let before = crate::color::luminance(p0[0], p0[1], p0[2]);
        apply_saturation(&mut p, 60.0);
        let after = crate::color::luminance(p[0], p[1], p[2]);
        assert!((after - before).abs() < 0.02, "luminance drifted: {before} -> {after}");
    }

    #[test]
    fn grayscale_flattens_channels_to_luminance() {
        let mut p = vec![0.9f32, 0.5, 0.1];
        let l = crate::color::luminance(0.9, 0.5, 0.1);
        apply_grayscale(&mut p);
        for c in &p {
            assert!((c - l).abs() < 1e-5, "expected {l}, got {c}");
        }
    }

    #[test]
    fn invert_is_its_own_inverse() {
        let before = vec![0.9f32, 0.5, 0.1];
        let mut p = before.clone();
        apply_invert(&mut p);
        apply_invert(&mut p);
        for (a, b) in p.iter().zip(before.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p eink-photo colorops`
Expected: module does not exist.

- [ ] **Step 3: Implement**

```rust
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
        let current = (((px[0] - l).abs() + (px[1] - l).abs() + (px[2] - l).abs()) / 3.0)
            .clamp(0.0, 1.0);
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
```

`color::luminance` must be `pub(crate)` or `pub` within the crate for the tests' `crate::color::luminance` path to resolve — it is already `pub` inside a private module, which is correct.

- [ ] **Step 4: Run**

Run: `cargo test -p eink-photo`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/eink-photo/src/colorops.rs crates/eink-photo/src/lib.rs
git commit -m "feat(eink-photo): vibrance, saturation, grayscale, and invert"
```

---

## Task 7: Preset, `palette_aware` endpoints, and the assembled pipeline

**Files:**
- Create: `crates/eink-photo/src/preset.rs`
- Modify: `crates/eink-photo/src/lib.rs` (`process` becomes real)

**Interfaces:**
- Produces:
  - `eink_photo::palette_endpoints(palette: &[(u8, u8, u8)]) -> Option<(f32, f32)>` — the tone-domain luminance of the darkest and lightest palette entries; `None` for an empty palette
  - `preset::apply_base_layer(params: &Params) -> Params` — returns `params` with unset fields filled from the preset
  - `eink_photo::process` now runs the full order

- [ ] **Step 1: Write the failing tests**

In `preset.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Params, Preset};

    #[test]
    fn preset_none_changes_nothing() {
        let p = Params { contrast: Some(10.0), ..Default::default() };
        let out = apply_base_layer(&p);
        assert_eq!(out, p);
    }

    #[test]
    fn eink_preset_fills_unset_fields() {
        let out = apply_base_layer(&Params { preset: Preset::Eink, ..Default::default() });
        assert_eq!(out.auto_levels, Some(true));
        assert!(out.shadows.unwrap() > 0.0);
        assert!(out.highlights.unwrap() < 0.0);
        assert!(out.clarity.unwrap() > 0.0);
        assert!(out.vibrance.unwrap() > 0.0);
        assert!(out.sharpen.is_some());
    }

    #[test]
    fn an_explicit_value_overrides_the_preset() {
        // The rule that keeps the preset predictable as its values are
        // retuned: `{ preset = "eink", clarity = 0 }` means no clarity.
        let out = apply_base_layer(&Params {
            preset: Preset::Eink,
            clarity: Some(0.0),
            ..Default::default()
        });
        assert_eq!(out.clarity, Some(0.0));
        // ...while its neighbours still come from the preset.
        assert!(out.vibrance.unwrap() > 0.0);
    }

    #[test]
    fn palette_endpoints_finds_the_darkest_and_lightest() {
        let palette = [(10, 10, 10), (232, 230, 224), (168, 58, 48), (63, 122, 69)];
        let (lo, hi) = palette_endpoints(&palette).expect("non-empty palette");
        assert!(lo < 0.1, "darkest entry should be near black: {lo}");
        assert!(hi > 0.85, "lightest entry should be near white: {hi}");
        assert!(lo < hi);
    }

    #[test]
    fn palette_endpoints_of_an_empty_palette_is_none() {
        assert!(palette_endpoints(&[]).is_none());
    }
}
```

In `lib.rs`'s test module:

```rust
    #[test]
    fn process_applies_operations_in_the_fixed_order() {
        // Sharpening must run AFTER the tonal work: if it ran first, a
        // subsequent endpoint compression would flatten the edges it created.
        // Compare a full run against a hand-ordered wrong one.
        let mut wide = vec![0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0];
        let params = Params {
            output_endpoints: Some((0.05, 0.92)),
            ..Default::default()
        };
        process(&mut wide, 2, 1, &params).unwrap();
        assert!((wide[0] - 0.05).abs() < 1e-3, "black must land on the panel black: {}", wide[0]);
        assert!((wide[3] - 0.92).abs() < 1e-3, "white must land on the panel white: {}", wide[3]);
    }

    #[test]
    fn process_rejects_an_out_of_range_slider() {
        let mut p = vec![0.5f32; 3];
        let err = process(&mut p, 1, 1, &Params { exposure: Some(30.0), ..Default::default() })
            .unwrap_err();
        assert!(matches!(err, PhotoError::OutOfRange { field: "exposure", .. }));
    }

    #[test]
    fn process_with_the_eink_preset_runs_end_to_end() {
        let mut p: Vec<f32> = (0..64 * 64 * 3).map(|i| (i % 255) as f32 / 255.0).collect();
        process(&mut p, 64, 64, &Params { preset: Preset::Eink, ..Default::default() })
            .expect("preset must run");
        assert!(p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p eink-photo`
Expected: compile errors and, for `process_applies_operations_in_the_fixed_order`, a failure — `process` is still a pass-through.

- [ ] **Step 3: Implement `preset.rs`**

```rust
//! The `eink` base layer and device-aware endpoint derivation.

use crate::{Params, Preset, Sharpen};

/// Tone-domain luminance of the darkest and lightest palette entries.
///
/// `palette_aware` uses this to stop the tone mapping spending range the
/// panel physically cannot show: there is no point mapping a shadow to 0.0
/// when the panel's blackest ink measures 0.05.
pub fn palette_endpoints(palette: &[(u8, u8, u8)]) -> Option<(f32, f32)> {
    if palette.is_empty() {
        return None;
    }
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for &(r, g, b) in palette {
        let l = crate::color::luminance(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        lo = lo.min(l);
        hi = hi.max(l);
    }
    Some((lo, hi))
}

/// Fill unset fields from the preset. Explicit values always win — that is
/// the rule that keeps `preset` predictable as its values are retuned.
pub fn apply_base_layer(params: &Params) -> Params {
    let mut out = params.clone();
    if params.preset == Preset::None {
        return out;
    }

    // Starting values, chosen for a low-dynamic-range six-colour panel.
    // These are an implementation detail and are expected to move as they
    // are measured against the calibration screens — never depend on the
    // exact numbers, only on their signs.
    out.auto_levels = out.auto_levels.or(Some(true));
    out.shadows = out.shadows.or(Some(20.0));
    out.highlights = out.highlights.or(Some(-20.0));
    out.clarity = out.clarity.or(Some(25.0));
    out.vibrance = out.vibrance.or(Some(30.0));
    out.sharpen = out.sharpen.or(Some(Sharpen { amount: 30.0, radius: 1.0 }));
    out
}
```

- [ ] **Step 4: Implement the real `process`**

Replace the pass-through body in `lib.rs`:

```rust
pub fn process(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    params: &Params,
) -> Result<(), PhotoError> {
    let expected = width * height * 3;
    if pixels.len() != expected {
        return Err(PhotoError::BufferLength { expected, got: pixels.len() });
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
    let check = |field: &'static str, v: Option<f32>, min: f32, max: f32| -> Result<(), PhotoError> {
        match v {
            Some(x) if x < min || x > max => {
                Err(PhotoError::OutOfRange { field, value: x, min, max })
            }
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
```

Add `mod colorops; mod preset; mod presence;` and `pub use preset::palette_endpoints;` to `lib.rs`.

- [ ] **Step 5: Run**

Run: `cargo test -p eink-photo`
Expected: all PASS, including `default_params_are_a_no_op` from Task 1 — if that one now fails, an operation is running when it should not, which is exactly what it exists to catch.

- [ ] **Step 6: Commit**

```bash
git add crates/eink-photo/src/
git commit -m "feat(eink-photo): eink preset, palette-aware endpoints, and the assembled pipeline"
```

---

## Task 8: The codec and geometry layer

**Files:**
- Create: `src/services/image_process.rs`
- Modify: `src/services/mod.rs` (`pub mod image_process;` + re-export), `Cargo.toml` (`image` dependency)

**Interfaces:**
- Produces:

```rust
pub struct GeometryOpts {
    pub crop: Option<(f32, f32, f32, f32)>, // x, y, w, h — normalised
    pub fit: Fit,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub enum Fit { Cover, Contain, Stretch, None }

pub enum OutputFormat { Png, Jpeg { quality: u8 } }

pub fn process_image(
    bytes: &[u8],
    geometry: &GeometryOpts,
    params: &eink_photo::Params,
    format: OutputFormat,
) -> Result<(String, u32, u32), ImageProcessError>;
```

Returns the `data:` URI plus the result's pixel width and height.

- [ ] **Step 1: Add the dependency**

In the root `Cargo.toml`, under `[dependencies]`:

```toml
# Decoders (png, zune-jpeg, image-webp) are already in the tree via resvg;
# this adds the façade, not a new decoder stack. Default features are off to
# keep the format list to what a screen actually embeds.
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
eink-photo = { path = "crates/eink-photo" }
```

Run `cargo build` and confirm the lockfile does not gain a second copy of `png` or `zune-jpeg` at a different major version. If it does, **stop and report** — a duplicated decoder is a real cost and the feature list may need adjusting.

- [ ] **Step 2: Write the failing tests**

At the bottom of `src/services/image_process.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny in-memory PNG, built rather than checked in as a fixture.
    fn test_png(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let v = (((x + y) % 2) * 200 + 27) as u8;
            *px = image::Rgb([v, v / 2, 255 - v]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn default_geometry() -> GeometryOpts {
        GeometryOpts { crop: None, fit: Fit::Cover, width: None, height: None }
    }

    #[test]
    fn no_geometry_keeps_the_source_dimensions() {
        let (_uri, w, h) = process_image(
            &test_png(37, 21),
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (37, 21));
    }

    #[test]
    fn cover_fills_the_box_exactly() {
        let g = GeometryOpts { width: Some(80), height: Some(48), ..default_geometry() };
        let (_uri, w, h) = process_image(
            &test_png(200, 200),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (80, 48));
    }

    #[test]
    fn contain_fits_inside_the_box_preserving_aspect() {
        let g = GeometryOpts { fit: Fit::Contain, width: Some(80), height: Some(48), ..default_geometry() };
        let (_uri, w, h) = process_image(
            &test_png(200, 100),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert!(w <= 80 && h <= 48, "must fit inside: {w}x{h}");
        assert!(w == 80 || h == 48, "must touch one edge: {w}x{h}");
    }

    #[test]
    fn one_dimension_scales_the_other_by_aspect() {
        let g = GeometryOpts { width: Some(100), height: None, ..default_geometry() };
        let (_uri, w, h) = process_image(
            &test_png(200, 100),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (100, 50));
    }

    #[test]
    fn fit_none_ignores_the_box() {
        let g = GeometryOpts { fit: Fit::None, width: Some(10), height: Some(10), ..default_geometry() };
        let (_uri, w, h) = process_image(
            &test_png(37, 21),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (37, 21));
    }

    #[test]
    fn crop_selects_a_normalised_region() {
        let g = GeometryOpts { crop: Some((0.25, 0.0, 0.5, 1.0)), fit: Fit::None, ..default_geometry() };
        let (_uri, w, h) = process_image(
            &test_png(100, 40),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (50, 40));
    }

    #[test]
    fn crop_outside_the_image_is_an_error() {
        let g = GeometryOpts { crop: Some((0.9, 0.0, 0.5, 1.0)), ..default_geometry() };
        let err = process_image(
            &test_png(100, 40),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::BadCrop(_)));
    }

    #[test]
    fn output_is_a_png_data_uri() {
        let (uri, _, _) = process_image(
            &test_png(8, 8),
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert!(uri.starts_with("data:image/png;base64,"), "{}", &uri[..40.min(uri.len())]);
    }

    #[test]
    fn jpeg_output_is_smaller_and_labelled_correctly() {
        let src = test_png(64, 64);
        let (png_uri, _, _) = process_image(
            &src, &default_geometry(), &eink_photo::Params::default(), OutputFormat::Png,
        ).unwrap();
        let (jpeg_uri, _, _) = process_image(
            &src, &default_geometry(), &eink_photo::Params::default(),
            OutputFormat::Jpeg { quality: 90 },
        ).unwrap();
        assert!(jpeg_uri.starts_with("data:image/jpeg;base64,"));
        let _ = png_uri; // size comparison is content-dependent; the label is the contract
    }

    #[test]
    fn undecodable_input_is_an_error_not_a_panic() {
        let err = process_image(
            b"not an image at all",
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::Decode(_)));
    }

    #[test]
    fn an_oversized_source_is_rejected_before_decoding() {
        // A 2-byte "image" cannot exceed the byte cap; build the assertion
        // around the cap itself so it cannot silently stop testing anything.
        let huge = vec![0u8; MAX_SOURCE_BYTES + 1];
        let err = process_image(
            &huge, &default_geometry(), &eink_photo::Params::default(), OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::TooLarge { .. }));
    }

    #[test]
    fn an_oversized_output_box_is_rejected() {
        let g = GeometryOpts { width: Some(MAX_OUTPUT_DIM + 1), height: Some(10), ..default_geometry() };
        let err = process_image(
            &test_png(8, 8), &g, &eink_photo::Params::default(), OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::TooLarge { .. }));
    }

    #[test]
    fn sharpening_runs_after_the_downscale() {
        // Ordering is observable: sharpening a 400px source that is then
        // downscaled to 100px would lose the effect entirely. Because
        // sharpening happens after the resize, the two runs must differ.
        let src = test_png(400, 400);
        let g = GeometryOpts { width: Some(100), height: Some(100), ..default_geometry() };
        let plain = process_image(&src, &g, &eink_photo::Params::default(), OutputFormat::Png).unwrap();
        let sharp = process_image(
            &src,
            &g,
            &eink_photo::Params {
                sharpen: Some(eink_photo::Sharpen { amount: 100.0, radius: 1.0 }),
                ..Default::default()
            },
            OutputFormat::Png,
        )
        .unwrap();
        assert_ne!(plain.0, sharp.0, "sharpening at output resolution must change the result");
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test --lib services::image_process -- --test-threads=4`
Expected: module does not exist.

- [ ] **Step 4: Implement**

```rust
//! Decode, crop, resize, tone-map, and re-encode an image for embedding in an
//! SVG. Steps 1-3 and 17 of the pipeline described in
//! `docs/superpowers/specs/2026-08-06-lua-colors-and-image-ops-design.md`;
//! steps 4-16 live in the dependency-free `eink-photo` crate.

use base64::Engine as _;
use image::GenericImageView as _;

/// Largest encoded input accepted, checked before any decoding.
pub const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
/// Largest source area accepted, enforced through `image::Limits`.
pub const MAX_SOURCE_PIXELS: u64 = 40_000_000;
/// Largest output dimension accepted, in pixels.
pub const MAX_OUTPUT_DIM: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
    Stretch,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct GeometryOpts {
    /// x, y, w, h — normalised 0..=1, relative to the decoded image.
    pub crop: Option<(f32, f32, f32, f32)>,
    pub fit: Fit,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Png,
    Jpeg { quality: u8 },
}

#[derive(Debug, thiserror::Error)]
pub enum ImageProcessError {
    #[error("could not decode the image: {0}")]
    Decode(String),
    #[error("could not encode the image: {0}")]
    Encode(String),
    #[error("invalid crop: {0}")]
    BadCrop(String),
    #[error("{what} exceeds the limit ({value} > {limit})")]
    TooLarge { what: &'static str, value: u64, limit: u64 },
    #[error("{0}")]
    Photo(String),
}

pub fn process_image(
    bytes: &[u8],
    geometry: &GeometryOpts,
    params: &eink_photo::Params,
    format: OutputFormat,
) -> Result<(String, u32, u32), ImageProcessError> {
    // Guard 1: encoded size, before touching a decoder.
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ImageProcessError::TooLarge {
            what: "source image size",
            value: bytes.len() as u64,
            limit: MAX_SOURCE_BYTES as u64,
        });
    }

    // Guard 2: requested output box, before allocating anything.
    for dim in [geometry.width, geometry.height].into_iter().flatten() {
        if dim > MAX_OUTPUT_DIM {
            return Err(ImageProcessError::TooLarge {
                what: "output dimension",
                value: dim as u64,
                limit: MAX_OUTPUT_DIM as u64,
            });
        }
    }

    // Guard 3: decoded area, enforced by the decoder itself so a
    // decompression bomb never gets allocated.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_OUTPUT_DIM.max(20_000));
    limits.max_image_height = Some(MAX_OUTPUT_DIM.max(20_000));
    limits.max_alloc = Some(MAX_SOURCE_PIXELS * 4);

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageProcessError::Decode(e.to_string()))?;
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| ImageProcessError::Decode(e.to_string()))?;

    // Step 2 — crop.
    let img = match geometry.crop {
        None => img,
        Some((cx, cy, cw, ch)) => {
            if !(0.0..=1.0).contains(&cx)
                || !(0.0..=1.0).contains(&cy)
                || cw <= 0.0
                || ch <= 0.0
                || cx + cw > 1.0 + 1e-6
                || cy + ch > 1.0 + 1e-6
            {
                return Err(ImageProcessError::BadCrop(format!(
                    "x={cx}, y={cy}, w={cw}, h={ch} does not lie within the image"
                )));
            }
            let (w, h) = img.dimensions();
            let px = (cx * w as f32).round() as u32;
            let py = (cy * h as f32).round() as u32;
            let pw = ((cw * w as f32).round() as u32).max(1).min(w - px);
            let ph = ((ch * h as f32).round() as u32).max(1).min(h - py);
            img.crop_imm(px, py, pw, ph)
        }
    };

    // Step 3 — fit / resize.
    let img = resize(img, geometry)?;
    let (out_w, out_h) = img.dimensions();

    // Steps 4-16 — hand the tone domain to eink-photo.
    let rgb = img.to_rgb8();
    let mut pixels: Vec<f32> = rgb.as_raw().iter().map(|&b| b as f32 / 255.0).collect();
    eink_photo::process(&mut pixels, out_w as usize, out_h as usize, params)
        .map_err(|e| ImageProcessError::Photo(e.to_string()))?;

    let bytes_out: Vec<u8> = pixels
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    let out = image::RgbImage::from_raw(out_w, out_h, bytes_out)
        .ok_or_else(|| ImageProcessError::Encode("pixel buffer size mismatch".to_string()))?;

    // Step 17 — encode.
    let mut buf = std::io::Cursor::new(Vec::new());
    let (mime, ()) = match format {
        OutputFormat::Png => {
            image::DynamicImage::ImageRgb8(out)
                .write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| ImageProcessError::Encode(e.to_string()))?;
            ("image/png", ())
        }
        OutputFormat::Jpeg { quality } => {
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
            enc.encode_image(&out)
                .map_err(|e| ImageProcessError::Encode(e.to_string()))?;
            ("image/jpeg", ())
        }
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok((format!("data:{mime};base64,{b64}"), out_w, out_h))
}

fn resize(
    img: image::DynamicImage,
    g: &GeometryOpts,
) -> Result<image::DynamicImage, ImageProcessError> {
    if g.fit == Fit::None {
        return Ok(img);
    }
    let (sw, sh) = img.dimensions();
    let (tw, th) = match (g.width, g.height) {
        (None, None) => return Ok(img),
        // One dimension given: scale the other by aspect ratio, in every mode.
        (Some(w), None) => (w, ((w as f32 / sw as f32) * sh as f32).round().max(1.0) as u32),
        (None, Some(h)) => (((h as f32 / sh as f32) * sw as f32).round().max(1.0) as u32, h),
        (Some(w), Some(h)) => (w, h),
    };

    Ok(match g.fit {
        Fit::None => unreachable!("handled above"),
        Fit::Stretch => img.resize_exact(tw, th, image::imageops::FilterType::Lanczos3),
        Fit::Contain => img.resize(tw, th, image::imageops::FilterType::Lanczos3),
        Fit::Cover => img.resize_to_fill(tw, th, image::imageops::FilterType::Lanczos3),
    })
}
```

**Note on EXIF orientation:** `image` 0.25's `ImageReader` does not apply EXIF orientation automatically. Check whether `DynamicImage::apply_orientation` and `ImageDecoder::orientation` are available in the resolved version; if they are, read the orientation before `decode()` and apply it. **If they are not, say so in your report and leave orientation unhandled** rather than hand-rolling an EXIF parser — an unrotated phone photo is a visible annoyance, a bespoke EXIF parser is a security surface.

- [ ] **Step 5: Register the module**

In `src/services/mod.rs`, add `pub mod image_process;` in alphabetical position and re-export:

```rust
pub use image_process::{process_image, Fit, GeometryOpts, ImageProcessError, OutputFormat};
```

- [ ] **Step 6: Run**

Run: `cargo test --lib services::image_process -- --test-threads=4`
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/services/image_process.rs src/services/mod.rs
git diff --cached --stat
git commit -m "feat: add image_process — decode, crop, fit, tone-map, and encode"
```

---

## Task 9: The `image_process` Lua global

**Files:**
- Modify: `src/services/lua_runtime.rs`
- Test: `tests/lua_api_test.rs`

**Interfaces:**
- Consumes: `services::image_process::{process_image, GeometryOpts, Fit, OutputFormat}`, `eink_photo::{Params, Preset, Sharpen, palette_endpoints}`, and (for `palette_aware`) `DeviceContext::colors_actual` from **Plan A Task 1**.
- Produces: the Lua global `image_process(bytes, opts) -> (data_uri, width, height)`.

**`palette_aware` sourcing, exactly:** `device.colors_actual`, then `device.colors`, then nothing. With nothing available it is a **logged no-op** via `log_warn`, never an error — a screen using the `eink` preset must still render on an uncalibrated panel.

- [ ] **Step 1: Write the failing tests**

`setup_test_env` writes text files only, so each test writes its own generated PNG into the same temp dir afterwards. `TempDir::path()` is that directory, and `read_asset` resolves against it.

```rust
/// A small generated PNG. Built, never committed — a binary fixture in the
/// repo is a fixture nobody can inspect or regenerate.
fn tiny_png(w: u32, h: u32) -> Vec<u8> {
    let mut img = image::RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        // Muted, low-saturation content: the case image_process exists for.
        let v = (120 + ((x + y) % 3) * 12) as u8;
        *px = image::Rgb([v + 8, v, v.saturating_sub(6)]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// setup_test_env + a generated `tiny.png` alongside the script.
fn setup_image_env(script_name: &str, script: &str) -> (TempDir, Arc<AssetLoader>) {
    let (temp_dir, loader) = setup_test_env(&[(script_name, script)]);
    std::fs::write(temp_dir.path().join("tiny.png"), tiny_png(40, 20))
        .expect("write test png");
    (temp_dir, loader)
}

#[test]
fn test_image_process_returns_a_data_uri_and_dimensions() {
    let script = r#"
        local png = read_asset("tiny.png")
        local src, w, h = image_process(png, { width = 20, height = 10, fit = "cover" })
        return {
            data = {
                is_png = string.sub(src, 1, 22) == "data:image/png;base64,",
                w = w,
                h = h,
            },
            refresh_rate = 60
        }
    "#;

    let (_temp, loader) = setup_image_env("test_img.lua", script);
    let runtime = LuaRuntime::new(loader);
    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_img.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("script must run");

    assert!(result.data["is_png"].as_bool().unwrap(), "must be a png data URI");
    assert_eq!(result.data["w"].as_i64().unwrap(), 20);
    assert_eq!(result.data["h"].as_i64().unwrap(), 10);
}

#[test]
fn test_image_process_rejects_an_out_of_range_slider() {
    // exposure = 30 is a typo for 3.0. It must fail loudly rather than
    // saturating to a white rectangle.
    let script = r#"
        local png = read_asset("tiny.png")
        local ok, err = pcall(function()
            return image_process(png, { exposure = 30 })
        end)
        return { data = { ok = ok, err = tostring(err) }, refresh_rate = 60 }
    "#;

    let (_temp, loader) = setup_image_env("test_img_range.lua", script);
    let runtime = LuaRuntime::new(loader);
    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_img_range.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("script must run — the pcall catches the error");

    assert!(!result.data["ok"].as_bool().unwrap(), "must have raised");
    let err = result.data["err"].as_str().unwrap();
    assert!(err.contains("exposure"), "the error must name the field: {err}");
}

#[test]
fn test_image_process_preset_runs_without_a_palette() {
    // palette_aware on a device with no palette at all: a logged no-op, not
    // an error. A screen using the eink preset must render everywhere.
    let script = r#"
        local png = read_asset("tiny.png")
        local src = image_process(png, { preset = "eink", palette_aware = true })
        return { data = { ok = src ~= nil }, refresh_rate = 60 }
    "#;

    let (_temp, loader) = setup_image_env("test_img_preset.lua", script);
    let runtime = LuaRuntime::new(loader);
    let ctx = DeviceContext {
        mac: "TE:ST:00:00:00:00".to_string(),
        width: Some(800),
        height: Some(480),
        colors: None,
        colors_actual: None,
        ..Default::default()
    };
    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_img_preset.lua"),
            &HashMap::new(),
            Some(&ctx),
            None,
        )
        .expect("script must run");

    assert!(result.data["ok"].as_bool().unwrap());
    assert!(
        result.logs.iter().any(|l| l.contains("palette_aware")),
        "the no-op must be logged, not silent: {:?}",
        result.logs
    );
}

#[test]
fn test_image_process_rejects_an_unknown_preset() {
    // A typo'd preset that silently does nothing ships looking like it worked.
    let script = r#"
        local png = read_asset("tiny.png")
        local ok = pcall(function() return image_process(png, { preset = "vivid" }) end)
        return { data = { ok = ok }, refresh_rate = 60 }
    "#;

    let (_temp, loader) = setup_image_env("test_img_preset_bad.lua", script);
    let runtime = LuaRuntime::new(loader);
    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_img_preset_bad.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("script must run");

    assert!(!result.data["ok"].as_bool().unwrap(), "unknown preset must raise");
}
```

`test_image_process_preset_runs_without_a_palette` needs `DeviceContext::colors_actual`, which comes from **Plan A Task 1**. If Plan A is not merged, drop the `colors_actual: None,` line — the rest of the test stands on its own — and note it in your report.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test lua_api_test image_process -- --test-threads=4`
Expected: Lua error, `attempt to call a nil value (global 'image_process')`.

- [ ] **Step 3: Implement the binding**

In `setup_globals`, alongside the other globals. The `DeviceContext`'s palette is captured for `palette_aware`:

```rust
        // image_process(bytes, opts) -> data_uri, width, height
        //
        // One call, fixed order (see the eink-photo crate docs). Raises a Lua
        // error on failure, matching http_get's contract and the `pcall`
        // idiom the examples use.
        let ctx_palette_hex: Option<Vec<String>> = ctx
            .and_then(|c| c.colors_actual.clone().or_else(|| c.colors.clone()));
        let img_log_sink = log_sink.clone();
        let image_process = lua.create_function(
            move |_, (bytes, opts): (mlua::String, Option<Table>)| {
                let opts = opts;
                let (geometry, params, format) =
                    parse_image_opts(opts.as_ref(), ctx_palette_hex.as_deref(), &img_log_sink)
                        .map_err(mlua::Error::external)?;
                let (uri, w, h) = crate::services::image_process::process_image(
                    &bytes.as_bytes(),
                    &geometry,
                    &params,
                    format,
                )
                .map_err(mlua::Error::external)?;
                Ok((uri, w, h))
            },
        )?;
        globals.set("image_process", image_process)?;
```

Add a free function in the same module (it is long; keep it out of `setup_globals`):

```rust
/// Translate a Lua options table into the three typed structs the pipeline
/// needs. Unknown `preset` and `fit` values are errors, never silent no-ops:
/// a typo that silently does nothing is worse than one that fails loudly.
fn parse_image_opts(
    opts: Option<&Table>,
    palette_hex: Option<&[String]>,
    log_sink: &Arc<Mutex<Vec<String>>>,
) -> Result<
    (
        crate::services::image_process::GeometryOpts,
        eink_photo::Params,
        crate::services::image_process::OutputFormat,
    ),
    String,
> {
    use crate::services::image_process::{Fit, GeometryOpts, OutputFormat};

    let Some(t) = opts else {
        return Ok((GeometryOpts::default(), eink_photo::Params::default(), OutputFormat::Png));
    };

    let num = |k: &str| -> Option<f32> { t.get::<f32>(k).ok() };
    let flag = |k: &str| -> Option<bool> {
        match t.get::<Value>(k) {
            Ok(Value::Boolean(b)) => Some(b),
            _ => None,
        }
    };

    // --- geometry ---
    let crop = match t.get::<Table>("crop") {
        Ok(c) => Some((
            c.get::<f32>("x").unwrap_or(0.0),
            c.get::<f32>("y").unwrap_or(0.0),
            c.get::<f32>("w").map_err(|_| "crop.w is required".to_string())?,
            c.get::<f32>("h").map_err(|_| "crop.h is required".to_string())?,
        )),
        Err(_) => None,
    };
    let fit = match t.get::<String>("fit").ok().as_deref() {
        None | Some("cover") => Fit::Cover,
        Some("contain") => Fit::Contain,
        Some("stretch") => Fit::Stretch,
        Some("none") => Fit::None,
        Some(other) => {
            return Err(format!(
                "unknown fit {other:?}; expected cover, contain, stretch or none"
            ))
        }
    };
    let geometry = GeometryOpts {
        crop,
        fit,
        width: t.get::<u32>("width").ok(),
        height: t.get::<u32>("height").ok(),
    };

    // --- tone params ---
    let preset = match t.get::<String>("preset").ok().as_deref() {
        None | Some("none") => eink_photo::Preset::None,
        Some("eink") => eink_photo::Preset::Eink,
        Some(other) => return Err(format!("unknown preset {other:?}; expected eink or none")),
    };

    let curve = match t.get::<Table>("curve") {
        Ok(c) => {
            let mut pts = Vec::new();
            for i in 1..=c.raw_len() {
                let pair: Table = c
                    .raw_get(i)
                    .map_err(|_| "curve entries must be {input, output} pairs".to_string())?;
                let x: f32 = pair.raw_get(1).map_err(|_| "curve point missing input".to_string())?;
                let y: f32 = pair.raw_get(2).map_err(|_| "curve point missing output".to_string())?;
                pts.push((x, y));
            }
            Some(pts)
        }
        Err(_) => None,
    };

    let sharpen = match t.get::<Table>("sharpen") {
        Ok(s) => Some(eink_photo::Sharpen {
            amount: s.get::<f32>("amount").unwrap_or(40.0),
            radius: s.get::<f32>("radius").unwrap_or(1.0),
        }),
        Err(_) => None,
    };

    // --- palette_aware ---
    let output_endpoints = if flag("palette_aware").unwrap_or(false) {
        match palette_hex {
            Some(hex) if !hex.is_empty() => {
                let rgb = crate::api::display::parse_colors_header(&hex.join(","));
                eink_photo::palette_endpoints(&rgb)
            }
            _ => {
                push_log(
                    log_sink,
                    "[warn] image_process: palette_aware was requested but this device \
                     has no palette; ignoring it"
                        .to_string(),
                );
                None
            }
        }
    } else {
        None
    };

    let params = eink_photo::Params {
        preset,
        exposure: num("exposure"),
        temperature: num("temperature"),
        tint: num("tint"),
        auto_levels: flag("auto_levels"),
        blacks: num("blacks"),
        whites: num("whites"),
        highlights: num("highlights"),
        shadows: num("shadows"),
        contrast: num("contrast"),
        curve,
        clarity: num("clarity"),
        vibrance: num("vibrance"),
        saturation: num("saturation"),
        grayscale: flag("grayscale"),
        invert: flag("invert"),
        sharpen,
        output_endpoints,
    };

    // --- output format ---
    let format = match t.get::<String>("format").ok().as_deref() {
        None | Some("png") => OutputFormat::Png,
        Some("jpeg") | Some("jpg") => OutputFormat::Jpeg {
            quality: t.get::<u8>("quality").unwrap_or(90),
        },
        Some(other) => return Err(format!("unknown format {other:?}; expected png or jpeg")),
    };

    Ok((geometry, params, format))
}
```

**Watch for two things while implementing:**
1. `setup_globals` may not have `ctx` in scope under that name at the point you add this — it is the `device_ctx: Option<&DeviceContext>` parameter. Capture the palette *before* the closure, since the closure is `'static`.
2. `mlua::String::as_bytes()` returns a borrowed view; make sure it lives long enough for the `process_image` call. Bind it to a local first if the borrow checker objects.

- [ ] **Step 4: Run**

Run: `cargo test --test lua_api_test image_process -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Full check**

Run: `make check`
Expected: clean. Report the total.

- [ ] **Step 6: Commit**

```bash
git add src/services/lua_runtime.rs tests/lua_api_test.rs
git diff --cached --stat
git commit -m "feat: add the image_process Lua global"
```

---

## Task 10: The end-to-end test that proves the point

**Files:**
- Test: `tests/image_process_e2e_test.rs` (create)

This is the assertion that the feature does what it *exists* to do, rather than merely that it runs. Everything before this proves the maths; this proves the maths helps.

**Interfaces:**
- Consumes: everything above, plus `ScreenStore::render` (harness in `tests/common/store.rs`).

- [ ] **Step 1: Write the test**

```rust
//! Proves the image pipeline achieves its purpose: that `vibrance` measurably
//! increases the share of pixels landing on chromatic palette entries after
//! dithering. Every other test in this feature proves an operation behaves as
//! specified; this one proves the specification was worth implementing.

mod common;

/// Count how many pixels of a dithered PNG land on a non-grey palette entry.
fn chromatic_share(png: &[u8]) -> f64 {
    let img = image::load_from_memory(png).expect("valid png").to_rgb8();
    let total = img.pixels().len() as f64;
    let chromatic = img
        .pixels()
        .filter(|p| {
            let [r, g, b] = p.0;
            let max = r.max(g).max(b) as i32;
            let min = r.min(g).min(b) as i32;
            // A palette entry is chromatic when its channels disagree.
            max - min > 24
        })
        .count() as f64;
    chromatic / total
}

/// A muted, low-saturation photograph — the case this feature exists for.
fn muted_photo(w: u32, h: u32) -> Vec<u8> {
    let mut img = image::RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        // Gentle tonal variation with only a whisper of colour: every pixel
        // sits close to grey, so nothing reaches a chromatic palette entry
        // without help.
        let base = 90 + ((x / 8 + y / 8) % 5) as u8 * 14;
        px.0 = [base.saturating_add(10), base, base.saturating_sub(8)];
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// Write a screen into the fixture's writable `local` repo whose script
/// embeds `photo.png` after running it through `image_process` with the
/// given vibrance.
fn write_photo_screen(dir: &std::path::Path, name: &str, vibrance: f32) {
    let screen_dir = dir.join("local").join(name);
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(screen_dir.join("photo.png"), muted_photo(200, 120)).unwrap();
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!("name: {name}\ndescription: photo test\n"),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("script.lua"),
        format!(
            r#"
            local photo = read_asset("photo.png")
            local src, w, h = image_process(photo, {{
                width = 200, height = 120, fit = "stretch",
                vibrance = {vibrance},
            }})
            return {{ data = {{ src = src, w = w, h = h }}, refresh_rate = 3600 }}
            "#
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{{ device.width }}" height="{{ device.height }}">
  <image x="0" y="0" width="{{ data.w }}" height="{{ data.h }}" href="{{ data.src }}"/>
</svg>"#,
    )
    .unwrap();
}

#[test]
fn vibrance_increases_the_chromatic_share_of_the_dithered_output() {
    // Without vibrance, muted colours never reach a chromatic palette entry
    // and the whole image dithers into greys — the exact failure this
    // feature exists to fix.
    let tmp = tempfile::tempdir().unwrap();
    let store = common::store::build_store(tmp.path(), &[]);
    write_photo_screen(tmp.path(), "dull", 0.0);
    write_photo_screen(tmp.path(), "vivid", 80.0);

    // The render palette must contain chromatic entries — see the note below.
    let opts = || byonk::services::screen_store::RenderOpts {
        model: "og".to_string(),
        width: Some(200),
        height: Some(120),
        timestamp: Some(1_750_000_000),
        ..Default::default()
    };

    let render = |name: &str| {
        let r = store.render(&format!("local/{name}"), opts());
        assert!(r.error.is_none(), "{name} must render: {:?} / {:?}", r.error, r.log);
        r.png
    };

    let dull = chromatic_share(&render("dull"));
    let vivid = chromatic_share(&render("vivid"));

    // Guard against the whole test silently measuring nothing.
    assert!(vivid > 0.0, "the render palette has no chromatic entries; this test proves nothing");

    assert!(
        vivid > dull * 1.2,
        "vibrance must push colour onto the palette: {dull} -> {vivid}"
    );
}
```

> **Two things to resolve while writing this, and report what you found:**
>
> 1. **The render palette must contain chromatic entries.** `RenderOpts` has no `colors` field — the official palette comes from the model/panel chain. Model `"og"` may resolve to four greys, in which case *neither* render can produce a chromatic pixel and the test measures nothing. Check `resolve_query_palette` for a colour model (`"og_4clr"`, or whatever the tree actually has) and use it, or configure a panel in the fixture's `config.yaml`. **Do not let this test pass with both shares at zero** — assert `dull >= 0.0 && vivid > 0.0` as a guard against exactly that.
> 2. `common::store::build_store` scaffolds screens through `create_screen`; here the screens are written directly instead, which is what its doc comment says callers may do. Confirm the disk source picks up directly-written screens without a loader rebuild — the doc comment says it stats and reads on every access, so it should.

- [ ] **Step 2: Flesh it out and run it**

Run: `cargo test --test image_process_e2e_test -- --test-threads=4 --nocapture`

**Before accepting this test, break it deliberately:** set `vibrance` to `Some(0.0)` in both runs and confirm it fails. A test that passes either way tells you nothing.

- [ ] **Step 3: If the assertion does not hold**

That is a **finding, not a test to weaken.** Report it with the two measured numbers. Possible real causes, in order of likelihood: the render palette has no chromatic entries (pass `panel` or `colors_actual` with colour in it); the source image is already saturated enough that vibrance's dull-pixel weighting barely acts (make the source more muted); the chromatic threshold of 24 is wrong for the palette in use. Do **not** lower the `1.2` ratio to make it pass without saying so.

- [ ] **Step 4: Commit**

```bash
git add tests/image_process_e2e_test.rs
git commit -m "test: prove image_process vibrance increases the dithered chromatic share"
```

---

## Task 11: Documentation and the `gphoto` example

**Files:**
- Modify: `docs/src/api/lua-api.md`
- Modify: `screens/examples/gphoto/script.lua`
- Modify: `CHANGES.md`

- [ ] **Step 1: Document `image_process`**

Add a new top-level section to `docs/src/api/lua-api.md`, after `## Asset Functions` (which is where `read_asset` and `base64_encode` live, the two functions authors reach for just before this one):

````markdown
## Image Functions

### image_process(bytes, options)

Prepares a photograph for an e-ink panel and returns it as a `data:` URI ready
to drop into an SVG `<image href="...">`.

An e-ink panel is a low-dynamic-range display with a handful of colours. A
photograph sent to it untouched loses its shadows to a black sink, blows its
highlights to paper white, and desaturates until nothing reaches a coloured
palette entry. These controls are the ones that fix that.

```lua
local photo = http_get("https://example.com/photo.jpg")
local src, w, h = image_process(photo, {
  preset = "eink",
  fit    = "cover",
  width  = layout.width,
  height = layout.height,
})

return { data = { image_src = src, image_w = w, image_h = h } }
```

Returns three values: the data URI, and the result's width and height in
pixels.

**All options are optional.** `image_process(bytes, {})` decodes and
re-encodes without changing anything.

| Option | Range | Effect |
|---|---|---|
| `crop` | `{x, y, w, h}`, 0–1 | Select a region before anything else happens |
| `fit` | `cover` \| `contain` \| `stretch` \| `none` | How the image meets `width`/`height`. Default `cover` |
| `width`, `height` | pixels | Target size. Omit both to keep the source size; give one and the other follows the aspect ratio |
| `exposure` | −5…5 | Stops of exposure |
| `temperature` | −100…100 | Positive is warmer |
| `tint` | −100…100 | Positive is greener |
| `auto_levels` | boolean | Stretch the histogram to the full range first |
| `blacks`, `whites` | −100…100 | Where the endpoints land |
| `highlights`, `shadows` | −100…100 | Recover the two ends. The most useful pair on e-ink |
| `contrast` | −100…100 | S-curve about mid-grey |
| `curve` | `{{in, out}, ...}` | Point tone curve, for anything the sliders miss |
| `clarity` | −100…100 | Large-radius local contrast. The single op that makes a dithered photo readable |
| `vibrance` | −100…100 | Saturation weighted toward dull pixels, so muted colours reach a coloured palette entry |
| `saturation` | −100…100 | Global saturation |
| `grayscale`, `invert` | boolean | |
| `sharpen` | `{amount = 0…100, radius = 0.3…10}` | Applied last, at output size |
| `preset` | `"eink"` \| `"none"` | A tuned base layer. Any option you set yourself overrides it |
| `palette_aware` | boolean | Place the black and white points at the panel's real darkest and lightest, so the tone mapping does not spend range the panel cannot show |
| `format` | `"png"` \| `"jpeg"` | Default `png` |
| `quality` | 1–100 | JPEG only. Default 90 |

**Order is fixed** and not something you control: crop → resize → exposure →
white balance → levels and endpoints → highlights/shadows → contrast → curve
→ clarity → vibrance → saturation → grayscale/invert → sharpen. Resizing
first is what keeps a 24-megapixel source cheap; sharpening last is what makes
it mean anything.

**Errors are raised**, like `http_get`. Wrap in `pcall` if a screen should
survive a bad image:

```lua
local ok, src = pcall(function()
  return image_process(photo, { preset = "eink" })
end)
if not ok then
  log_error("image failed: " .. tostring(src))
end
```

Out-of-range values are errors rather than being silently clamped, so
`exposure = 30` (a typo for `3.0`) is caught instead of producing a white
rectangle.

**`palette_aware`** uses `device.colors_actual` when the panel is calibrated,
falling back to `device.colors`. On a device with neither it does nothing and
says so in the log — a screen using it still renders everywhere.
````

- [ ] **Step 2: Upgrade the `gphoto` example**

In `screens/examples/gphoto/script.lua`, replace:

```lua
local image_src = "data:image/jpeg;base64," .. base64_encode(img_data)
```

with:

```lua
-- Prepare the photo for the panel: a straight photograph loses its shadows
-- and desaturates until nothing reaches a coloured palette entry.
local image_src = image_process(img_data, {
  preset        = "eink",
  palette_aware = true,
  fit           = "cover",
  width         = width,
  height        = height,
})
```

Then check `screens/examples/gphoto/screen.svg` — it uses
`width="{{ data.width }}"` and `preserveAspectRatio="xMidYMid slice"`. Since
`image_process` now returns an image already at exactly `width`x`height`, the
`slice` behaviour is redundant but harmless. **Leave the SVG alone** unless the
render visibly changes; verify with `render_screen` or `/dev` and say what you
saw.

- [ ] **Step 3: CHANGES.md**

Under `## [Unreleased]`, `### Added` — user-facing wording only:

```markdown
- New `image_process()` function for Lua scripts: crop, resize, tone-map and
  sharpen a photograph before embedding it in a screen. A `preset = "eink"`
  one-liner handles the common case, and `palette_aware` tunes the result to
  what your panel can actually display.
- The `gphoto` example now uses it, so photo screens look markedly better out
  of the box.
```

- [ ] **Step 4: Build the docs**

Run: `make docs`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add docs/src/api/lua-api.md screens/examples/gphoto/script.lua CHANGES.md
git diff --cached --stat
git commit -m "docs: document image_process and use it in the gphoto example"
```

---

## Final verification

- [ ] `make check` — fmt clean, clippy clean, all tests pass. Report the exact count.
- [ ] `make docs` — clean.
- [ ] `cargo tree -p eink-photo` shows **no dependencies**. If it does not, that is a plan violation — report it.
- [ ] `git status` clean; no untracked file was swept into a commit.
- [ ] Render the `gphoto` example and **look at it** — via `render_screen` or `/dev`. Report whether the photo actually improved. This feature exists to make pictures look better, and no test can tell you whether it did.
- [ ] Report, honestly and specifically: any test you wrote that could pass against broken code; any tolerance or threshold you adjusted to make a test pass, and why; whether EXIF orientation ended up handled; and any step in this plan you found to be wrong.
