# Gamut Mapping Implementation Plan

> **SUPERSEDED IN PART — read this first.** Tasks 1–13 are implemented and
> merged into `feat/screen-store-authoring-core`. Two owner rulings landed
> afterwards and changed the design this plan describes:
>
> - **Ruling 16** — compression runs along a ray converging on **mid-grey**,
>   not at fixed lightness. Every statement below about "compressing chroma at
>   clamped lightness" is obsolete, including the Global Constraint "Hue is
>   never modified. Only chroma is compressed" — hue is still never modified,
>   but lightness now moves too.
> - **Ruling 17** — the knee default is **0.99**, not 0.6/0.8.
>
> The design of record is `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md`
> and the module docs in `crates/eink-dither/src/gamut/mapper.rs`. This file is
> kept for the task history; do not implement from it without checking both.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a screen mark continuous-tone regions with `data-byonk-tone="continuous"` so those pixels are compressed into the panel's physically reachable colour hull before dithering, trading colorimetric accuracy for preserved differences (gradients, hue order, local contrast).

**Architecture:** A new `gamut` module in `eink-dither` builds the convex hull of the palette's *actual* colours in linear RGB, precomputes a `Cmax(hue, lightness)` table by binary-searching that hull, derives one content-adaptive compression factor `R` per adaptation group, and compresses chroma through a knee curve that is strictly increasing and asymptotic to `Cmax`. On the byonk side, a streaming SVG rewriter produces a second document whose paint is white inside marked subtrees and black outside; rasterizing it with the *same* renderer yields a per-pixel mask. `render_to_palette_png` maps the masked pixels between rasterization and dithering.

**Tech Stack:** Rust 2021, `eink-dither` (workspace crate), `resvg`/`usvg` 0.46, `tiny-skia`, `quick-xml` (new dependency), `mlua` for the Lua knobs.

## Global Constraints

- **Opt-in, zero-cost when unused.** A document with no `data-byonk-tone` attribute must render byte-identically to today. Detect the attribute with a substring scan before doing any rewriting or extra rasterization.
- **The hull is built from the colours the ditherer targets** — `Palette::actual_linear(i)`, which falls back to official colours when no measured set resolved. The hull and the dither target must never diverge.
- **Hue is never modified.** Only chroma is compressed, at clamped lightness.
- **Strict monotonicity of the chroma map is a correctness property**, asserted on the float chroma function (8-bit sRGB quantisation legitimately collapses adjacent values; do not assert monotonicity on bytes).
- **Clamp linear RGB to `[0, 1]` before `Srgb::from`.** An `Oklch → Oklab → LinearRgb`
  round trip can land a hair outside the cube, and `color::lut::linear_to_srgb`
  carries an epsilon-free `debug_assert!(0.0..=1.0)` before clamping for release.
  So an unclamped conversion panics in debug (i.e. under `cargo test`) while
  behaving identically in release. Measured over a 421k-colour sweep with the
  six-ink palette at `R = 2.5`, the only excursion is pure white at `1.0000001`
  — one ULP. Excursions are larger and genuine, not fp noise, when chroma
  compression targets a hue outside sRGB (worst `-4.7e-4` at `r = 1.0`), where
  clamping is the correct response rather than a workaround. Task 6 does this in
  `map_color`; any later task converting `Oklch` back to `Srgb` must too.
- **No silent fallback on mask failure.** A mask rasterization error returns `RenderError`. Rendering something materially different while reporting success is the failure mode this design exists to avoid.
- Defaults: `knee = 0.8`, `amount = 1.0`, `max_compression = 2.5`. The knee
  matches the ACES 1.3 RGC threshold band and is measurement-backed; see the
  `GamutOptions::knee` doc comment in Task 6 for the derivation.
- English for all comments, identifiers and docs.
- Build with `make check` and **pass `timeout: 600000`** — it exceeds the Bash tool's 120 s default. Cap parallelism with `CARGO_BUILD_JOBS=2` (shared machine).
- **Never `git add -A` or `git add .`** — add by explicit path and verify `git diff --cached` before committing.
- All user-visible changes go in `CHANGES.md` under Unreleased.

## Deviations from the spec (read before starting)

The spec was written on 2026-08-07. Two of its statements are now stale, verified against the tree at `81ba62b`:

1. **"Exact-match pinning is disabled [inside marked regions]" and "`preserve_exact` is global today and must become an optional per-pixel mask" — both obsolete.** Exact-match pinning was removed from the crate entirely (see the doc comment at `crates/eink-dither/src/preprocess/preprocessor.rs:88-94`, "That is gone"). There is no `preserve_exact` anywhere in `eink-dither`. **No `eink-dither` API change for pinning is needed, and no task in this plan implements one.** The mask is still required — it selects which pixels get gamut-mapped.
2. **Prerequisites are satisfied.** Prerequisite 1 (pinning) is gone as above. Prerequisite 2 (`error_clamp` starving feedback at channel extremes) is resolved: `DitherOptions::default()` now uses `error_clamp: 1.0` (`crates/eink-dither/src/dither/options.rs:118`), and the stale `0.11` panel pins were removed.

Three design decisions the spec left open, resolved here and implemented as specified below:

3. **CSS is a real hazard and is handled by stripping, not by precedence.** Screen templates set `fill` from `<style>` rules (e.g. `screens/examples/hello/screen.svg` has `.date { fill: #555555; }`), and a CSS rule beats a presentation attribute. Rather than depend on whether usvg honours the `style` attribute over a stylesheet, the rewriter **removes paint declarations from `<style>` blocks** in the mask document and sets both the presentation attribute and the inline `style`. Geometry-affecting declarations (`font-*`, `stroke-width`, `text-anchor`, `letter-spacing`, `dominant-baseline`, `display`, `visibility`) are preserved, because they change what area is covered. **The declaration match must be case-insensitive and tolerate whitespace before the colon** — CSS allows `FILL: red` and `fill : red`, and measurement showed both survived an exact-match stripper, where they beat the presentation attribute and silently invert that element's mask polarity.
4. **Known mis-marking, accepted and documented.** `<image>` elements become a `<rect>` over their layout box, so a non-opaque or letterboxed image marks its whole box; an element painted `none` only via CSS becomes painted in the mask; and a stroke set only by a stylesheet rule is lost from the mask. The first two only ever *grow* the marked region, which is harmless (the mask background is already black); the third *shrinks* it, which is the deliberate fail-safe direction. Documented in the rewriter's module docs, not silently absorbed.
5. **`<defs>` content has its paint attributes stripped rather than rewritten**, so a `<use>` inherits paint from its use site and lands in the correct mask polarity. No screen in the tree uses `<use>` today; this keeps it correct if one does.

## File structure

**New — `eink-dither`:**

| File | Responsibility |
|---|---|
| `crates/eink-dither/src/color/oklch.rs` | `Oklch` type, promoted from `preprocess` and made public (Task 1) |
| `crates/eink-dither/src/gamut/mod.rs` | Module docs; re-exports `GamutMapper`, `GamutOptions` |
| `crates/eink-dither/src/gamut/hull.rs` | `Hull` — convex hull of palette colours in linear RGB, membership, rank/degeneracy, achievable lightness range |
| `crates/eink-dither/src/gamut/cmax.rs` | `CmaxTable` — precomputed `Cmax(h, L)`, bilinear sampling with hue wraparound |
| `crates/eink-dither/src/gamut/knee.rs` | `compress_chroma` — the pure knee curve |
| `crates/eink-dither/src/gamut/adapt.rs` | `adaptation_factor` — percentile of `rho` with absolute floor, capped |
| `crates/eink-dither/src/gamut/mapper.rs` | `GamutMapper` — assembles the above; `map_frame` |

**New — byonk:**

| File | Responsibility |
|---|---|
| `src/rendering/tone_mask.rs` | SVG → mask-SVG rewriter, and the marked-attribute presence scan |

**Modified:**

| File | Change |
|---|---|
| `crates/eink-dither/src/lib.rs` | `pub mod gamut;` + re-exports; `pub use color::Oklch` |
| `crates/eink-dither/src/preprocess/oklch.rs` | Deleted; `preprocess/mod.rs` re-points at `color::Oklch` |
| `crates/eink-dither/src/domain_tests.rs` | Oracle test validating `CmaxTable` against `best_reachable`; regression metrics |
| `src/rendering/svg_to_png.rs` | Mask rasterization, gamut step, `DitherTuning::gamut` |
| `src/rendering/mod.rs` | `pub mod tone_mask;` |
| `src/models/config.rs` | `GamutTuningValues` + merge into the tuning chain |
| `src/services/lua_runtime.rs` | Parse the `gamut` table from the script return |
| `src/services/content_pipeline.rs` | Thread the resolved gamut options through |
| `src/main.rs` | CLI tuning construction |
| `Cargo.toml` | `quick-xml` dependency |
| `docs/src/` | Authoring documentation |
| `CHANGES.md` | Unreleased entry |

---

### Task 1: Promote `Oklch` to the public colour module

`Oklch` already exists as `pub(crate)` in `preprocess/oklch.rs` with both `From` conversions and a round-trip test. The gamut module needs it publicly. Move it rather than duplicating it.

**Files:**
- Create: `crates/eink-dither/src/color/oklch.rs`
- Delete: `crates/eink-dither/src/preprocess/oklch.rs`
- Modify: `crates/eink-dither/src/color/mod.rs`, `crates/eink-dither/src/preprocess/mod.rs`, `crates/eink-dither/src/lib.rs:217`, `crates/eink-dither/src/domain_tests.rs:15`

**Interfaces:**
- Produces: `eink_dither::Oklch { pub l: f32, pub c: f32, pub h: f32 }`, `impl From<Oklab> for Oklch`, `impl From<Oklch> for Oklab`, `Oklch::scale_chroma(self, f32) -> Self`. `h` is in **radians** from `atan2(b, a)`, range `(-PI, PI]`.

- [ ] **Step 1: Write the failing test**

Append to `crates/eink-dither/src/color/mod.rs` (creating the module reference first is Step 3; this test asserts the public path exists):

```rust
#[cfg(test)]
mod public_oklch_tests {
    use crate::{LinearRgb, Oklab, Oklch};

    #[test]
    fn oklch_is_publicly_reachable_and_round_trips() {
        let lab = Oklab::from(LinearRgb::new(0.3, 0.1, 0.05));
        let lch = Oklch::from(lab);
        let back = Oklab::from(lch);
        assert!((back.l - lab.l).abs() < 1e-6, "l drifted");
        assert!((back.a - lab.a).abs() < 1e-6, "a drifted");
        assert!((back.b - lab.b).abs() < 1e-6, "b drifted");
        assert!(lch.c > 0.0, "a saturated colour must have non-zero chroma");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither public_oklch`
Expected: FAIL to compile — `no `Oklch` in the root`

- [ ] **Step 3: Move the type**

`git mv crates/eink-dither/src/preprocess/oklch.rs crates/eink-dither/src/color/oklch.rs`

In `color/oklch.rs`: change `pub(crate) struct Oklch` to `pub struct Oklch`, change `use crate::Oklab;` to `use super::Oklab;`, and replace the "Internal Use" doc section with:

```rust
//! # Use
//!
//! Polar form of Oklab. Chroma scaling preserves hue and lightness exactly,
//! which is what both the saturation boost in
//! [`PreprocessOptions::saturation`](crate::PreprocessOptions::saturation) and
//! the gamut mapper in [`crate::gamut`] rely on.
```

In `color/mod.rs` add `mod oklch;` and `pub use oklch::Oklch;` alongside the existing re-exports.

In `preprocess/mod.rs` remove `mod oklch;` and any `pub(crate) use oklch::Oklch;`, and replace internal references with `use crate::Oklch;`.

In `lib.rs:217` change to `pub use color::{LinearRgb, Oklab, Oklch, Srgb};`.

In `domain_tests.rs:15` change `use crate::preprocess::Oklch;` to `use crate::Oklch;`.

- [ ] **Step 4: Run the full crate tests**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither`
Expected: PASS, including the moved `test_oklab_to_oklch_round_trip`

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/color/oklch.rs crates/eink-dither/src/color/mod.rs \
        crates/eink-dither/src/preprocess/mod.rs crates/eink-dither/src/preprocess/oklch.rs \
        crates/eink-dither/src/lib.rs crates/eink-dither/src/domain_tests.rs
git commit -m "refactor(eink-dither): promote Oklch to the public colour module"
```

---

### Task 2: Convex hull of the palette in linear RGB

The achievable set of a dithered patch is the convex hull of the palette's actual colours in **linear RGB** — that is where light adds. With at most 16 palette entries, brute-force facet enumeration over all triples is correct and fast enough to run once per palette.

**Files:**
- Create: `crates/eink-dither/src/gamut/hull.rs`, `crates/eink-dither/src/gamut/mod.rs`
- Modify: `crates/eink-dither/src/lib.rs`

**Interfaces:**
- Consumes: `Palette::len()`, `Palette::actual_linear(idx) -> LinearRgb` (Task 0 — already exists).
- Produces:
  ```rust
  pub enum HullShape { Volume, Line, Plane }
  pub struct Hull { /* private */ }
  impl Hull {
      pub fn from_palette(palette: &Palette) -> Self;
      pub fn shape(&self) -> HullShape;
      pub fn contains(&self, p: LinearRgb) -> bool;
      pub fn lightness_range(&self) -> (f32, f32);
  }
  ```
  `lightness_range` returns the min and max Oklab L reachable **on the achromatic axis** (`a = b = 0`) inside the hull. For a degenerate hull it returns the L range of the palette points themselves.

- [ ] **Step 1: Write the shared test fixtures and the failing tests**

Tasks 2, 3 and 6 all need the same two palettes. Define them **once** in
`crates/eink-dither/src/gamut/mod.rs` so the three test modules share them
rather than each carrying a copy:

```rust
/// Palettes shared by the gamut modules' unit tests.
#[cfg(test)]
pub(crate) mod test_support {
    use crate::{Palette, Srgb};

    /// The six-ink panel palette, official colours.
    pub(crate) fn six_colour() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(0, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(255, 255, 0),
            ],
            None,
        )
        .unwrap()
    }

    /// A four-level greyscale panel — the degenerate-hull case.
    pub(crate) fn four_grey() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(85, 85, 85),
                Srgb::from_u8(170, 170, 170),
                Srgb::from_u8(255, 255, 255),
            ],
            None,
        )
        .unwrap()
    }
}
```

Then create `crates/eink-dither/src/gamut/hull.rs` containing only this test
module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamut::test_support::{four_grey, six_colour};
    use crate::{LinearRgb, Srgb};

    #[test]
    fn palette_vertices_are_inside_their_own_hull() {
        let p = six_colour();
        let hull = Hull::from_palette(&p);
        assert_eq!(hull.shape(), HullShape::Volume);
        for i in 0..p.len() {
            assert!(
                hull.contains(p.actual_linear(i)),
                "palette entry {i} must lie in its own hull"
            );
        }
    }

    #[test]
    fn centroid_is_inside_and_far_exterior_is_outside() {
        let p = six_colour();
        let hull = Hull::from_palette(&p);
        let mut c = [0.0f32; 3];
        for i in 0..p.len() {
            let e = p.actual_linear(i);
            c[0] += e.r / p.len() as f32;
            c[1] += e.g / p.len() as f32;
            c[2] += e.b / p.len() as f32;
        }
        assert!(hull.contains(LinearRgb::new(c[0], c[1], c[2])), "centroid must be inside");
        assert!(!hull.contains(LinearRgb::new(5.0, -3.0, 2.0)), "far exterior must be outside");
    }

    #[test]
    fn cyan_is_outside_a_palette_that_lacks_it() {
        // Pure cyan is not producible by mixing black/white/R/G/B/Y additively
        // at the intensity of full cyan: it sits outside the hull.
        let hull = Hull::from_palette(&six_colour());
        assert!(!hull.contains(LinearRgb::from(Srgb::from_u8(0, 255, 255))));
    }

    #[test]
    fn greyscale_palette_collapses_to_a_line() {
        let hull = Hull::from_palette(&four_grey());
        assert_eq!(hull.shape(), HullShape::Line);
    }

    #[test]
    fn lightness_range_spans_black_to_white() {
        let hull = Hull::from_palette(&six_colour());
        let (lo, hi) = hull.lightness_range();
        assert!(lo < 0.02, "black must be reachable, got {lo}");
        assert!(hi > 0.98, "white must be reachable, got {hi}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod gamut;` to `crates/eink-dither/src/lib.rs` after `pub mod dither;`, and create `crates/eink-dither/src/gamut/mod.rs` with `pub mod hull;` above the `test_support` module from Step 1:

```rust
//! Gamut mapping for continuous-tone regions.

pub mod hull;
```

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::hull`
Expected: FAIL to compile — `cannot find type `Hull` in this scope`

- [ ] **Step 3: Implement the hull**

Prepend to `crates/eink-dither/src/gamut/hull.rs`:

```rust
//! Convex hull of the palette's actual colours in linear RGB.
//!
//! A dithered patch's average is by construction a convex combination of the
//! palette's actual colours **in linear RGB** — that is where light adds. So
//! the convex hull of those colours bounds what any error-diffusion algorithm
//! can reproduce. The set is *not* convex in Oklab, which is why the hull
//! cannot be computed in perceptual space.
//!
//! With at most 16 palette entries, enumerating all point triples and keeping
//! those whose plane has every other point on one side is exact and costs
//! under a millisecond. It runs once when the palette resolves.

use crate::{LinearRgb, Oklab, Palette};

/// Tolerance for plane-side tests, in linear-RGB units.
const EPS: f32 = 1e-5;

/// Dimensionality of the palette's point set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullShape {
    /// Full 3-D body — the normal case for a chromatic panel.
    Volume,
    /// All points collinear — a greyscale palette. No chroma is reachable.
    Line,
    /// Coplanar but not collinear. Vanishingly unlikely in practice; callers
    /// treat it as "do not map" rather than guessing.
    Plane,
}

/// An outward-oriented facet: every palette point satisfies `n · p <= d`.
#[derive(Debug, Clone, Copy)]
struct Facet {
    n: [f32; 3],
    d: f32,
}

/// The convex hull of a palette's actual colours in linear RGB.
#[derive(Debug, Clone)]
pub struct Hull {
    facets: Vec<Facet>,
    shape: HullShape,
    l_min: f32,
    l_max: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

impl Hull {
    /// Build the hull from the colours the ditherer actually targets.
    pub fn from_palette(palette: &Palette) -> Self {
        let pts: Vec<[f32; 3]> = (0..palette.len())
            .map(|i| {
                let c = palette.actual_linear(i);
                [c.r, c.g, c.b]
            })
            .collect();

        let shape = classify(&pts);
        let facets = if shape == HullShape::Volume {
            enumerate_facets(&pts)
        } else {
            Vec::new()
        };

        let mut hull = Self {
            facets,
            shape,
            l_min: 0.0,
            l_max: 1.0,
        };
        let (l_min, l_max) = hull.compute_lightness_range(&pts);
        hull.l_min = l_min;
        hull.l_max = l_max;
        hull
    }

    /// Dimensionality of the point set.
    pub fn shape(&self) -> HullShape {
        self.shape
    }

    /// Is this colour inside the hull?
    ///
    /// Always false for a degenerate hull: a point has measure zero against a
    /// line or plane, so no useful membership question can be asked of it.
    /// Callers branch on [`Hull::shape`] before relying on this.
    pub fn contains(&self, p: LinearRgb) -> bool {
        if self.shape != HullShape::Volume {
            return false;
        }
        let q = [p.r, p.g, p.b];
        self.facets.iter().all(|f| dot(f.n, q) <= f.d + EPS)
    }

    /// The Oklab lightness range reachable on the achromatic axis.
    pub fn lightness_range(&self) -> (f32, f32) {
        (self.l_min, self.l_max)
    }

    /// Binary-search the grey axis for the darkest and lightest neutral inside
    /// the hull. For a degenerate hull, fall back to the palette points' own L
    /// range, which is exactly right for a greyscale ramp.
    fn compute_lightness_range(&self, pts: &[[f32; 3]]) -> (f32, f32) {
        if self.shape != HullShape::Volume {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for p in pts {
                let l = Oklab::from(LinearRgb::new(p[0], p[1], p[2])).l;
                lo = lo.min(l);
                hi = hi.max(l);
            }
            return (lo, hi);
        }

        let grey_inside = |l: f32| self.contains(LinearRgb::from(Oklab::new(l, 0.0, 0.0)));

        // Find any interior neutral to bracket from.
        let mut seed = None;
        for i in 0..=64 {
            let l = i as f32 / 64.0;
            if grey_inside(l) {
                seed = Some(l);
                break;
            }
        }
        let Some(seed) = seed else {
            // No neutral is reachable at all. Degenerate for our purposes.
            return (0.0, 1.0);
        };

        // Walk down, then up, by bisection.
        let (mut lo_out, mut lo_in) = (0.0f32, seed);
        if grey_inside(0.0) {
            lo_in = 0.0;
        } else {
            for _ in 0..24 {
                let mid = 0.5 * (lo_out + lo_in);
                if grey_inside(mid) {
                    lo_in = mid;
                } else {
                    lo_out = mid;
                }
            }
        }
        let (mut hi_out, mut hi_in) = (1.0f32, seed);
        if grey_inside(1.0) {
            hi_in = 1.0;
        } else {
            for _ in 0..24 {
                let mid = 0.5 * (hi_out + hi_in);
                if grey_inside(mid) {
                    hi_in = mid;
                } else {
                    hi_out = mid;
                }
            }
        }
        (lo_in, hi_in)
    }
}

/// Determine whether the points span a volume, a plane, or a line.
fn classify(pts: &[[f32; 3]]) -> HullShape {
    if pts.len() < 3 {
        return HullShape::Line;
    }
    let p0 = pts[0];
    // First independent direction.
    let Some(u) = pts
        .iter()
        .map(|p| sub(*p, p0))
        .find(|v| norm(*v) > EPS)
    else {
        return HullShape::Line;
    };
    // Second independent direction: a point off the line through p0 + u.
    let Some(n) = pts
        .iter()
        .map(|p| cross(u, sub(*p, p0)))
        .find(|c| norm(*c) > EPS)
    else {
        return HullShape::Line;
    };
    // Third: a point off that plane.
    let off_plane = pts
        .iter()
        .any(|p| dot(n, sub(*p, p0)).abs() > EPS * norm(n).max(1.0));
    if off_plane {
        HullShape::Volume
    } else {
        HullShape::Plane
    }
}

/// Every triple whose plane has all other points on one side is a hull facet.
/// Normals are oriented outward so that `n · p <= d` holds for every point.
fn enumerate_facets(pts: &[[f32; 3]]) -> Vec<Facet> {
    let n_pts = pts.len();
    let mut facets: Vec<Facet> = Vec::new();

    for i in 0..n_pts {
        for j in (i + 1)..n_pts {
            for k in (j + 1)..n_pts {
                let mut n = cross(sub(pts[j], pts[i]), sub(pts[k], pts[i]));
                let len = norm(n);
                if len < EPS {
                    continue; // collinear triple, no plane
                }
                n = [n[0] / len, n[1] / len, n[2] / len];
                let mut d = dot(n, pts[i]);

                let mut above = false;
                let mut below = false;
                for p in pts {
                    let s = dot(n, *p) - d;
                    if s > EPS {
                        above = true;
                    } else if s < -EPS {
                        below = true;
                    }
                }
                if above && below {
                    continue; // plane cuts through the body
                }
                if above {
                    n = [-n[0], -n[1], -n[2]];
                    d = -d;
                }

                // Skip a plane we already have (coplanar triples repeat).
                let dup = facets.iter().any(|f| {
                    (f.n[0] - n[0]).abs() < 1e-4
                        && (f.n[1] - n[1]).abs() < 1e-4
                        && (f.n[2] - n[2]).abs() < 1e-4
                        && (f.d - d).abs() < 1e-4
                });
                if !dup {
                    facets.push(Facet { n, d });
                }
            }
        }
    }

    facets
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::hull`
Expected: PASS, 5 tests

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/gamut/mod.rs crates/eink-dither/src/gamut/hull.rs crates/eink-dither/src/lib.rs
git commit -m "feat(eink-dither): convex hull of the palette in linear RGB"
```

---

### Task 3: The `Cmax(hue, lightness)` table

Per-pixel hull queries are too slow. Precompute the largest in-hull chroma per (hue, lightness) bin once per palette, and sample it bilinearly at render time.

**Files:**
- Create: `crates/eink-dither/src/gamut/cmax.rs`
- Modify: `crates/eink-dither/src/gamut/mod.rs`

**Interfaces:**
- Consumes: `Hull::from_palette`, `Hull::contains`, `Hull::shape`, `Hull::lightness_range`, `HullShape`.
- Produces:
  ```rust
  pub const HUE_BINS: usize = 128;
  pub const LIGHTNESS_BINS: usize = 64;
  pub struct CmaxTable { /* private */ }
  impl CmaxTable {
      pub fn build(hull: &Hull) -> Self;
      /// `h` in radians (any value; wrapped), `l` in 0..=1 (clamped).
      pub fn sample(&self, h: f32, l: f32) -> f32;
      pub fn lightness_range(&self) -> (f32, f32);
      pub fn is_achromatic(&self) -> bool;
  }
  ```

- [ ] **Step 1: Write the failing tests**

Create `crates/eink-dither/src/gamut/cmax.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamut::hull::Hull;
    use crate::gamut::test_support::{four_grey, six_colour};
    use crate::{LinearRgb, Oklab, Oklch};

    #[test]
    fn sampled_chroma_is_inside_the_hull_everywhere() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        for hi in 0..64 {
            let h = -std::f32::consts::PI + (hi as f32 / 64.0) * std::f32::consts::TAU;
            for li in 1..64 {
                let l = li as f32 / 64.0;
                let c = table.sample(h, l);
                if c <= 0.0 {
                    continue;
                }
                // Sit just inside the reported limit. The sample may overshoot
                // the true boundary between bins: by a small relative amount at
                // ordinary chroma, and by a small absolute amount in the
                // near-black rows where Cmax itself is tiny. Back off by
                // whichever is larger before probing.
                let probe_c = c - (0.08 * c).max(0.0015);
                if probe_c <= 0.0 {
                    continue;
                }
                let probe = Oklch { l, c: probe_c, h };
                assert!(
                    hull.contains(LinearRgb::from(Oklab::from(probe))),
                    "sample(h={h:.3}, l={l:.3}) = {c:.4} is not reachable"
                );
            }
        }
    }

    #[test]
    fn chroma_limit_is_zero_at_the_lightness_extremes() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        assert!(table.sample(0.0, 0.0) < 0.02, "black admits no chroma");
        assert!(table.sample(0.0, 1.0) < 0.02, "white admits no chroma");
    }

    #[test]
    fn mid_lightness_admits_real_chroma() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        // Red sits near h = 0.5 rad in Oklab.
        let c = table.sample(0.5, 0.55);
        assert!(c > 0.05, "mid-lightness warm hue should reach chroma, got {c}");
    }

    #[test]
    fn hue_wraps_continuously() {
        let hull = Hull::from_palette(&six_colour());
        let table = CmaxTable::build(&hull);
        let a = table.sample(std::f32::consts::PI - 1e-4, 0.5);
        let b = table.sample(-std::f32::consts::PI + 1e-4, 0.5);
        assert!((a - b).abs() < 0.01, "hue must wrap: {a} vs {b}");
    }

    #[test]
    fn greyscale_palette_reports_achromatic_and_zero_chroma() {
        let table = CmaxTable::build(&Hull::from_palette(&four_grey()));
        assert!(table.is_achromatic());
        assert_eq!(table.sample(1.0, 0.5), 0.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod cmax;` to `crates/eink-dither/src/gamut/mod.rs`.

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::cmax`
Expected: FAIL to compile — `cannot find type `CmaxTable``

- [ ] **Step 3: Implement the table**

Prepend to `crates/eink-dither/src/gamut/cmax.rs`:

```rust
//! Precomputed chroma limit as a function of hue and lightness.
//!
//! Querying the hull per pixel is far too slow. Instead, when the palette
//! resolves, binary-search the largest in-hull chroma for each (hue,
//! lightness) bin and store the result. Render-time lookup is a bilinear
//! sample, with hue wrapping around the circle.
//!
//! The table is built once per palette, not per frame.

use super::hull::{Hull, HullShape};
use crate::{LinearRgb, Oklab, Oklch};

/// Hue bins around the full circle.
pub const HUE_BINS: usize = 128;
/// Lightness bins across 0..=1.
pub const LIGHTNESS_BINS: usize = 64;

/// Upper bound for the chroma search. Oklab chroma inside the sRGB cube peaks
/// around 0.33; 0.5 leaves generous headroom without wasting iterations.
const C_SEARCH_HI: f32 = 0.5;
/// Bisection steps — 24 resolves chroma to well under 1e-6.
const SEARCH_STEPS: usize = 24;

/// `Cmax(hue, lightness)` for one palette.
#[derive(Debug, Clone)]
pub struct CmaxTable {
    /// Row-major `[hue][lightness]`, length `HUE_BINS * LIGHTNESS_BINS`.
    data: Vec<f32>,
    l_min: f32,
    l_max: f32,
    achromatic: bool,
    unmappable: bool,
}

impl CmaxTable {
    /// Build the table by bisecting the hull boundary in each bin.
    ///
    /// Two degenerate cases are deliberately distinguished, because they call
    /// for opposite behaviour:
    ///
    /// - **Greyscale palette** (`HullShape::Line`) — no chroma is reachable, so
    ///   every limit is zero and marked content desaturates to grey. That is
    ///   the correct result on a four-level panel, not a bug.
    /// - **Unmappable hull** — coplanar, or a full volume whose grey axis lies
    ///   entirely outside it (`!Hull::is_mappable()`). Chroma compression has
    ///   no meaningful target here, so the mapper declines and leaves the
    ///   content untouched rather than crushing it onto a lightness the panel
    ///   cannot render.
    pub fn build(hull: &Hull) -> Self {
        let (l_min, l_max) = hull.lightness_range();
        let achromatic = hull.shape() == HullShape::Line;
        let unmappable = !achromatic && !hull.is_mappable();

        let mut data = vec![0.0f32; HUE_BINS * LIGHTNESS_BINS];
        if !achromatic && !unmappable {
            for hi in 0..HUE_BINS {
                let h = hue_of_bin(hi);
                for li in 0..LIGHTNESS_BINS {
                    let l = li as f32 / (LIGHTNESS_BINS - 1) as f32;
                    data[hi * LIGHTNESS_BINS + li] = max_chroma(hull, h, l);
                }
            }
        }

        Self {
            data,
            l_min,
            l_max,
            achromatic,
            unmappable,
        }
    }

    /// Sample the limit, bilinearly, wrapping hue and clamping lightness.
    pub fn sample(&self, h: f32, l: f32) -> f32 {
        if self.achromatic || self.unmappable {
            return 0.0;
        }

        let tau = std::f32::consts::TAU;
        // Map h into [0, 1) around the circle, matching `hue_of_bin`.
        let hn = ((h + std::f32::consts::PI).rem_euclid(tau)) / tau;
        let hf = hn * HUE_BINS as f32;
        let h0 = hf.floor() as usize % HUE_BINS;
        let h1 = (h0 + 1) % HUE_BINS;
        let ht = hf - hf.floor();

        let lf = l.clamp(0.0, 1.0) * (LIGHTNESS_BINS - 1) as f32;
        let l0 = lf.floor() as usize;
        let l1 = (l0 + 1).min(LIGHTNESS_BINS - 1);
        let lt = lf - lf.floor();

        let at = |hb: usize, lb: usize| self.data[hb * LIGHTNESS_BINS + lb];
        let a = at(h0, l0) * (1.0 - lt) + at(h0, l1) * lt;
        let b = at(h1, l0) * (1.0 - lt) + at(h1, l1) * lt;
        (a * (1.0 - ht) + b * ht).max(0.0)
    }

    /// The lightness range reachable on the achromatic axis.
    pub fn lightness_range(&self) -> (f32, f32) {
        (self.l_min, self.l_max)
    }

    /// True when the palette admits no chroma at all — a greyscale panel.
    /// Marked content desaturates to grey, which is the correct result.
    pub fn is_achromatic(&self) -> bool {
        self.achromatic
    }

    /// True when chroma compression has no meaningful target: a coplanar hull,
    /// or a volume whose grey axis lies entirely outside it. The mapper leaves
    /// such content untouched rather than guessing.
    pub fn is_unmappable(&self) -> bool {
        self.unmappable
    }
}

/// Bin centre hue, in radians, matching `sample`'s inverse mapping.
fn hue_of_bin(hi: usize) -> f32 {
    let tau = std::f32::consts::TAU;
    (hi as f32 / HUE_BINS as f32) * tau - std::f32::consts::PI
}

/// Largest chroma at this (hue, lightness) whose Oklch point is still inside
/// the hull. Zero when even the neutral at this lightness is unreachable.
fn max_chroma(hull: &Hull, h: f32, l: f32) -> f32 {
    // At pure black (L = 0), the Oklab-to-linear cubic map degenerates, causing
    // `contains` to admit a spurious non-zero chroma. Handle this explicitly
    // since black is a hull vertex.
    if l <= 0.001 {
        return 0.0;
    }

    let inside = |c: f32| hull.contains(LinearRgb::from(Oklab::from(Oklch { l, c, h })));

    if !inside(0.0) {
        return 0.0;
    }
    if inside(C_SEARCH_HI) {
        return C_SEARCH_HI;
    }
    let (mut lo, mut hi) = (0.0f32, C_SEARCH_HI);
    for _ in 0..SEARCH_STEPS {
        let mid = 0.5 * (lo + hi);
        if inside(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::cmax`
Expected: PASS, 5 tests

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/gamut/cmax.rs crates/eink-dither/src/gamut/mod.rs
git commit -m "feat(eink-dither): precomputed Cmax(hue, lightness) chroma-limit table"
```

---

### Task 4: The knee compression curve

Pure maths, no palette. This is where the design's central property lives: strictly increasing, so nothing collapses onto a shared value.

**Files:**
- Create: `crates/eink-dither/src/gamut/knee.rs`
- Modify: `crates/eink-dither/src/gamut/mod.rs`

**Interfaces:**
- Produces: `pub fn compress_chroma(c: f32, c_max: f32, knee: f32) -> f32`

- [ ] **Step 1: Write the failing tests**

Create `crates/eink-dither/src/gamut/knee.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const CMAX: f32 = 0.20;
    const K: f32 = 0.6;

    #[test]
    fn below_the_knee_is_identity() {
        for i in 0..=60 {
            let c = (i as f32 / 100.0) * CMAX;
            assert!(
                (compress_chroma(c, CMAX, K) - c).abs() < 1e-6,
                "c={c} must pass through untouched"
            );
        }
    }

    #[test]
    fn continuous_at_the_knee() {
        let below = compress_chroma(K * CMAX - 1e-5, CMAX, K);
        let above = compress_chroma(K * CMAX + 1e-5, CMAX, K);
        assert!((above - below).abs() < 1e-4, "discontinuity: {below} -> {above}");
    }

    #[test]
    fn strictly_increasing_across_the_reachable_range() {
        let mut prev = f32::NEG_INFINITY;
        for i in 0..20_000 {
            // Up to c = 3.0, i.e. t = 36. Measured across every sRGB colour,
            // rho = C/Cmax peaks at 5.02, which is t = 11.05 at k = 0.6, so
            // this covers three times the reachable domain. Testing far
            // beyond it would only measure f32 resolution, not the curve.
            let c = i as f32 * 0.00015;
            let out = compress_chroma(c, CMAX, K);
            assert!(out > prev, "not strictly increasing at c={c}: {prev} -> {out}");
            prev = out;
        }
    }

    #[test]
    fn asymptotic_to_cmax_and_never_reaches_it() {
        for c in [CMAX, 2.0 * CMAX, 10.0 * CMAX, 1000.0 * CMAX] {
            let out = compress_chroma(c, CMAX, K);
            assert!(out < CMAX, "c={c} produced {out}, must stay under {CMAX}");
        }
        assert!(
            compress_chroma(1000.0 * CMAX, CMAX, K) > 0.999 * CMAX,
            "extreme input should approach the bound"
        );
    }

    #[test]
    fn zero_cmax_yields_zero() {
        assert_eq!(compress_chroma(0.3, 0.0, K), 0.0);
    }

    #[test]
    fn knee_of_zero_compresses_everything() {
        // k = 0 means the curve bends from the origin; still bounded and
        // strictly increasing.
        assert!(compress_chroma(0.01, CMAX, 0.0) < 0.01);
        assert!(compress_chroma(0.02, CMAX, 0.0) > compress_chroma(0.01, CMAX, 0.0));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod knee;` to `crates/eink-dither/src/gamut/mod.rs`.

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::knee`
Expected: FAIL to compile — `cannot find function `compress_chroma``

- [ ] **Step 3: Implement the curve**

Prepend to `crates/eink-dither/src/gamut/knee.rs`:

```rust
//! The chroma compression curve.
//!
//! Below `knee * c_max` the input passes through untouched, so low-chroma
//! content — the bulk of most images — is never desaturated. Above it, a
//! power shoulder maps the whole remaining half-line into
//! `[knee * c_max, c_max)`:
//!
//! ```text
//! C <= k*Cmax :  C' = C
//! C >  k*Cmax :  C' = k*Cmax + (1-k)*Cmax * t/(1+t^p)^(1/p),
//!                t  = (C - k*Cmax) / ((1-k)*Cmax)
//! ```
//!
//! This is the `powerP` curve of the ACES 1.3 Reference Gamut Compression,
//! at its default exponent `p = 1.2`. The exponent controls how sharply the
//! shoulder rolls off: `p = 1` is the classic Reinhard form, and `p → ∞`
//! degenerates to a hard clip.
//!
//! The shoulder is a power curve rather than an exponential for a measured
//! reason. `1 - exp(-t)` reaches 1.0 in `f32` at `t ≈ 10.2`, and the reachable
//! input domain extends to `t ≈ 11.05` — so an exponential shoulder returns
//! *exactly* `c_max` for real pixels, silently becoming the clipping this
//! design rejects. The power form decays polynomially: at `p = 1.2` it stays
//! strictly below 1.0 out to `t ≈ 85.9`, roughly eight times beyond anything
//! reachable.
//!
//! **The reachable domain is measured, not assumed.** Sweeping every sRGB
//! colour with non-zero chroma against this crate's `CmaxTable` for the
//! six-ink palette, `rho = C / Cmax` peaks at **5.02** (median 0.91, p99.9
//! 4.23) — `Cmax` shrinks toward black and white, but so does the chroma any
//! sRGB colour can have there, so the ratio stays bounded. With `k = 0.6`,
//! `rho = 5.02` is `t = 11.05`. The monotonicity test therefore covers `t` to
//! about 36, three times the reachable maximum, rather than an arbitrary range.
//!
//! The curve is continuous at the knee and **strictly increasing everywhere**,
//! approaching `c_max` asymptotically without reaching it. That property is
//! the formal statement of the design's goal: two colours that differed before
//! still differ after. Nothing collapses onto a shared value — which is
//! exactly what a clipping approach (HPMINDE) would do, and why it was
//! rejected.
//!
//! Because the shoulder accepts any input however large, content beyond the
//! adaptation cap is compressed very hard but never clipped.

/// Exponent of the shoulder roll-off — the ACES 1.3 Reference Gamut
/// Compression default. Higher values protect near-boundary chroma but
/// crowd far-out-of-gamut values together; lower values do the reverse.
const SHOULDER_POWER: f32 = 1.2;

/// Compress `c` into `[0, c_max)`, leaving everything below the knee alone.
///
/// # Arguments
/// * `c` — input chroma, already normalised by the adaptation factor
/// * `c_max` — the reachable chroma limit at this hue and lightness
/// * `knee` — fraction of `c_max` at which compression begins, in `0..=1`
#[inline]
pub fn compress_chroma(c: f32, c_max: f32, knee: f32) -> f32 {
    if c_max <= 0.0 {
        return 0.0;
    }
    let k = knee.clamp(0.0, 0.999);
    let threshold = k * c_max;
    if c <= threshold {
        return c;
    }
    let span = (1.0 - k) * c_max;
    let t = (c - threshold) / span;
    let p = SHOULDER_POWER;
    threshold + span * (t / (1.0 + t.powf(p)).powf(1.0 / p))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::knee`
Expected: PASS, 6 tests

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/gamut/knee.rs crates/eink-dither/src/gamut/mod.rs
git commit -m "feat(eink-dither): strictly-increasing chroma knee curve"
```

---

### Task 5: Content adaptation factor

Derive one scalar `R` from the content: the 99th percentile of `rho = C / Cmax`, with an absolute floor on the discard count and a cap on the result.

**Files:**
- Create: `crates/eink-dither/src/gamut/adapt.rs`
- Modify: `crates/eink-dither/src/gamut/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const PERCENTILE: f32 = 0.99;
  pub const MIN_DISCARD: usize = 32;
  /// `rhos` is consumed and reordered in place.
  pub fn adaptation_factor(rhos: &mut [f32], max_compression: f32) -> f32;
  ```
  Returns `1.0` (identity) when the content is already in gamut.

- [ ] **Step 1: Write the failing tests**

Create `crates/eink-dither/src/gamut/adapt.rs` with only this test module:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod adapt;` to `crates/eink-dither/src/gamut/mod.rs`.

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::adapt`
Expected: FAIL to compile — `cannot find function `adaptation_factor``

- [ ] **Step 3: Implement the adaptation**

Prepend to `crates/eink-dither/src/gamut/adapt.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::adapt`
Expected: PASS, 7 tests

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/gamut/adapt.rs crates/eink-dither/src/gamut/mod.rs
git commit -m "feat(eink-dither): content adaptation factor with percentile, floor and cap"
```

---

### Task 6: `GamutMapper` — assemble and map a frame

Ties hull, table, adaptation and knee together, and asserts the four design properties end to end.

**Files:**
- Create: `crates/eink-dither/src/gamut/mapper.rs`
- Modify: `crates/eink-dither/src/gamut/mod.rs`, `crates/eink-dither/src/lib.rs`

**Interfaces:**
- Consumes: `Hull::from_palette`, `CmaxTable::build`/`sample`/`lightness_range`/`is_achromatic`, `adaptation_factor`, `compress_chroma`, `Oklch`, `Oklab`, `LinearRgb`, `Srgb`, `Palette`.
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub struct GamutOptions {
      pub knee: f32,            // default 0.8
      pub amount: f32,          // default 1.0
      pub max_compression: f32, // default 2.5
  }
  impl Default for GamutOptions { /* the values above */ }

  pub struct GamutMapper { /* private */ }
  impl GamutMapper {
      pub fn new(palette: &Palette) -> Self;
      /// Map every pixel where `mask[i]` is true, in place. `mask` must be the
      /// same length as `pixels`. Pixels outside the mask are untouched.
      pub fn map_frame(&self, pixels: &mut [Srgb], mask: &[bool], opts: GamutOptions);
      /// Map a single colour with an explicit adaptation factor. Exposed for
      /// tests and for callers that already know `R`.
      pub fn map_color(&self, c: Srgb, r: f32, opts: GamutOptions) -> Srgb;
      /// `C / Cmax(h, L)` for one colour — the adaptation input.
      pub fn rho(&self, c: Srgb) -> f32;
  }
  ```
  Re-exported from the crate root as `eink_dither::{GamutMapper, GamutOptions}`.

- [ ] **Step 1: Write the failing tests**

Create `crates/eink-dither/src/gamut/mapper.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamut::hull::Hull;
    use crate::gamut::test_support::{four_grey, six_colour};
    use crate::{LinearRgb, Oklab, Oklch, Srgb};

    /// A spread of saturated colours, well outside a six-ink gamut.
    fn vivid_frame() -> Vec<Srgb> {
        let mut v = Vec::new();
        for i in 0..64 {
            for j in 0..64 {
                v.push(Srgb::from_u8((i * 4) as u8, (j * 4) as u8, 200));
            }
        }
        v
    }

    #[test]
    fn mapping_twice_equals_mapping_once() {
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let mask = vec![true; 64 * 64];

        let mut once = vivid_frame();
        m.map_frame(&mut once, &mask, opts);

        let mut twice = once.clone();
        m.map_frame(&mut twice, &mask, opts);

        for (i, (a, b)) in once.iter().zip(twice.iter()).enumerate() {
            assert_eq!(
                a.to_bytes(),
                b.to_bytes(),
                "pixel {i} changed on the second pass: {:?} -> {:?}",
                a.to_bytes(),
                b.to_bytes()
            );
        }
    }

    #[test]
    fn in_gamut_content_is_returned_unchanged() {
        let p = six_colour();
        let m = GamutMapper::new(&p);
        // Midpoints between palette entries are inside the hull by convexity.
        let mut pixels: Vec<Srgb> = Vec::new();
        for i in 0..p.len() {
            for j in 0..p.len() {
                let a = p.actual_linear(i);
                let b = p.actual_linear(j);
                pixels.push(Srgb::from(LinearRgb::new(
                    0.5 * (a.r + b.r),
                    0.5 * (a.g + b.g),
                    0.5 * (a.b + b.b),
                )));
            }
        }
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(a.to_bytes(), b.to_bytes(), "in-gamut pixel {i} was altered");
        }
    }

    #[test]
    fn hue_is_preserved() {
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        for c in [
            Srgb::from_u8(255, 0, 128),
            Srgb::from_u8(0, 200, 255),
            Srgb::from_u8(180, 255, 0),
            Srgb::from_u8(120, 0, 255),
        ] {
            let out = m.map_color(c, 2.0, opts);
            let h_in = Oklch::from(Oklab::from(LinearRgb::from(c))).h;
            let h_out = Oklch::from(Oklab::from(LinearRgb::from(out))).h;
            let mut d = (h_out - h_in).abs();
            if d > std::f32::consts::PI {
                d = std::f32::consts::TAU - d;
            }
            // 8-bit output quantisation dominates this tolerance.
            assert!(d < 0.05, "hue moved by {d} rad for {:?}", c.to_bytes());
        }
    }

    #[test]
    fn chroma_map_is_strictly_monotonic() {
        // Asserted on the float chroma function, not on bytes: 8-bit output
        // quantisation legitimately collapses adjacent values.
        let m = GamutMapper::new(&six_colour());
        let opts = GamutOptions::default();
        let (h, l) = (0.7f32, 0.55f32);
        let mut prev = f32::NEG_INFINITY;
        for i in 0..5000 {
            let c = i as f32 * 0.0002;
            let out = m.mapped_chroma(c, h, l, 2.0, opts);
            assert!(out > prev, "chroma map not increasing at c={c}: {prev} -> {out}");
            prev = out;
        }
    }

    #[test]
    fn mapped_output_lands_inside_the_hull() {
        let p = six_colour();
        let m = GamutMapper::new(&p);
        let hull = Hull::from_palette(&p);
        let mut pixels = vivid_frame();
        let mask = vec![true; pixels.len()];
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        let outside = pixels
            .iter()
            .filter(|c| !hull.contains(LinearRgb::from(**c)))
            .count();
        // 8-bit round-tripping can nudge a boundary pixel just outside.
        let ratio = outside as f32 / pixels.len() as f32;
        assert!(ratio < 0.02, "{:.1}% of mapped pixels left the hull", ratio * 100.0);
    }

    #[test]
    fn unmasked_pixels_are_never_touched() {
        let m = GamutMapper::new(&six_colour());
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mut mask = vec![false; pixels.len()];
        mask[0] = true;
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        for i in 1..pixels.len() {
            assert_eq!(before[i].to_bytes(), pixels[i].to_bytes(), "unmasked pixel {i} changed");
        }
    }

    #[test]
    fn amount_zero_is_a_no_op() {
        let m = GamutMapper::new(&six_colour());
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(
            &mut pixels,
            &mask,
            GamutOptions {
                amount: 0.0,
                ..GamutOptions::default()
            },
        );
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(a.to_bytes(), b.to_bytes(), "amount=0 altered pixel {i}");
        }
    }

    #[test]
    fn greyscale_palette_desaturates_rather_than_flinging_at_an_ink() {
        let m = GamutMapper::new(&four_grey());
        let mut pixels = vec![Srgb::from_u8(220, 30, 40)];
        m.map_frame(&mut pixels, &[true], GamutOptions::default());
        let (r, g, b) = {
            let v = pixels[0].to_bytes();
            (v[0] as i32, v[1] as i32, v[2] as i32)
        };
        assert!(
            (r - g).abs() <= 2 && (g - b).abs() <= 2,
            "expected a neutral, got {r},{g},{b}"
        );
    }

    #[test]
    fn an_unmappable_hull_leaves_content_untouched() {
        // A full-volume hull whose grey axis lies entirely outside it. There
        // is no meaningful chroma target, so the mapper must decline rather
        // than desaturate — the opposite of the greyscale case above.
        let p = Palette::new(
            &[
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(255, 51, 0),
                Srgb::from_u8(255, 0, 51),
                Srgb::from_u8(204, 26, 26),
            ],
            None,
        )
        .unwrap();
        let m = GamutMapper::new(&p);
        let mut pixels = vivid_frame();
        let before = pixels.clone();
        let mask = vec![true; pixels.len()];
        m.map_frame(&mut pixels, &mask, GamutOptions::default());
        for (i, (a, b)) in before.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(
                a.to_bytes(),
                b.to_bytes(),
                "unmappable hull must be the identity, pixel {i} changed"
            );
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod mapper;` to `crates/eink-dither/src/gamut/mod.rs`, and add to `crates/eink-dither/src/lib.rs` next to the other re-exports:

```rust
pub use gamut::{GamutMapper, GamutOptions};
```

and in `gamut/mod.rs`:

```rust
pub use mapper::{GamutMapper, GamutOptions};
```

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::mapper`
Expected: FAIL to compile — `cannot find type `GamutMapper``

- [ ] **Step 3: Implement the mapper**

Prepend to `crates/eink-dither/src/gamut/mapper.rs`:

```rust
//! The gamut mapper: hull, chroma-limit table, content adaptation and knee,
//! assembled into a per-frame operation.
//!
//! Correction happens on the rasterized frame, immediately before dithering —
//! the only point where the mapping sees the pixels as they will actually be
//! dithered, after scaling, compositing and any SVG filters.
//!
//! This deliberately trades colorimetric accuracy for discriminability. Mean
//! dE against the original is *expected to get worse*; what improves is the
//! preservation of differences — gradients that used to band, hues that used
//! to collapse onto one ink, and hue ordering that used to invert.
//!
//! # Why chroma-only suffices
//!
//! Because a six-ink palette contains both pure black and pure white, every
//! `(L, h)` has a non-empty achievable range `[0, Cmax]`, so compressing
//! chroma at fixed lightness always lands in gamut. For a palette lacking a
//! near-black or near-white, lightness is first clamped into the hull's
//! achievable range.

use super::adapt::adaptation_factor;
use super::cmax::CmaxTable;
use super::hull::Hull;
use super::knee::compress_chroma;
use crate::{LinearRgb, Oklab, Oklch, Palette, Srgb};

/// Tuning knobs. Frame-level, not per adaptation group: groups change only
/// which pixels are measured together to derive `R`, not the curve's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GamutOptions {
    /// Where compression begins, as a fraction of `Cmax`.
    ///
    /// The default sits in the same band as the ACES 1.3 Reference Gamut
    /// Compression thresholds (`0.815`, `0.803`, `0.880`), which are expressed
    /// in the same normalised units.
    ///
    /// An earlier draft chose 0.6, reasoning that this gamut is small enough
    /// that almost everything falls outside it, so a high knee would crush the
    /// vivid range into a sliver near `Cmax`. Measurement does not support
    /// either half of that. Across every sRGB colour with non-zero chroma,
    /// `rho = C/Cmax` has a median of 0.91 and a p90 of 1.30 — about half the
    /// cube is outside the hull, not almost all of it. And because `map_frame`
    /// normalises by `R`, the 99th percentile of `rho`, the "sliver" only ever
    /// holds the top ~1% of a region's pixels.
    ///
    /// Measured against that, a low knee is a bad trade. At `knee = 0.6` the
    /// frame's vivid end (`rho/R = 1`) renders at 82.4% of the achievable
    /// chroma; at 0.8 it renders at 91.2%. What the lower knee buys back is
    /// separation in the out-of-gamut tail of 0.005 in Oklab chroma and below
    /// — against roughly 0.02 for one JND, on a panel that dithers six inks.
    /// It spends visible chroma to preserve differences nothing can render.
    pub knee: f32,
    /// Interpolation between input and mapped chroma:
    /// `C_out = C + amount * (C' - C)`.
    ///
    /// At `1.0` the output is the mapped chroma; at `0.0` the region is
    /// untouched, which makes it a clean A/B switch for judging the effect on
    /// a real panel. **Only `amount = 1.0` guarantees in-gamut output** —
    /// lower values can leave chroma above `Cmax`, which the ditherer then
    /// clips as it does today. It is a comparison and taste control, not a
    /// correctness one.
    pub amount: f32,
    /// Cap on `R` — literally "never compress chroma by more than this".
    ///
    /// Raising it lets an extremely vivid image adapt further, at the cost of
    /// flattening everything else; lowering it protects the bulk of the image
    /// and pushes the extremes into the knee's asymptotic tail instead, where
    /// they stay distinguishable but heavily compressed.
    pub max_compression: f32,
}

impl Default for GamutOptions {
    fn default() -> Self {
        Self {
            knee: 0.8,
            amount: 1.0,
            max_compression: 2.5,
        }
    }
}

/// Maps colours into a palette's reachable hull. Build once per palette.
#[derive(Debug, Clone)]
pub struct GamutMapper {
    table: CmaxTable,
    l_min: f32,
    l_max: f32,
}

impl GamutMapper {
    /// Build from the colours the ditherer targets — measured when they
    /// resolve, official otherwise. The hull and the dither target must not
    /// diverge.
    pub fn new(palette: &Palette) -> Self {
        let hull = Hull::from_palette(palette);
        let table = CmaxTable::build(&hull);
        let (l_min, l_max) = table.lightness_range();
        Self {
            table,
            l_min,
            l_max,
        }
    }

    /// `C / Cmax(h, L)` — how far out of gamut this colour is, 1.0 being
    /// exactly on the boundary. Infinite when the palette admits no chroma at
    /// this hue and lightness but the colour has some.
    pub fn rho(&self, c: Srgb) -> f32 {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(c)));
        let l = lch.l.clamp(self.l_min, self.l_max);
        let c_max = self.table.sample(lch.h, l);
        if c_max <= 0.0 {
            if lch.c <= 0.0 {
                0.0
            } else {
                f32::INFINITY
            }
        } else {
            lch.c / c_max
        }
    }

    /// The chroma the mapper would produce, in float. Separate from
    /// [`GamutMapper::map_color`] so monotonicity can be asserted without 8-bit
    /// quantisation in the way.
    pub(crate) fn mapped_chroma(
        &self,
        c: f32,
        h: f32,
        l: f32,
        r: f32,
        opts: GamutOptions,
    ) -> f32 {
        let l = l.clamp(self.l_min, self.l_max);
        let c_max = self.table.sample(h, l);
        let compressed = compress_chroma(c / r.max(1.0), c_max, opts.knee);
        c + opts.amount * (compressed - c)
    }

    /// Map one colour with an explicit adaptation factor.
    pub fn map_color(&self, color: Srgb, r: f32, opts: GamutOptions) -> Srgb {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(color)));
        let l = lch.l.clamp(self.l_min, self.l_max);
        let c_out = self.mapped_chroma(lch.c, lch.h, l, r, opts);
        Srgb::from(LinearRgb::from(Oklab::from(Oklch {
            l,
            c: c_out.max(0.0),
            h: lch.h,
        })))
    }

    /// Map every masked pixel in place.
    ///
    /// Derives one adaptation factor from the masked pixels, then applies the
    /// curve. When the masked content is already in gamut (`R <= 1`) this is
    /// the identity and nothing is needlessly desaturated.
    pub fn map_frame(&self, pixels: &mut [Srgb], mask: &[bool], opts: GamutOptions) {
        debug_assert_eq!(pixels.len(), mask.len(), "mask must match the frame");
        if opts.amount == 0.0 {
            return;
        }
        // No meaningful compression target: leave the content alone rather
        // than crushing it onto a lightness the panel cannot render. See
        // `CmaxTable::is_unmappable`.
        if self.table.is_unmappable() {
            return;
        }

        let mut rhos: Vec<f32> = pixels
            .iter()
            .zip(mask.iter())
            .filter(|(_, &m)| m)
            .map(|(p, _)| self.rho(*p))
            .collect();
        if rhos.is_empty() {
            return;
        }

        let r = adaptation_factor(&mut rhos, opts.max_compression);
        // Identity: content already fits. Skip rather than desaturate.
        if r <= 1.0 && !self.table.is_achromatic() {
            return;
        }

        for (p, &m) in pixels.iter_mut().zip(mask.iter()) {
            if m {
                *p = self.map_color(*p, r, opts);
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut::`
Expected: PASS, all gamut tests (hull 5, cmax 5, knee 6, adapt 7, mapper 8)

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/gamut/mapper.rs crates/eink-dither/src/gamut/mod.rs crates/eink-dither/src/lib.rs
git commit -m "feat(eink-dither): GamutMapper with idempotence, hue and monotonicity properties"
```

---

### Task 7: Validate the fast table against the slow oracle

`best_reachable()` in `domain_tests.rs` computes exact hull projections by optimisation — too slow for production, ideal as a test oracle. The slow correct thing checks the fast thing.

**Files:**
- Modify: `crates/eink-dither/src/domain_tests.rs`

**Interfaces:**
- Consumes: `best_reachable(&Palette, Oklab) -> (f32, Vec<f32>)` (already private in `domain_tests.rs`, returns `(distance, weights)`); `GamutMapper`, `CmaxTable`, `Hull`.

- [ ] **Step 1: Write the failing test**

Append to the module in `crates/eink-dither/src/domain_tests.rs` that already contains `best_reachable` (so it is in scope without changing visibility):

```rust
    /// The fast `Cmax` table must agree with the slow exact oracle.
    ///
    /// `best_reachable` finds the closest point in the hull by optimisation.
    /// For a target sitting exactly at the table's reported chroma limit, that
    /// distance must be near zero — the point is on the boundary, so it is
    /// reachable. For a target well beyond the limit it must be clearly
    /// non-zero. If the table over-reports, the first check fails; if it
    /// under-reports, the second does.
    #[test]
    #[ignore = "sweeps the hue/lightness grid against an optimiser; slow"]
    fn test_cmax_table_agrees_with_reachability_oracle() {
        use crate::gamut::cmax::CmaxTable;
        use crate::gamut::hull::Hull;
        use crate::Oklch;

        let palette = six_color_palette();
        let table = CmaxTable::build(&Hull::from_palette(&palette));

        let mut at_limit_worst = 0.0f32;
        let mut beyond_limit_min = f32::MAX;
        let mut checked = 0;

        for hi in 0..24 {
            let h = -std::f32::consts::PI
                + (hi as f32 / 24.0) * std::f32::consts::TAU;
            for li in 1..16 {
                let l = li as f32 / 16.0;
                let c_max = table.sample(h, l);
                if c_max < 1e-3 {
                    continue;
                }
                checked += 1;

                // Just inside the reported limit: must be reachable. Compared
                // RELATIVE to c_max — see the ruling below.
                let inside = Oklab::from(Oklch { l, c: c_max * 0.9, h });
                let (d_in, _) = best_reachable(&palette, inside);
                at_limit_worst = at_limit_worst.max(d_in / c_max);

                // Well beyond: must not be.
                let outside = Oklab::from(Oklch { l, c: c_max * 2.5, h });
                let (d_out, _) = best_reachable(&palette, outside);
                beyond_limit_min = beyond_limit_min.min(d_out / c_max);
            }
        }

        eprintln!(
            "cmax oracle: checked {checked} bins, worst in-limit ratio {at_limit_worst:.4}, \
             smallest beyond-limit ratio {beyond_limit_min:.4}"
        );
        assert!(checked > 100, "grid produced too few usable bins: {checked}");
        assert!(
            at_limit_worst < IN_LIMIT_MAX_RATIO,
            "table over-reports Cmax: a point at 0.9*Cmax was {at_limit_worst:.4}*Cmax from the hull"
        );
        assert!(
            beyond_limit_min > BEYOND_LIMIT_MIN_RATIO,
            "table under-reports Cmax: a point at 2.5*Cmax was only \
             {beyond_limit_min:.4}*Cmax from the hull"
        );
    }
```

**Ruling (2026-08-08): the thresholds are RELATIVE to `Cmax`, and the oracle is
fixed — `cmax.rs` is NOT.** The first draft of this task asserted absolute dE
(`< 0.02` / `> 0.02`) and instructed that a failure meant the table was wrong.
Both halves were mistaken, and measurement settled it:

1. **The absolute threshold is structurally wrong.** Both statistics scale with
   `Cmax`, and `Cmax → 0` at both lightness extremes. Even discounting the
   darkest row, `beyond_limit_min` was `0.02100` against a `0.02` threshold — a
   1.05× margin. Note also that `d_out` is a 3-D distance whose nearest hull
   point need not be radial, so `d_out < 1.5*Cmax` never demonstrated
   under-reporting; the check only establishes "not on the hull". Keep it
   generous.
2. **The dark-row failure was the ORACLE, not the table.** `best_reachable` is
   coordinate descent; from a pure-black start, growing the diluting weight is a
   zero-gradient move (the cost normalises by the weight sum) and the smallest
   ink step overshoots `L = 0.0625`, so the descent halts at pure black and
   reports the target's own chroma as the distance. Witness: at
   `(L=0.06250, C=0.01219, h=1.571)` the stock oracle returns `0.01219`
   ("nothing reachable"); with dilute near-black starts and the ladder extended
   to `0.0001` it returns `0.00007` — 163× better — landing at `C=0.01225`,
   which is 90.5% of the table's reported `0.01354` and so **vindicates the
   table**. Task 3 stands; `cmax.rs` is unchanged.

Therefore this task must **first repair `best_reachable` in place** (add dilute
near-black starts; extend the step ladder with `0.001, 0.0005, 0.0001`), which
strictly improves every caller. Measured effect: the worst `d_in/Cmax` ratio is
`0.90` at `L=0.0625` and `0.0916` at `L=0.125` with the stock ladder, and
`≤0.0083` on every row once repaired. All six existing callers are `#[ignore]`d
diagnostics, so the default suite is unaffected.

`IN_LIMIT_MAX_RATIO` and `BEYOND_LIMIT_MIN_RATIO` are named constants whose
values are **measured after the oracle repair**, not guessed — see Step 3.

If a `six_color_palette()` helper does not already exist in that module, add it directly above the test:

```rust
    /// The six-ink panel palette, official colours.
    fn six_color_palette() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(0, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(255, 255, 0),
            ],
            None,
        )
        .unwrap()
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither test_cmax_table_agrees -- --ignored --nocapture`

`six_color_palette()` does **not** exist in that module and there is no
equivalent to reuse, so add it — there is no collision to resolve.

Expected once `best_reachable` is repaired: **PASS**. If it still fails, do
**not** adjust a threshold and do **not** edit `cmax.rs` — report it. The table
has been independently validated against a repaired oracle (see the ruling
above), so a further failure means something not yet understood.

- [ ] **Step 3: Record the measured numbers**

Set the two named constants from the **repaired** oracle's measured extremes,
with margin, and record the real observed values — never the illustrative
figures a draft of this plan once carried. Reference points measured on
2026-08-08: with the stock oracle the worst in-limit ratio was `0.90`
(`L = 0.0625`); repaired, every row is `≤ 0.0083`. The smallest beyond-limit
ratio was `0.4582` with the stock oracle, and **will decrease** once the oracle
is repaired, because a better optimiser finds closer points — so it must be
re-measured, not carried over.

Choose each constant to clear its measured extreme by a stated margin (aim for
≥3× on the in-limit ratio, ≥1.5× on the beyond-limit ratio), rounded to a clean
value, and put the measurement and the margin in the doc comment, e.g.:

```rust
    /// Measured 2026-08-08 on the six-ink official palette, repaired oracle:
    /// N bins, worst in-limit ratio X (constant A, M× margin), smallest
    /// beyond-limit ratio Y (constant B, M× margin).
```

- [ ] **Step 4: Run again to confirm**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither test_cmax_table_agrees -- --ignored --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/domain_tests.rs
git commit -m "test(eink-dither): validate the Cmax table against the exact reachability oracle"
```

---

### Task 8: The SVG tone-mask rewriter

Produce a second SVG whose paint is white inside `data-byonk-tone="continuous"` subtrees and black everywhere else. Recolouring rather than deleting makes **occlusion just work**: an unmarked shape covering part of a marked photo correctly masks it out, because the renderer resolves z-order for us.

**Files:**
- Create: `src/rendering/tone_mask.rs`
- Modify: `src/rendering/mod.rs`, `Cargo.toml`

**Interfaces:**
- Produces:
  ```rust
  pub const TONE_ATTR: &str = "data-byonk-tone";
  pub const TONE_GROUP_ATTR: &str = "data-byonk-tone-group";
  /// Cheap presence check: does this document mark anything at all?
  pub fn has_tone_markup(svg: &[u8]) -> bool;
  /// Rewrite into a mask document. Errors only on malformed XML.
  pub fn build_mask_svg(svg: &[u8]) -> Result<Vec<u8>, ToneMaskError>;

  #[derive(Debug, thiserror::Error)]
  pub enum ToneMaskError {
      #[error("mask rewrite failed: {0}")]
      Xml(String),
  }
  ```

- [ ] **Step 1: Add the dependency**

```bash
CARGO_BUILD_JOBS=2 cargo add quick-xml
```

Then add a comment above the new line in `Cargo.toml`:

```toml
# Streaming XML read+write for the tone-mask rewriter (src/rendering/tone_mask.rs).
# Needed because the mask document must round-trip everything it does not touch;
# roxmltree (already present via usvg) is read-only.
```

- [ ] **Step 2: Write the failing tests**

Create `src/rendering/tone_mask.rs` with only this test module:

> **The `r##"…"##` delimiters below are load-bearing, not a style choice.** A
> plain `r#"…"#` raw string is terminated by the first `"#` sequence, and SVG is
> full of them (`fill="#ff0000"`, `href="#sym"`). Every literal that contains
> `"#` must use `r##"…"##`. Verified empirically — with `r#"…"#` these tests do
> not compile. Do not "simplify" the delimiters.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn mask_of(svg: &str) -> String {
        String::from_utf8(build_mask_svg(svg.as_bytes()).expect("rewrite must succeed")).unwrap()
    }

    #[test]
    fn presence_check_is_exact() {
        assert!(!has_tone_markup(br#"<svg><rect fill="red"/></svg>"#));
        assert!(has_tone_markup(
            br#"<svg><g data-byonk-tone="continuous"><rect/></g></svg>"#
        ));
        // A near-miss must not trigger the expensive path.
        assert!(!has_tone_markup(br#"<svg data-byonk-tone-ish="x"/></svg>"#));
    }

    #[test]
    fn unmarked_shapes_become_black() {
        let out = mask_of(r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a"/></g><rect id="b" fill="#ff0000"/></svg>"##);
        let b = out.split(r#"id="b""#).nth(1).unwrap();
        assert!(b.contains("#000000"), "unmarked rect must be black: {b}");
        assert!(!b.contains("#ff0000"), "original paint must be gone: {b}");
    }

    #[test]
    fn marked_shapes_become_white() {
        let out = mask_of(r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a" fill="#123456"/></g></svg>"##);
        let a = out.split(r#"id="a""#).nth(1).unwrap();
        assert!(a.contains("#ffffff"), "marked rect must be white: {a}");
    }

    #[test]
    fn marking_is_inherited_by_descendants() {
        let out = mask_of(r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><g><circle id="deep"/></g></g></svg>"#);
        let d = out.split(r#"id="deep""#).nth(1).unwrap();
        assert!(d.contains("#ffffff"), "descendant must inherit continuous: {d}");
    }

    #[test]
    fn a_descendant_can_override_back_to_graphic() {
        let out = mask_of(r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="bg"/><text id="label" data-byonk-tone="graphic">18:42</text></g></svg>"#);
        let bg = out.split(r#"id="bg""#).nth(1).unwrap();
        let label = out.split(r#"id="label""#).nth(1).unwrap();
        assert!(bg.contains("#ffffff"), "background must be marked: {bg}");
        assert!(label.contains("#000000"), "override must unmark the label: {label}");
    }

    #[test]
    fn tone_scope_closes_with_its_element() {
        let out = mask_of(r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="in"/></g><rect id="out"/></svg>"#);
        assert!(out.split(r#"id="in""#).nth(1).unwrap().contains("#ffffff"));
        assert!(out.split(r#"id="out""#).nth(1).unwrap().contains("#000000"));
    }

    #[test]
    fn self_closing_marked_element_does_not_leak_scope() {
        let out = mask_of(r#"<svg xmlns="http://www.w3.org/2000/svg"><rect id="m" data-byonk-tone="continuous"/><rect id="after"/></svg>"#);
        assert!(out.split(r#"id="m""#).nth(1).unwrap().contains("#ffffff"));
        assert!(
            out.split(r#"id="after""#).nth(1).unwrap().contains("#000000"),
            "a self-closing marked element must not mark its siblings"
        );
    }

    #[test]
    fn fill_none_is_preserved() {
        let out = mask_of(r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a" fill="none" stroke="#f00"/></g></svg>"##);
        let a = out.split(r#"id="a""#).nth(1).unwrap();
        assert!(a.contains(r#"fill="none""#), "fill:none must survive: {a}");
        assert!(a.contains(r##"stroke="#ffffff""##), "stroke must be marked: {a}");
    }

    #[test]
    fn css_paint_declarations_are_stripped_but_geometry_survives() {
        let out = mask_of(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><defs><style>.date { font-size: 11px; fill: #555555; stroke: red; font-family: Outfit; }</style></defs><text class="date" id="t">x</text></svg>"#,
        );
        assert!(!out.contains("#555555"), "CSS fill must be stripped: {out}");
        assert!(!out.contains("stroke: red"), "CSS stroke must be stripped: {out}");
        assert!(out.contains("font-size: 11px"), "geometry CSS must survive: {out}");
        assert!(out.contains("font-family: Outfit"), "geometry CSS must survive: {out}");
    }

    #[test]
    fn images_become_rects_over_their_box() {
        let out = mask_of(r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><image id="p" x="10" y="20" width="100" height="50" href="p.png"/></g></svg>"#);
        assert!(!out.contains("<image"), "image must be replaced: {out}");
        assert!(out.contains(r#"x="10""#) && out.contains(r#"width="100""#), "box must survive: {out}");
        assert!(out.contains("#ffffff"), "image box must be marked: {out}");
    }

    #[test]
    fn defs_content_loses_paint_so_use_sites_decide() {
        let out = mask_of(r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><g id="sym"><rect id="sr" fill="#abcdef"/></g></defs><use href="#sym" id="u"/></svg>"##);
        // Scope to this element's own tag — the rest of the document legitimately
        // contains painted elements, and an unscoped tail would match them.
        let sr = out.split(r#"id="sr""#).nth(1).unwrap().split('>').next().unwrap();
        assert!(!sr.contains("#abcdef"), "defs paint must be stripped: {sr}");
        assert!(!sr.contains(r##"fill="#"##), "defs must not gain paint either: {sr}");
        assert!(out.split(r#"id="u""#).nth(1).unwrap().contains("#000000"));
    }

    #[test]
    fn start_form_image_is_replaced_and_its_subtree_dropped() {
        // `<image>…</image>` is legal SVG (it may carry `<title>`/`<desc>`).
        // It must be replaced exactly like the self-closing form; leaving it
        // intact would put the real photograph into the mask document, where
        // its pixels would threshold into an arbitrary mask.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><image id="p" x="10" y="20" width="100" height="50" href="p.png"><title>a caption</title></image></g><rect id="after"/></svg>"##,
        );
        assert!(!out.contains("<image"), "start-form image must be replaced: {out}");
        assert!(!out.contains("</image>"), "no orphan end tag: {out}");
        assert!(!out.contains("a caption"), "image subtree must be dropped: {out}");
        let p = out.split(r#"id="p""#).nth(1).unwrap();
        assert!(p.contains(r#"width="100""#), "box must survive: {p}");
        assert!(p.contains("#ffffff"), "image box must be marked: {p}");
        assert!(
            out.split(r#"id="after""#).nth(1).unwrap().contains("#000000"),
            "swallowing the subtree must not disturb later siblings: {out}"
        );
    }

    #[test]
    fn css_paint_is_stripped_case_insensitively_and_around_whitespace() {
        // CSS property names are case-insensitive and allow whitespace before
        // the colon. A paint declaration that survives into the mask beats the
        // presentation attribute and silently inverts that element's polarity.
        for decl in [
            "FILL: red;",
            "Fill: red;",
            "fill : red;",
            "STROKE: red;",
            "fill\t: red;",
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.d {{ {decl} }}</style><rect class="d" id="r"/></svg>"#
            );
            let out = mask_of(&svg);
            assert!(!out.contains("red"), "{decl} must be stripped: {out}");
        }
    }

    #[test]
    fn paint_is_written_to_the_inline_style_as_well() {
        // A stylesheet rule beats a presentation attribute, so the paint must
        // also be in the inline style, which beats both.
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a"/></g><rect id="b"/></svg>"##,
        );
        let a = out.split(r#"id="a""#).nth(1).unwrap().split('>').next().unwrap();
        assert!(a.contains("style="), "marked element needs an inline style: {a}");
        assert!(a.contains("fill:#ffffff"), "inline style must carry paint: {a}");
        assert!(a.contains("stroke:#ffffff"), "inline style must carry stroke: {a}");
        let b = out.split(r#"id="b""#).nth(1).unwrap().split('>').next().unwrap();
        assert!(b.contains("fill:#000000"), "unmarked element inline style: {b}");
    }

    #[test]
    fn inline_style_keeps_geometry_and_replaces_only_paint() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><text id="t" style="font-size:11px;fill:#555555">x</text></g></svg>"##,
        );
        let t = out.split(r#"id="t""#).nth(1).unwrap().split('>').next().unwrap();
        assert!(t.contains("font-size:11px"), "geometry must survive: {t}");
        assert!(!t.contains("#555555"), "original paint must be gone: {t}");
        assert!(t.contains("fill:#ffffff"), "our paint must be present: {t}");
    }

    #[test]
    fn defs_content_gets_no_inline_paint() {
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><rect id="sr" style="font-size:9px;fill:#abcdef"/></defs></svg>"##,
        );
        let sr = out.split(r#"id="sr""#).nth(1).unwrap().split('>').next().unwrap();
        assert!(sr.contains("font-size:9px"), "geometry must survive in defs: {sr}");
        assert!(!sr.contains("fill:"), "defs must gain no paint: {sr}");
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_silent_fallback() {
        assert!(build_mask_svg(b"<svg><g></svg>").is_err());
    }

    #[test]
    fn tone_attributes_are_dropped_from_the_mask_document() {
        let out = mask_of(r#"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous" data-byonk-tone-group="sky"><rect/></g></svg>"#);
        assert!(!out.contains("data-byonk-tone"), "marker must not survive: {out}");
    }
}
```

- [ ] **Step 3: Implement the rewriter**

Prepend to `src/rendering/tone_mask.rs`:

```rust
//! Rewrites an SVG into a mask document for gamut mapping.
//!
//! Every element inside a `data-byonk-tone="continuous"` subtree is painted
//! white; everything else is painted black. Rasterizing the result with the
//! same renderer, over a black background, yields a per-pixel mask saying
//! which pixels belong to a continuous-tone region.
//!
//! Recolouring rather than deleting is deliberate: it makes **occlusion just
//! work**, because an unmarked shape covering part of a marked photo correctly
//! masks it out — the renderer resolves z-order for us.
//!
//! # CSS
//!
//! A CSS rule beats a presentation attribute, and screen templates do set
//! `fill` from `<style>` blocks. Rather than depend on stylesheet precedence,
//! this rewriter **strips paint declarations from `<style>` content** and sets
//! paint on the elements. Geometry-affecting declarations are preserved,
//! because they change what area is covered.
//!
//! # Known mis-marking
//!
//! Three cases move the mask edge slightly. All three are accepted:
//!
//! - An `<image>` becomes a `<rect>` over its layout box, so a transparent or
//!   letterboxed image marks its whole box. This applies to both the
//!   self-closing form and `<image>…</image>`, whose subtree is dropped.
//! - An element painted `none` only via CSS becomes painted here.
//! - A stroke set only by a stylesheet rule is lost, because paint
//!   declarations are stripped before the stroke is resolved. That element
//!   under-marks by half its stroke width.
//!
//! The first two only ever *grow* the region, which is harmless: the mask
//! background is already black, and growing it inside a marked region maps a
//! few extra background pixels, where mapping in-gamut content is the identity.
//!
//! The third *shrinks* it, and that is the deliberate fail-safe direction. The
//! alternative — painting `stroke` unconditionally — invents a stroke on every
//! unstroked shape, since SVG's initial `stroke` is `none`, and moves edges by
//! an unbounded `stroke-width / 2`.

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::io::Cursor;

/// Marks an element and its descendants as continuous-tone.
pub const TONE_ATTR: &str = "data-byonk-tone";
/// Names the adaptation group an element belongs to.
pub const TONE_GROUP_ATTR: &str = "data-byonk-tone-group";

const WHITE: &str = "#ffffff";
const BLACK: &str = "#000000";

/// Paint properties whose value must come from us, not the document.
const PAINT_PROPS: [&str; 8] = [
    "fill",
    "stroke",
    "fill-opacity",
    "stroke-opacity",
    "opacity",
    "color",
    "stop-color",
    "stop-opacity",
];

#[derive(Debug, thiserror::Error)]
pub enum ToneMaskError {
    #[error("mask rewrite failed: {0}")]
    Xml(String),
}

/// Does this document mark anything at all?
///
/// Cheap enough to run on every render; when it returns false the caller skips
/// the mask rasterization entirely and the document renders exactly as it does
/// today.
pub fn has_tone_markup(svg: &[u8]) -> bool {
    svg.windows(TONE_ATTR.len() + 1).any(|w| {
        w[..TONE_ATTR.len()] == *TONE_ATTR.as_bytes()
            && (w[TONE_ATTR.len()] == b'=' || w[TONE_ATTR.len()] == b' ')
    })
}

/// Effective tone of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Graphic,
    Continuous,
}

impl Tone {
    fn paint(self) -> &'static str {
        match self {
            Tone::Continuous => WHITE,
            Tone::Graphic => BLACK,
        }
    }
}

/// Rewrite `svg` into its mask document.
pub fn build_mask_svg(svg: &[u8]) -> Result<Vec<u8>, ToneMaskError> {
    let mut reader = Reader::from_reader(svg);
    reader.config_mut().check_end_names = true;
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // Effective tone for each open element, innermost last.
    let mut tone_stack: Vec<Tone> = vec![Tone::Graphic];
    // Depth of open `<defs>` elements — content there is stripped, not painted.
    let mut defs_depth: usize = 0;
    // Depth of open `<style>` elements — text there is a stylesheet.
    let mut style_depth: usize = 0;
    // Depth inside a start-form `<image>` whose subtree we are swallowing.
    // `<image>…</image>` is legal and may hold `<title>`/`<desc>`; the element
    // is replaced by a rect, so nothing inside it may reach the mask.
    let mut image_skip_depth: usize = 0;
    let mut buf = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| ToneMaskError::Xml(e.to_string()))?
        {
            Event::Eof => break,

            Event::Start(e) => {
                // Anything nested inside a replaced `<image>` is dropped.
                if image_skip_depth > 0 {
                    image_skip_depth += 1;
                    buf.clear();
                    continue;
                }
                let name = e.name().as_ref().to_vec();
                let tone = resolve_tone(&e, *tone_stack.last().unwrap());
                if name == b"image" {
                    // Start-form image: emit the rect and swallow the subtree.
                    // No tone_stack push — the matching End is swallowed too.
                    let rect = image_to_rect(&e, tone, defs_depth > 0)?;
                    writer
                        .write_event(Event::Empty(rect))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                    image_skip_depth = 1;
                    buf.clear();
                    continue;
                }
                tone_stack.push(tone);
                if name == b"defs" {
                    defs_depth += 1;
                }
                if name == b"style" {
                    style_depth += 1;
                }
                let out = rewrite_start(&e, tone, defs_depth > 0)?;
                writer
                    .write_event(Event::Start(out))
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }

            Event::End(e) => {
                if image_skip_depth > 0 {
                    image_skip_depth -= 1;
                    buf.clear();
                    continue;
                }
                let name = e.name().as_ref().to_vec();
                if name == b"defs" {
                    defs_depth = defs_depth.saturating_sub(1);
                }
                if name == b"style" {
                    style_depth = style_depth.saturating_sub(1);
                }
                if tone_stack.len() > 1 {
                    tone_stack.pop();
                }
                writer
                    .write_event(Event::End(e))
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }

            Event::Empty(e) => {
                if image_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                // Self-closing: its tone applies to itself only, never to its
                // siblings.
                let tone = resolve_tone(&e, *tone_stack.last().unwrap());
                if e.name().as_ref() == b"image" {
                    let rect = image_to_rect(&e, tone, defs_depth > 0)?;
                    writer
                        .write_event(Event::Empty(rect))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                } else {
                    let out = rewrite_start(&e, tone, defs_depth > 0)?;
                    writer
                        .write_event(Event::Empty(out))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                }
            }

            Event::Text(t) => {
                if image_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                if style_depth > 0 {
                    // `xml10_content` is quick-xml 0.41's unescaping accessor;
                    // the older `unescape()` no longer exists.
                    let css = t
                        .xml10_content()
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                    let cleaned = strip_paint_declarations(&css);
                    writer
                        .write_event(Event::Text(BytesText::new(&cleaned)))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                } else {
                    writer
                        .write_event(Event::Text(t))
                        .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
                }
            }

            Event::CData(c) if style_depth > 0 => {
                let css = String::from_utf8_lossy(c.as_ref()).to_string();
                let cleaned = strip_paint_declarations(&css);
                writer
                    .write_event(Event::Text(BytesText::new(&cleaned)))
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }

            other => {
                if image_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                writer
                    .write_event(other)
                    .map_err(|e| ToneMaskError::Xml(e.to_string()))?;
            }
        }
        buf.clear();
    }

    Ok(writer.into_inner().into_inner())
}

/// An element's effective tone: its own attribute if present, else its parent's.
fn resolve_tone(e: &BytesStart, inherited: Tone) -> Tone {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == TONE_ATTR.as_bytes() {
            return match attr.value.as_ref() {
                b"continuous" => Tone::Continuous,
                _ => Tone::Graphic,
            };
        }
    }
    inherited
}

/// Copy an element, replacing paint with the mask colour.
///
/// Inside `<defs>` paint is stripped instead, so a `<use>` site decides the
/// polarity of the content it pulls in.
fn rewrite_start(
    e: &BytesStart,
    tone: Tone,
    in_defs: bool,
) -> Result<BytesStart<'static>, ToneMaskError> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut out = BytesStart::new(name);

    let mut fill_none = false;
    let mut stroke_none = false;
    let mut kept_style = String::new();

    for attr in e.attributes().with_checks(false) {
        let attr = attr.map_err(|e| ToneMaskError::Xml(e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = String::from_utf8_lossy(attr.value.as_ref()).into_owned();

        // The markers are ours; they must not survive into the mask document.
        if key == TONE_ATTR || key == TONE_GROUP_ATTR {
            continue;
        }
        // Paint we are about to set ourselves.
        if PAINT_PROPS.contains(&key.as_str()) {
            if key == "fill" && value.trim() == "none" {
                fill_none = true;
            }
            if key == "stroke" && value.trim() == "none" {
                stroke_none = true;
            }
            continue;
        }
        if key == "style" {
            // Held back until the paint is known, so both land in one attribute.
            kept_style = strip_paint_declarations_inline(&value);
            if value.contains("fill:none") || value.contains("fill: none") {
                fill_none = true;
            }
            if value.contains("stroke:none") || value.contains("stroke: none") {
                stroke_none = true;
            }
            continue;
        }
        out.push_attribute(Attribute::from((key.as_str(), value.as_str())));
    }

    if !in_defs {
        let paint = tone.paint();
        let fill = if fill_none { "none" } else { paint };
        let stroke = if stroke_none { "none" } else { paint };
        out.push_attribute(Attribute::from(("fill", fill)));
        out.push_attribute(Attribute::from(("stroke", stroke)));
        out.push_attribute(Attribute::from(("fill-opacity", "1")));
        out.push_attribute(Attribute::from(("stroke-opacity", "1")));
        out.push_attribute(Attribute::from(("opacity", "1")));
        // Belt and braces: a stylesheet rule beats a presentation attribute, so
        // the paint goes in the inline style too. Stripping is the first line of
        // defence; this is what holds if a paint declaration ever survives it.
        push_style(&mut out, &kept_style, Some((fill, stroke)));
    } else {
        push_style(&mut out, &kept_style, None);
    }

    Ok(out)
}

/// Write the `style` attribute, merging the document's surviving declarations
/// with our paint. Omitted entirely when there is nothing to say.
fn push_style(out: &mut BytesStart<'static>, kept: &str, paint: Option<(&str, &str)>) {
    let mut style = String::new();
    let kept = kept.trim().trim_end_matches(';');
    if !kept.is_empty() {
        style.push_str(kept);
        style.push(';');
    }
    if let Some((fill, stroke)) = paint {
        style.push_str(&format!(
            "fill:{fill};stroke:{stroke};fill-opacity:1;stroke-opacity:1;opacity:1"
        ));
    }
    let style = style.trim_end_matches(';');
    if !style.is_empty() {
        out.push_attribute(Attribute::from(("style", style)));
    }
}

/// Replace an `<image>` with a solid rect over its layout box.
///
/// An image's pixels are not a paint, so it cannot be recoloured. The box is
/// the closest honest approximation; see the module docs on over-marking.
fn image_to_rect(
    e: &BytesStart,
    tone: Tone,
    in_defs: bool,
) -> Result<BytesStart<'static>, ToneMaskError> {
    let mut rect = BytesStart::new("rect");

    for attr in e.attributes().with_checks(false) {
        let attr = attr.map_err(|e| ToneMaskError::Xml(e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
        // Geometry and placement carry over; the pixel source does not.
        if matches!(
            key.as_str(),
            "x" | "y" | "width" | "height" | "transform" | "clip-path" | "mask" | "id" | "class"
        ) {
            rect.push_attribute(Attribute::from((key.as_str(), value.as_str())));
        }
    }

    if !in_defs {
        let paint = tone.paint();
        rect.push_attribute(Attribute::from(("fill", paint)));
        rect.push_attribute(Attribute::from(("stroke", "none")));
        rect.push_attribute(Attribute::from(("fill-opacity", "1")));
        rect.push_attribute(Attribute::from(("opacity", "1")));
        push_style(&mut rect, "", Some((paint, "none")));
    }

    Ok(rect)
}

/// Remove paint declarations from a stylesheet, keeping geometry ones.
fn strip_paint_declarations(css: &str) -> String {
    // Operate declaration by declaration; braces and selectors pass through.
    let mut out = String::with_capacity(css.len());
    for chunk in css.split_inclusive([';', '{', '}']) {
        if is_paint_declaration(chunk) {
            continue;
        }
        out.push_str(chunk);
    }
    out
}

/// Same, for a `style="..."` attribute value (no selectors or braces).
fn strip_paint_declarations_inline(style: &str) -> String {
    style
        .split(';')
        .filter(|d| !is_paint_declaration(d))
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .collect::<Vec<_>>()
        .join(";")
}

/// Does this declaration set a paint property?
fn is_paint_declaration(decl: &str) -> bool {
    let body = decl.trim_end_matches([';', '{', '}']);
    let Some((prop, _)) = body.rsplit_once(':') else {
        return false;
    };
    // The property name is the last token before the colon — everything
    // before it is a selector or the tail of a previous declaration.
    // CSS property names are case-insensitive, and whitespace is legal before
    // the colon (`fill : red`). Both forms must be recognised: a paint
    // declaration that survives into the mask beats our presentation attribute
    // and silently inverts that element's mask polarity.
    let prop = prop
        .trim_end()
        .rsplit([' ', '\t', '\n', '{', '}', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    PAINT_PROPS.contains(&prop.as_str())
}
```

Add `pub mod tone_mask;` to `src/rendering/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test tone_mask`
Expected: PASS, 18 tests

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/rendering/tone_mask.rs src/rendering/mod.rs
git commit -m "feat(rendering): SVG tone-mask rewriter for continuous-tone regions"
```

---

### Task 9: Rasterize the mask to a per-pixel boolean

**Files:**
- Modify: `src/rendering/svg_to_png.rs`

**Interfaces:**
- Consumes: `tone_mask::build_mask_svg`, `SvgRenderer::rasterize_svg` (private).
- Produces: `SvgRenderer::rasterize_tone_mask(&self, svg_data: &[u8], spec: DisplaySpec) -> Result<Vec<bool>, RenderError>` — length `width * height`, `true` where continuous-tone.

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` in `src/rendering/svg_to_png.rs`:

**The `r##"…"##` delimiters are load-bearing.** Both literals contain
`fill="#ffffff"` / `fill="#336699"` / `fill="#000000"`, and the sequence `"#`
terminates a `r#"…"#` raw string — with single hashes the test module is a
syntax error. Do not "simplify" the delimiters, and do not change the SVG
content to avoid them.

```rust
    #[test]
    fn tone_mask_marks_only_the_marked_region() {
        let renderer = SvgRenderer::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="50" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let mask = renderer
            .rasterize_tone_mask(svg.as_bytes(), spec)
            .expect("mask must rasterize");

        assert_eq!(mask.len(), 100 * 100);
        assert!(mask[50 * 100 + 25], "left half must be marked");
        assert!(!mask[50 * 100 + 75], "right half must not be marked");
    }

    #[test]
    fn tone_mask_respects_occlusion_by_unmarked_shapes() {
        let renderer = SvgRenderer::new();
        // A marked photo area with an unmarked label drawn over its middle.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="100" height="100" fill="#336699"/>
            </g>
            <rect x="40" y="40" width="20" height="20" fill="#000000"/>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let mask = renderer.rasterize_tone_mask(svg.as_bytes(), spec).unwrap();
        assert!(mask[10 * 100 + 10], "photo area must be marked");
        assert!(
            !mask[50 * 100 + 50],
            "the occluding unmarked rect must punch through the mask"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `CARGO_BUILD_JOBS=2 cargo test tone_mask_marks tone_mask_respects`
Expected: FAIL to compile — `no method named `rasterize_tone_mask``

- [ ] **Step 3: Implement mask rasterization**

Add to `impl SvgRenderer` in `src/rendering/svg_to_png.rs`, after `rasterize_svg`:

```rust
    /// Rasterize the tone mask for `svg_data`.
    ///
    /// The mask document is the original with paint forced to white inside
    /// `data-byonk-tone="continuous"` subtrees and black elsewhere, drawn over
    /// a black background so unpainted area reads as unmarked. Edge
    /// antialiasing produces greys; threshold at 0.5.
    ///
    /// Failure is a hard error, deliberately. The mask comes from a document
    /// that just rasterized successfully, by the same renderer, with only
    /// paint values changed — so the realistic failure paths are all our own
    /// bugs in the rewriter. Silently rendering something materially different
    /// while reporting success is the failure mode that costs hours.
    // Task 10 wires this into `render_to_palette_png`; until then the lib
    // build has no caller and `clippy --all-targets -D warnings` rejects it
    // as dead code. `#[expect]` is wrong here — the cfg(test) build *does*
    // use it, so the expectation goes unfulfilled and that is a warning too.
    // Task 10 removes this attribute.
    #[allow(dead_code)]
    fn rasterize_tone_mask(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
    ) -> Result<Vec<bool>, RenderError> {
        let mask_svg = crate::rendering::tone_mask::build_mask_svg(svg_data)
            .map_err(|e| RenderError::SvgParse(format!("tone mask: {e}")))?;

        let options = usvg::Options {
            fontdb: self.fontdb.clone(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_data(&mask_svg, &options)
            .map_err(|e| RenderError::SvgParse(format!("tone mask: {e}")))?;

        let svg_size = tree.size();
        let scale = (spec.width as f32 / svg_size.width())
            .min(spec.height as f32 / svg_size.height());
        let offset_x = (spec.width as f32 - svg_size.width() * scale) / 2.0;
        let offset_y = (spec.height as f32 - svg_size.height() * scale) / 2.0;

        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::BLACK);

        let transform = Transform::from_scale(scale, scale).post_translate(offset_x, offset_y);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap
            .data()
            .chunks_exact(4)
            .map(|px| {
                // Premultiplied RGBA over an opaque black fill: the green
                // channel alone separates white from black cleanly.
                px[1] >= 128
            })
            .collect())
    }
```

Note the scale/offset computation duplicates `rasterize_svg`'s; that is intentional and load-bearing — the mask must use **exactly** the same transform as the frame. Extract it into a shared private helper and call it from both, so they cannot drift. It takes plain floats, not a size type:

```rust
    /// The scale-and-centre transform that fits an SVG of `svg_w` x `svg_h`
    /// into `spec`.
    ///
    /// Shared by the frame and the tone mask: if these two ever disagreed the
    /// mask would be offset from the pixels it selects. Takes plain floats
    /// rather than a size type so it does not depend on which crate
    /// `usvg::Tree::size` currently returns.
    fn fit_transform(svg_w: f32, svg_h: f32, spec: DisplaySpec) -> Transform {
        let scale = (spec.width as f32 / svg_w).min(spec.height as f32 / svg_h);
        let offset_x = (spec.width as f32 - svg_w * scale) / 2.0;
        let offset_y = (spec.height as f32 - svg_h * scale) / 2.0;
        Transform::from_scale(scale, scale).post_translate(offset_x, offset_y)
    }
```

Replace the inline computation in both `rasterize_svg` and `rasterize_tone_mask` with:

```rust
        let svg_size = tree.size();
        let transform = Self::fit_transform(svg_size.width(), svg_size.height(), spec);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib tone_mask`
Expected: PASS, 20 tests (18 rewriter + 2 rasterization).

Then confirm the tree stays green for the lib build too, which is where the
dead-code allow earns its place:
`CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings`

- [ ] **Step 5: Commit**

```bash
git add src/rendering/svg_to_png.rs
git commit -m "feat(rendering): rasterize the tone mask with the frame's exact transform"
```

---

### Task 9b: Never invent a stroke in the mask document

**Why this task exists.** Task 9 was the first code to *rasterize* a mask, and
that immediately exposed a defect in Task 8's rewriter that no rewriter test
could see — every `tone_mask` test asserts on the mask **document text**, and
this defect is only observable in pixels.

`rewrite_start` sets `stroke` unconditionally to the tone paint. SVG's initial
`stroke` is `none`, so **every marked shape that declared no stroke gains one**,
at the initial `stroke-width` of 1, centred on its edge. Measured on a shape
spanning `x = 50..=149` at a 200x200 spec:

| document | mask span before | should be |
|---|---|---|
| plain unstroked rect | `50..=150` | `50..=149` |
| `<style>.p{stroke:none;stroke-width:20}</style>` | `40..=159` | `50..=149` |

The second row is the important one: the error is **not sub-pixel and not
bounded**. `stroke` is a paint property, so the author's `stroke: none` is
*stripped* from the stylesheet, while `stroke-width` is deliberately *preserved*
as geometry. The two rules combine into a mask error of `stroke-width / 2`.

The harmful direction is not the over-mark (mapping in-gamut content is the
identity). It is that an **unmarked** shape drawn over a marked photo gains a
**black** stroke and *erodes* the photo mask by `stroke-width / 2` all round —
a visible unmapped band around every label on a photo.

**Owner ruling (2026-08-08): stroke-evidence stack, fixed before Task 10.**
Set `stroke` to the tone paint only where the element actually has stroke
evidence, tracked through an inheritance stack exactly like the existing tone
stack. Two traps this must avoid, and which a blanket `stroke="none"` would
fall into:

- **Stroke-only shapes.** A `<line>`, or a `<path fill="none" stroke="…">`, has
  no fill area at all. Drop its stroke and it vanishes from the mask entirely —
  a marked one is never mapped, an unmarked one never occludes.
- **Inherited stroke.** `<g stroke="black"><line/></g>` is legal: the line has
  no stroke of its own but inherits one. Writing an explicit `stroke="none"` on
  it would *override* the inherited paint and erase it.

A stroke set **only** by a stylesheet rule cannot be seen, because paint
declarations are stripped before this runs. Such an element loses its stroke and
therefore *under*-marks by half its stroke width. That is the fail-safe
direction, and it becomes the third documented known case.

**Files:**
- Modify: `src/rendering/tone_mask.rs`
- Modify: `src/rendering/svg_to_png.rs` (one rasterization regression test)

**A validated reference implementation** — this exact design, built and passing
the full gate — is saved at
`.superpowers/sdd/2026-08-08-gamut-mapping/task-9b-validated.diff`. Read it and
follow it; the code below is the same thing.

- [ ] **Step 1: Add the `Stroke` type**

After `impl Tone { … }` in `src/rendering/tone_mask.rs`:

```rust
/// Whether an element paints a stroke, as far as the mask can tell.
///
/// The mask must not *introduce* a stroke where the document has none. SVG's
/// initial `stroke` is `none`, so painting `stroke` unconditionally would widen
/// every marked shape by half its stroke-width — and `stroke-width` survives
/// into the mask as geometry, so that error is unbounded, not sub-pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stroke {
    /// This element or an ancestor declares a stroke that is not `none`.
    Painted,
    /// No stroke declaration up the tree, or the nearest one is `none`.
    Absent,
}
```

- [ ] **Step 2: Add the resolver and its two helpers**

Immediately before `fn resolve_tone`:

```rust
/// An element's effective stroke: its own declaration if it has one, else its
/// parent's.
///
/// Only presentation attributes and inline styles count as evidence. A stroke
/// set solely by a stylesheet rule cannot be seen here, because paint
/// declarations are stripped from stylesheets before this runs. Such an element
/// loses its stroke in the mask and therefore *under*-marks by half its stroke
/// width — the fail-safe direction, and the third known case in the module docs.
fn resolve_stroke(e: &BytesStart, inherited: Stroke) -> Stroke {
    let mut from_attr: Option<Stroke> = None;
    let mut from_style: Option<Stroke> = None;

    for attr in e.attributes().with_checks(false).flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        let value = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
        match key.as_str() {
            "stroke" => from_attr = Some(stroke_from_value(&value)),
            "style" => {
                from_style = declaration_value(&value, "stroke").map(|v| stroke_from_value(&v));
            }
            _ => {}
        }
    }

    // An inline style beats a presentation attribute, whatever the source order.
    from_style.or(from_attr).unwrap_or(inherited)
}

fn stroke_from_value(value: &str) -> Stroke {
    if value.trim().eq_ignore_ascii_case("none") {
        Stroke::Absent
    } else {
        Stroke::Painted
    }
}

/// The value of `prop` in an inline `style="..."`.
///
/// Uses the same property-name normalisation as [`is_paint_declaration`]:
/// names are case-insensitive and whitespace before the colon is legal. The
/// last declaration wins, as in CSS, and a trailing `!important` is dropped.
fn declaration_value(style: &str, prop: &str) -> Option<String> {
    let mut found = None;
    for decl in style.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        if name.trim().to_ascii_lowercase() == prop {
            let value = value.trim();
            let value = value
                .strip_suffix("!important")
                .map(str::trim_end)
                .unwrap_or(value);
            found = Some(value.trim().to_string());
        }
    }
    found
}
```

- [ ] **Step 3: Thread the stack through `build_mask_svg`**

Four edits in the event loop, each mirroring what `tone_stack` already does:

1. Next to the `tone_stack` declaration:

```rust
    // Effective stroke for each open element, innermost last. The root default
    // is SVG's initial value, `none`.
    let mut stroke_stack: Vec<Stroke> = vec![Stroke::Absent];
```

2. In `Event::Start`, directly after the `let tone = resolve_tone(…)` line:

```rust
                let stroke = resolve_stroke(&e, *stroke_stack.last().unwrap());
```

   This must sit **before** the `if name == b"image"` branch, which `continue`s.

3. In `Event::Start`, after `tone_stack.push(tone);` add `stroke_stack.push(stroke);`,
   and change the call to `rewrite_start(&e, tone, defs_depth > 0, stroke)?`.

4. In `Event::End`, after the `tone_stack` pop:

```rust
                if stroke_stack.len() > 1 {
                    stroke_stack.pop();
                }
```

5. In `Event::Empty`, add the same `resolve_stroke` line after `resolve_tone`
   (no push — a self-closing element's stroke applies to itself only), and pass
   `stroke` to `rewrite_start`.

`image_to_rect` already writes `stroke="none"` and is correct unchanged.

- [ ] **Step 4: Use the evidence in `rewrite_start`**

Change the signature to take `stroke_state: Stroke`, and replace the
`fill_none` / `stroke_none` sniffing. The `fill` detection moves to the same
`declaration_value` helper: the old `value.contains("fill:none")` was an exact
match that missed `fill : none` and `FILL: NONE`, the identical defect class the
owner already ruled on in `ba8859c`. Its failure direction flips from
over-marking to the fail-safe under-marking.

```rust
    let mut fill_none_attr: Option<bool> = None;
    let mut fill_none_style: Option<bool> = None;
```

in place of the two `bool`s, then in the attribute loop:

```rust
        if PAINT_PROPS.contains(&key.as_str()) {
            if key == "fill" {
                fill_none_attr = Some(value.trim().eq_ignore_ascii_case("none"));
            }
            continue;
        }
        if key == "style" {
            // Held back until the paint is known, so both land in one attribute.
            kept_style = strip_paint_declarations_inline(&value);
            // Same normalisation as the stylesheet stripper: `fill : NONE` is
            // a legal way to write it, and the old exact-match check missed it.
            fill_none_style =
                declaration_value(&value, "fill").map(|v| v.eq_ignore_ascii_case("none"));
            continue;
        }
```

and in the `if !in_defs` block:

```rust
        let paint = tone.paint();
        // An inline style beats a presentation attribute.
        let fill_none = fill_none_style.or(fill_none_attr).unwrap_or(false);
        let fill = if fill_none { "none" } else { paint };
        // Never introduce a stroke the document does not have: SVG's initial
        // stroke is `none`, and `stroke-width` survives into the mask, so
        // painting unconditionally would move every edge by stroke-width/2.
        let stroke = match stroke_state {
            Stroke::Painted => paint,
            Stroke::Absent => "none",
        };
```

The rest of the function is unchanged.

- [ ] **Step 5: Update the one test that encoded the old behaviour**

`paint_is_written_to_the_inline_style_as_well` asserts `stroke:#ffffff` on
`<rect id="a"/>` — an element with no stroke. That assertion **is** the defect,
written down. Give the fixture a stroked sibling and split the assertion:

```rust
        let out = mask_of(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><g data-byonk-tone="continuous"><rect id="a"/><rect id="s" stroke="#123456"/></g><rect id="b"/></svg>"##,
        );
```

then replace the `stroke:#ffffff` assertion on `a` with:

```rust
        // `#a` declares no stroke, and SVG's initial stroke is `none`. The mask
        // must not invent one: a stroke here would widen the marked area by
        // half the stroke-width, which `stroke-width` in the document can make
        // arbitrarily large.
        assert!(
            a.contains("stroke:none"),
            "an unstroked element must not gain a stroke: {a}"
        );
        // A genuinely stroked element keeps its stroke, repainted.
        let s = out
            .split(r#"id="s""#)
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            s.contains("stroke:#ffffff"),
            "a stroked element must keep its stroke, repainted: {s}"
        );
```

Every other existing test is unaffected and must keep passing unchanged.

- [ ] **Step 6: Add six rewriter tests**

Append to `mod tests` in `tone_mask.rs`. **`r##"…"##` delimiters are
load-bearing** wherever an assertion contains `stroke="#ffffff"` — the sequence
`"#` terminates a `r#"…"#` string. This bit during authoring; do not "simplify"
them.

| test | asserts |
|---|---|
| `an_unstroked_shape_gains_no_stroke_in_the_mask` | a marked `<rect>` with no stroke gets `stroke="none"`, and *not* `stroke="#ffffff"` |
| `a_stroked_shape_keeps_its_stroke_repainted` | `<path fill="none" stroke="#123456">` keeps `stroke="#ffffff"` and `fill="none"` |
| `stroke_is_inherited_from_an_ancestor_group` | `<g stroke="#000000"><line id="l"/></g>` → the line gets `stroke="#ffffff"` |
| `an_explicit_stroke_none_still_suppresses_the_stroke` | `stroke="none"` on a child of a stroked `<g>` wins over the inherited stroke |
| `inline_style_stroke_is_recognised_around_case_and_whitespace` | each of `stroke:#123456`, `STROKE: #123456`, `Stroke : #123456`, `stroke\t: #123456`, `stroke: #123456 !important` is seen as a stroke |
| `an_inline_style_stroke_beats_the_presentation_attribute` | `stroke="none" style="stroke:#123456"` → painted; `stroke="#123456" style="stroke:none"` → none |

- [ ] **Step 7: Add the rasterization regression test**

Append to `mod tests` in `src/rendering/svg_to_png.rs`. This is the only test
that could have caught the original defect, and the C case is the control that
proves the fix is an *evidence* fix rather than blanket stroke removal:

```rust
    /// The mask must not invent a stroke. This is the only kind of test that
    /// can catch it: the rewriter's own tests assert on the mask *document*,
    /// and an added `stroke` is only visible once the document is rasterized.
    ///
    /// Measured before the fix: case A marked 50..=150 and case B 40..=159.
    #[test]
    fn tone_mask_edge_does_not_spill_past_an_unstroked_shape() {
        let renderer = SvgRenderer::new();
        let spec = DisplaySpec::from_dimensions(200, 200).unwrap();
        let span = |svg: &str| {
            let mask = renderer.rasterize_tone_mask(svg.as_bytes(), spec).unwrap();
            let row = 100usize;
            let first = (0..200).find(|&x| mask[row * 200 + x]).unwrap();
            let last = (0..200).rev().find(|&x| mask[row * 200 + x]).unwrap();
            (first, last)
        };

        // A: a plain unstroked shape. SVG's initial stroke is `none`, so the
        // mask edge must land exactly on the geometry.
        let plain = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="50" y="50" width="100" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        assert_eq!(span(plain), (50, 149), "an unstroked shape must not widen");

        // B: `stroke` is a paint property, so the stylesheet's `stroke: none`
        // is stripped — while `stroke-width` survives as geometry. Inventing a
        // stroke here would widen the mask by half of it, unboundedly.
        let css_width = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <style>.p { stroke: none; stroke-width: 20; }</style>
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect class="p" x="50" y="50" width="100" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        assert_eq!(
            span(css_width),
            (50, 149),
            "a stripped stroke must not resurrect via stroke-width"
        );

        // C: the control. A shape that genuinely IS stroked must still mark its
        // stroke, or the fix would just be deleting strokes.
        let stroked = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="50" y="50" width="100" height="100" fill="#336699" stroke="#336699" stroke-width="20"/>
            </g>
          </svg>"##;
        assert_eq!(
            span(stroked),
            (40, 159),
            "a real stroke must still be marked"
        );
    }
```

- [ ] **Step 8: Document the third known case**

In the module docs of `tone_mask.rs`, the "Known over-marking" section says
"Two cases grow the marked region slightly." Make it three, and note that this
one *shrinks* it:

```rust
//! - A stroke set only by a stylesheet rule is lost, because paint
//!   declarations are stripped before the stroke is resolved. That element
//!   under-marks by half its stroke width. This is the one case that shrinks
//!   the region rather than growing it, and it is the fail-safe direction:
//!   the alternative, painting `stroke` unconditionally, invents a stroke on
//!   every unstroked shape and moves edges by an unbounded `stroke-width / 2`.
```

- [ ] **Step 9: Verify**

```
CARGO_BUILD_JOBS=2 cargo test --lib tone_mask
```
Expected: **27 pass** (18 existing rewriter + 6 new rewriter + 3 rasterization).

```
make check
```
Expected: fully green; byonk lib suite **429 -> 436**.

- [ ] **Step 10: Commit**

```bash
git add src/rendering/tone_mask.rs src/rendering/svg_to_png.rs
git commit -m "fix(rendering): the tone mask must not invent a stroke"
```

---

### Task 10: Wire gamut mapping into the render path

**Files:**
- Modify: `src/rendering/svg_to_png.rs`

**Interfaces:**
- Consumes: `has_tone_markup`, `rasterize_tone_mask`, `GamutMapper`, `GamutOptions`.
- Produces: `DitherTuning` gains `pub gamut: Option<GamutOptions>`.

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` in `src/rendering/svg_to_png.rs`:

```rust
    /// The opt-in guarantee: an unmarked document must render byte-identically
    /// whether or not the gamut knobs are present.
    #[test]
    fn unmarked_document_renders_byte_identically() {
        let renderer = SvgRenderer::new();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <rect x="10" y="10" width="60" height="60" fill="#c06020"/>
          </svg>"#;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let palette = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
        ];

        let plain = renderer
            .render_to_palette_png(svg.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();

        let tuning = DitherTuning {
            gamut: Some(eink_dither::GamutOptions::default()),
            ..Default::default()
        };
        let with_knobs = renderer
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                Some(&tuning),
            )
            .unwrap();

        assert_eq!(plain, with_knobs, "unmarked document must be unaffected");
    }

    /// A marked vivid region must actually change.
    #[test]
    fn marked_region_is_altered_by_mapping() {
        let renderer = SvgRenderer::new();
        let marked = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="100" height="100" fill="#ff00aa"/>
            </g>
          </svg>"#;
        let unmarked = marked.replace(r#" data-byonk-tone="continuous""#, "");
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let palette = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
        ];

        let a = renderer
            .render_to_palette_png(marked.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();
        let b = renderer
            .render_to_palette_png(unmarked.as_bytes(), spec, &palette, None, false, None, None)
            .unwrap();

        assert_ne!(a, b, "marking a vivid region must change the output");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `CARGO_BUILD_JOBS=2 cargo test unmarked_document marked_region`
Expected: FAIL to compile — `struct `DitherTuning` has no field named `gamut``

- [ ] **Step 3: Wire it in**

In `src/rendering/svg_to_png.rs`, extend the struct and its docs:

```rust
/// Optional per-render tuning overrides (dev mode, script, device config).
///
/// `gamut` belongs here rather than in a separate parameter because gamut
/// mapping runs in the same stage as dithering, against the same palette, and
/// is configured from the same places in the priority chain.
#[derive(Debug, Default)]
pub struct DitherTuning {
    pub serpentine: Option<bool>,
    pub error_clamp: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub strength: Option<f32>,
    /// Gamut mapping knobs. `None` uses [`GamutOptions::default`]; mapping
    /// still only happens where the document marks a continuous-tone region.
    pub gamut: Option<eink_dither::GamutOptions>,
}
```

Add the import at the top: change the `eink_dither` use line to

```rust
use eink_dither::{
    DitherAlgorithm, EinkDitherer, GamutMapper, GamutOptions, Palette as EinkPalette,
    Srgb as EinkSrgb,
};
```

In `render_to_palette_png`, replace the line

```rust
        let pixels = rgba_to_eink_srgb(pixmap.data());
```

with

```rust
        let mut pixels = rgba_to_eink_srgb(pixmap.data());

        // Gamut mapping, opt-in per region. An unmarked document skips the
        // second rasterization entirely and renders exactly as it did before
        // this feature existed.
        if crate::rendering::tone_mask::has_tone_markup(svg_data) {
            let gamut_opts = tuning
                .and_then(|t| t.gamut)
                .unwrap_or_else(GamutOptions::default);
            if gamut_opts.amount != 0.0 {
                let mask = self.rasterize_tone_mask(svg_data, spec)?;
                if mask.len() == pixels.len() {
                    let marked = mask.iter().filter(|m| **m).count();
                    tracing::debug!(
                        marked_pixels = marked,
                        total_pixels = pixels.len(),
                        knee = gamut_opts.knee,
                        amount = gamut_opts.amount,
                        max_compression = gamut_opts.max_compression,
                        "applying gamut mapping to continuous-tone regions"
                    );
                    GamutMapper::new(&eink_palette).map_frame(&mut pixels, &mask, gamut_opts);
                } else {
                    // Cannot happen: both rasterize to `spec`. Loud rather
                    // than silently skipped.
                    return Err(RenderError::Dither(format!(
                        "tone mask length {} does not match frame {}",
                        mask.len(),
                        pixels.len()
                    )));
                }
            }
        }
```

Note `eink_palette` is moved into `EinkDitherer::new(eink_palette)` further down; since `GamutMapper::new` only borrows, this ordering compiles as written. Keep the gamut block **before** the `EinkDitherer::new` call.

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk rendering::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/rendering/svg_to_png.rs
git commit -m "feat(rendering): apply gamut mapping to marked regions before dithering"
```

---

### Task 11: The `GamutTuningValues` type and the Lua surface

The knobs travel through **eight** structs between the script and the renderer.
This task lands the type and the two ends (Lua in, `DitherTuning` out); Task 12
threads the six structs in between. They are split because the first is
reviewable on its own — a script can return the table and it parses — while the
second is mechanical plumbing across the display path.

The full chain, mapped from the tree at `81ba62b` (grep `error_clamp` to see it):

| # | Struct | File |
|---|---|---|
| 1 | `lua_runtime::ScriptResult` | `src/services/lua_runtime.rs:32` |
| 2 | `content_pipeline::ScriptResult` (`script_*` prefix) | `src/services/content_pipeline.rs:50` |
| 3 | `DeviceContext` (`dither_*` prefix, script-visible) | `src/services/content_pipeline.rs:93` |
| 4 | `DitherTuningValues` | `src/models/config.rs:16` |
| 5 | `DeviceConfig` | `src/models/config.rs:264` |
| 6 | `CachedContent` | `src/services/content_cache.rs:30` |
| 7 | `RenderParams` | `src/api/display.rs:85` |
| 8 | `svg_to_png::DitherTuning` | `src/rendering/svg_to_png.rs` (done in Task 10) |

**Files:**
- Modify: `src/models/config.rs`, `src/models/mod.rs`, `src/services/lua_runtime.rs`

**Interfaces:**
- Produces:
  ```rust
  // src/models/config.rs
  #[derive(Debug, Deserialize, Clone, Default, PartialEq)]
  pub struct GamutTuningValues {
      pub knee: Option<f32>,
      pub amount: Option<f32>,
      pub max_compression: Option<f32>,
  }
  impl GamutTuningValues {
      pub fn or(&self, other: &GamutTuningValues) -> GamutTuningValues;
      pub fn is_empty(&self) -> bool;
      /// Resolve against `GamutOptions::default()`.
      pub fn resolve(&self) -> eink_dither::GamutOptions;
  }
  ```
  `DitherTuningValues` gains `pub gamut: GamutTuningValues` (flattened through `or`/`is_empty`). `ScriptResult` gains `pub gamut: Option<GamutTuningValues>`.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` in `src/models/config.rs`:

```rust
    #[test]
    fn gamut_values_resolve_against_the_crate_defaults() {
        let defaults = eink_dither::GamutOptions::default();
        assert_eq!(GamutTuningValues::default().resolve(), defaults);

        let partial = GamutTuningValues {
            knee: Some(0.4),
            ..Default::default()
        };
        let resolved = partial.resolve();
        assert_eq!(resolved.knee, 0.4);
        assert_eq!(resolved.amount, defaults.amount);
        assert_eq!(resolved.max_compression, defaults.max_compression);
    }

    #[test]
    fn gamut_values_merge_with_self_winning() {
        let hi = GamutTuningValues {
            knee: Some(0.4),
            ..Default::default()
        };
        let lo = GamutTuningValues {
            knee: Some(0.9),
            amount: Some(0.5),
            max_compression: None,
        };
        let merged = hi.or(&lo);
        assert_eq!(merged.knee, Some(0.4), "self must win");
        assert_eq!(merged.amount, Some(0.5), "other must fill the gap");
    }

    #[test]
    fn dither_tuning_carries_gamut_through_the_chain() {
        let script = DitherTuningValues {
            gamut: GamutTuningValues {
                amount: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let panel = DitherTuningValues {
            gamut: GamutTuningValues {
                knee: Some(0.55),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = script.or(&panel);
        assert_eq!(merged.gamut.amount, Some(0.0));
        assert_eq!(merged.gamut.knee, Some(0.55));
        assert!(!merged.is_empty());
    }
```

Append to the `mod tests` in `src/services/lua_runtime.rs` (follow the surrounding tests' pattern for constructing a runtime and running a script; if a helper like `run_test_script(&str)` already exists, use it rather than building a new one):

```rust
    #[test]
    fn script_can_return_gamut_knobs() {
        let result = run_test_script(
            r#"
            return {
                data = {},
                gamut = { knee = 0.45, amount = 0.8, max_compression = 3.0 },
            }
            "#,
        )
        .expect("script must run");
        let g = result.gamut.expect("gamut table must be parsed");
        assert_eq!(g.knee, Some(0.45));
        assert_eq!(g.amount, Some(0.8));
        assert_eq!(g.max_compression, Some(3.0));
    }

    #[test]
    fn a_partial_gamut_table_leaves_the_rest_unset() {
        let result = run_test_script(r#"return { data = {}, gamut = { amount = 0 } }"#)
            .expect("script must run");
        let g = result.gamut.expect("gamut table must be parsed");
        assert_eq!(g.amount, Some(0.0));
        assert_eq!(g.knee, None);
    }

    #[test]
    fn no_gamut_table_means_none() {
        let result = run_test_script(r#"return { data = {} }"#).expect("script must run");
        assert!(result.gamut.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `CARGO_BUILD_JOBS=2 cargo test gamut_values script_can_return_gamut a_partial_gamut no_gamut_table`
Expected: FAIL to compile — `cannot find type `GamutTuningValues``

- [ ] **Step 3: Implement the plumbing**

In `src/models/config.rs`, add above `DitherTuningValues`:

```rust
/// Gamut mapping knobs, at every level of the tuning priority chain.
///
/// Frame-level, not per adaptation group: groups change only which pixels are
/// measured together to derive the compression factor, not the curve's shape.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct GamutTuningValues {
    /// Where compression begins, as a fraction of the reachable chroma limit.
    pub knee: Option<f32>,
    /// 0 = no mapping, 1 = full. Only 1 guarantees in-gamut output.
    pub amount: Option<f32>,
    /// Cap on the compression factor.
    pub max_compression: Option<f32>,
}

impl GamutTuningValues {
    /// Merge: self takes priority, other fills gaps.
    pub fn or(&self, other: &GamutTuningValues) -> GamutTuningValues {
        GamutTuningValues {
            knee: self.knee.or(other.knee),
            amount: self.amount.or(other.amount),
            max_compression: self.max_compression.or(other.max_compression),
        }
    }

    /// Returns true if all fields are None.
    pub fn is_empty(&self) -> bool {
        self.knee.is_none() && self.amount.is_none() && self.max_compression.is_none()
    }

    /// Fill the gaps from the crate defaults.
    pub fn resolve(&self) -> eink_dither::GamutOptions {
        let d = eink_dither::GamutOptions::default();
        eink_dither::GamutOptions {
            knee: self.knee.unwrap_or(d.knee),
            amount: self.amount.unwrap_or(d.amount),
            max_compression: self.max_compression.unwrap_or(d.max_compression),
        }
    }
}
```

Extend `DitherTuningValues` with `#[serde(default)] pub gamut: GamutTuningValues,`, and update `or` and `is_empty`:

```rust
            gamut: self.gamut.or(&other.gamut),
```
```rust
            && self.gamut.is_empty()
```

In the `PanelDitherVisitor::visit_map` match, add before the catch-all arm:

```rust
                        "gamut" => {
                            defaults.gamut = map.next_value()?;
                        }
```

In `src/services/lua_runtime.rs`, add `pub gamut: Option<crate::models::GamutTuningValues>,` to `ScriptResult` with the doc comment:

```rust
    /// Optional gamut mapping knobs from the script return. Only takes effect
    /// where the SVG marks a region `data-byonk-tone="continuous"`.
```

and parse it after the existing tuning parameters (around line 402):

```rust
        // Parse the optional gamut sub-table from the script return.
        let gamut = result.get::<Table>("gamut").ok().map(|t| {
            crate::models::GamutTuningValues {
                knee: t.get::<f32>("knee").ok(),
                amount: t.get::<f32>("amount").ok(),
                max_compression: t.get::<f32>("max_compression").ok(),
            }
        });
```

Add `gamut,` to the `ScriptResult { .. }` literal.

In `src/models/mod.rs`, add `GamutTuningValues` to the `pub use config::{...}` list.

**Keep the tree compiling.** Adding a field to `DitherTuningValues` breaks every
struct literal that does not use `..Default::default()`. Run the build and add
`gamut: Default::default(),` to each site the compiler names — as of `81ba62b`
those are `src/api/display.rs` (~lines 814, 927) and `src/main.rs` (~lines 385,
404). These are placeholders that carry no value yet; **Task 12 replaces each
one with the real source**. Do not wire any actual value here — that is Task
12's reviewable deliverable, and doing it now would leave Task 12 with nothing
to review.

Every task in this plan ends on a green `cargo test`; a commit that does not
build is never acceptable, even mid-plan.

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk gamut_values script_can_return_gamut a_partial_gamut no_gamut_table`
Expected: PASS, 6 tests, and the whole crate builds.

- [ ] **Step 5: Commit**

```bash
git add src/models/config.rs src/models/mod.rs src/services/lua_runtime.rs \
        src/api/display.rs src/main.rs
git commit -m "feat(config): GamutTuningValues type and the script-side gamut table"
```

---

### Task 12: Thread the gamut knobs through the display path

Mechanical, but it must be complete: a struct that silently drops `gamut` gives
a screen whose knobs are ignored with no error. Work the table in Task 11 from
the script end outwards and let the compiler enumerate the sites.

**Files:**
- Modify: `src/services/content_pipeline.rs`, `src/services/content_cache.rs`, `src/services/screen_store.rs`, `src/api/display.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `GamutTuningValues` (Task 11), `DitherTuning::gamut` (Task 10).
- Produces: `RenderParams` gains `pub gamut: GamutTuningValues`; `resolve_dither_tuning` populates `DitherTuning::gamut`.

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` in `src/api/display.rs`:

```rust
    #[test]
    fn gamut_follows_the_script_over_device_over_panel_priority() {
        let script = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.4),
                ..Default::default()
            },
            ..Default::default()
        };
        let device = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.7),
                amount: Some(0.5),
                ..Default::default()
            },
            ..Default::default()
        };
        let panel = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.9),
                amount: Some(1.0),
                max_compression: Some(4.0),
            },
            ..Default::default()
        };

        let resolved = resolve_tuning(&script, &device, &panel);
        assert_eq!(resolved.gamut.knee, Some(0.4), "script must win");
        assert_eq!(resolved.gamut.amount, Some(0.5), "device fills the gap");
        assert_eq!(
            resolved.gamut.max_compression,
            Some(4.0),
            "panel fills what neither set"
        );
    }

    #[test]
    fn a_gamut_only_override_counts_as_an_override() {
        // `resolve_effective_tuning` short-circuits when any override field is
        // set. A gamut-only override must not be silently ignored.
        let over = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                amount: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let other = DitherTuningValues {
            error_clamp: Some(0.5),
            ..Default::default()
        };
        let resolved = resolve_effective_tuning(&over, &other, &other, &other);
        assert_eq!(resolved.gamut.amount, Some(0.0));
        assert_eq!(
            resolved.error_clamp, None,
            "an explicit override replaces the whole struct"
        );
    }

    #[test]
    fn render_params_carry_gamut_into_the_dither_tuning() {
        let params = RenderParams {
            palette: vec![(0, 0, 0), (255, 255, 255)],
            measured_colors: None,
            measured_source: SRC_NONE,
            dither: None,
            error_clamp: None,
            noise_scale: None,
            chroma_clamp: None,
            strength: None,
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.45),
                ..Default::default()
            },
        };
        let (tuning, has_tuning) = resolve_dither_tuning(&params);
        assert!(has_tuning, "a gamut knob is a tuning override");
        assert_eq!(tuning.gamut.expect("gamut must be set").knee, 0.45);
    }
```

`SRC_NONE` is one of the `SRC_*` consts already in that file — use whichever
name the file defines for "no measured colours"; the field is
`measured_source: &'static str`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk api::display`
Expected: FAIL to compile — `struct `RenderParams` has no field named `gamut``

- [ ] **Step 3: Thread the field through each struct**

**`src/services/content_pipeline.rs:50`** — add to `ScriptResult`, next to `script_error_clamp`:

```rust
    /// Gamut mapping knobs from the script return, if it set any.
    pub script_gamut: Option<crate::models::GamutTuningValues>,
```

and at the construction site (~line 306, alongside `script_error_clamp: lua_result.error_clamp`):

```rust
            script_gamut: lua_result.gamut,
```

**`src/services/content_pipeline.rs:93`** — `DeviceContext` exposes the resolved
tuning back to the script. Add the three resolved values so a script can read
what it will actually get, matching the existing `dither_*` naming:

```rust
    pub dither_gamut_knee: Option<f32>,
    pub dither_gamut_amount: Option<f32>,
    pub dither_gamut_max_compression: Option<f32>,
```

Populate them from `pre_script_tuning.gamut` at `src/api/display.rs` (~line 856,
where `dither_error_clamp: pre_script_tuning.error_clamp` sits):

```rust
        dither_gamut_knee: pre_script_tuning.gamut.knee,
        dither_gamut_amount: pre_script_tuning.gamut.amount,
        dither_gamut_max_compression: pre_script_tuning.gamut.max_compression,
```

and expose them in the Lua `device.dither` table at
`src/services/lua_runtime.rs` (~line 601, where `device_table.set("dither", …)`
builds the sub-table) as a nested `gamut` table, mirroring the script's own
return shape:

```rust
            let gamut_table = lua.create_table()?;
            if let Some(v) = ctx.dither_gamut_knee {
                gamut_table.set("knee", v)?;
            }
            if let Some(v) = ctx.dither_gamut_amount {
                gamut_table.set("amount", v)?;
            }
            if let Some(v) = ctx.dither_gamut_max_compression {
                gamut_table.set("max_compression", v)?;
            }
            dither_table.set("gamut", gamut_table)?;
```

**`src/models/config.rs:264`** — add to `DeviceConfig`, next to `error_clamp`:

```rust
    /// Optional gamut mapping overrides for continuous-tone regions
    #[serde(default)]
    pub gamut: GamutTuningValues,
```

**`src/services/content_cache.rs:30`** — add to `CachedContent`, next to
`error_clamp`:

```rust
    pub gamut: crate::models::GamutTuningValues,
```

and add `gamut: …` at each of its construction sites (the compiler lists them).

**`src/services/screen_store.rs:123`** — add `pub gamut: crate::models::GamutTuningValues,`
to the struct that carries `error_clamp` there, and populate it from the
resolved tuning at its construction site.

**`src/api/display.rs`**, five edits:

1. `RenderParams` (~line 85) — add `pub gamut: crate::models::GamutTuningValues,`
2. The `RenderParams { … }` literal (~line 373) — add `gamut: tuning.gamut.clone(),`
3. `resolve_effective_tuning` (~line 165) — add to the override test:
   ```rust
        || !override_tuning.gamut.is_empty()
   ```
4. `resolve_dither_tuning` (~line 183) — add to the struct literal and the flag:
   ```rust
        gamut: Some(render_params.gamut.resolve()),
   ```
   ```rust
        || !render_params.gamut.is_empty()
   ```
5. `dc_tuning` (~line 814) — add:
   ```rust
        gamut: device_config.map(|dc| dc.gamut.clone()).unwrap_or_default(),
   ```
   and `script_tuning` (~line 927) — add:
   ```rust
        gamut: result.script_gamut.clone().unwrap_or_default(),
   ```
   and the cached-tuning `DitherTuning` (~line 1142) — add:
   ```rust
        gamut: Some(cached.gamut.resolve()),
   ```
   with the `has_tuning` flag extended by `|| !cached.gamut.is_empty()`.

**`src/main.rs`** — `cli_tuning` (~line 447) gets `gamut: None,`; the two
`DitherTuningValues` literals (~lines 385, 404) get `gamut: Default::default(),`.

- [ ] **Step 4: Run the full check**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk`
Expected: PASS. Then confirm nothing was missed:

```bash
grep -rn "error_clamp" --include=*.rs src/ | grep -v "gamut" | wc -l
grep -rn "gamut" --include=*.rs src/ | wc -l
```

Every struct that carries `error_clamp` must now also carry `gamut`. Walk the
first list and confirm each site has a gamut counterpart; a struct the compiler
did not force (because `GamutTuningValues` implements `Default` and the literal
used `..Default::default()`) is exactly the silent-drop case this step exists to
catch.

- [ ] **Step 5: Commit**

```bash
git add src/api/display.rs src/services/content_pipeline.rs src/services/content_cache.rs \
        src/services/screen_store.rs src/services/lua_runtime.rs src/models/config.rs src/main.rs
git commit -m "feat(display): thread gamut knobs through the script, device and panel chain"
```

---

### Task 13: Regression metrics and the visual golden

Mean dE is the wrong yardstick here and is **expected to worsen**. Replace it with metrics that measure what the design is actually for: preserved differences.

**Files:**
- Modify: `crates/eink-dither/src/domain_tests.rs`
- Modify: `crates/eink-dither/tests/visual_compare.rs`
- Modify (conditionally, see Step 4): `screens/builtin/calibration/gamut/screen.svg`

**Interfaces:**
- Consumes: `GamutMapper`, `GamutOptions`, `EinkDitherer`, `Palette`, existing `visual_compare` helpers.

- [ ] **Step 1: Write the failing tests**

Append to `crates/eink-dither/src/domain_tests.rs`, in the module that holds the other sweeps:

```rust
    /// Hue ordering around the circle must be preserved.
    ///
    /// Today 285° lands at h8°, out of order with both its neighbours. The
    /// gamut mapper carries hue through untouched, so the ordering of the
    /// *mapped* targets must be monotonic even where the dithered result is
    /// not. This measures the mapper, not the ditherer.
    #[test]
    #[ignore = "diagnostic sweep"]
    fn test_gamut_mapping_preserves_hue_order() {
        use crate::{GamutMapper, GamutOptions, Oklch};

        let palette = six_color_palette();
        let mapper = GamutMapper::new(&palette);
        let opts = GamutOptions::default();

        let mut inversions = 0;
        let mut prev: Option<f32> = None;
        for deg in (0..360).step_by(15) {
            let h = (deg as f32).to_radians() - std::f32::consts::PI;
            let src = Srgb::from(LinearRgb::from(Oklab::from(Oklch {
                l: 0.55,
                c: 0.20,
                h,
            })));
            let mapped = mapper.map_color(src, 2.0, opts);
            let h_out = Oklch::from(Oklab::from(LinearRgb::from(mapped))).h;
            if let Some(p) = prev {
                // Both sequences advance around the circle; a decrease that is
                // not the single wrap point is an inversion.
                let step = h_out - p;
                if step < 0.0 && step > -std::f32::consts::PI {
                    inversions += 1;
                    eprintln!("hue inversion at {deg}°: {p:.3} -> {h_out:.3}");
                }
            }
            prev = Some(h_out);
        }
        assert_eq!(inversions, 0, "gamut mapping must not reorder hues");
    }

    /// Local contrast across a gradient ramp must survive mapping.
    ///
    /// The point of the knee's strict monotonicity: adjacent steps of a ramp
    /// stay distinct. A clipping approach would collapse the top of the ramp
    /// to a single value.
    #[test]
    #[ignore = "diagnostic sweep"]
    fn test_gamut_mapping_preserves_local_contrast() {
        use crate::{GamutMapper, GamutOptions, Oklch};

        let palette = six_color_palette();
        let mapper = GamutMapper::new(&palette);
        let opts = GamutOptions::default();

        // A saturation ramp at fixed hue and lightness.
        let steps: Vec<f32> = (0..64).map(|i| i as f32 / 63.0 * 0.32).collect();
        let mut collapsed = 0;
        let mut prev_c = f32::NEG_INFINITY;
        for &c in &steps {
            let out = mapper.mapped_chroma(c, 0.6, 0.55, 2.0, opts);
            if out <= prev_c {
                collapsed += 1;
                eprintln!("ramp collapsed at c={c:.4}: {prev_c:.6} -> {out:.6}");
            }
            prev_c = out;
        }
        assert_eq!(collapsed, 0, "every ramp step must stay distinct");
    }
```

Append to `crates/eink-dither/tests/visual_compare.rs`. The helpers it uses all
already exist in that file: `panel() -> Palette`, `hsl_to_rgb(h, s, l)`,
`triptych(orig, before, after, w, h) -> (Vec<u8>, usize, usize)`,
`write(name, buf, w, h)`, and `DitheredImage::to_rgb_actual()`.

```rust
/// Visual golden: the hue x lightness field with and without gamut mapping.
///
/// Mean dE is *expected to worsen*; what to look for is banding turning into
/// gradation, and hue bands that used to collapse onto one ink separating.
/// Render the field and look — flat-patch dE cannot tell you this.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_gamut_mapping_before_after() {
    use eink_dither::{GamutMapper, GamutOptions};

    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl_to_rgb(x as f32 / W as f32, 1.0, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let before = EinkDitherer::new(palette.clone())
        .dither(&pixels, W, H)
        .to_rgb_actual();

    let mut mapped = pixels.clone();
    let mask = vec![true; mapped.len()];
    GamutMapper::new(&palette).map_frame(&mut mapped, &mask, GamutOptions::default());
    let after = EinkDitherer::new(palette.clone())
        .dither(&mapped, W, H)
        .to_rgb_actual();

    let (buf, ow, oh) = triptych(&pixels, &before, &after, W, H);
    write("gamut-mapping-field.png", &buf, ow, oh);
    eprintln!("original | unmapped | mapped — inspect by eye");
}

/// The same comparison at three knee values, to pick one by eye.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_gamut_knee_sweep() {
    use eink_dither::{GamutMapper, GamutOptions};

    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();
    let mapper = GamutMapper::new(&palette);

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl_to_rgb(x as f32 / W as f32, 1.0, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let baseline = EinkDitherer::new(palette.clone())
        .dither(&pixels, W, H)
        .to_rgb_actual();

    for knee in [0.4f32, 0.6, 0.8] {
        let mut mapped = pixels.clone();
        let mask = vec![true; mapped.len()];
        mapper.map_frame(
            &mut mapped,
            &mask,
            GamutOptions {
                knee,
                ..GamutOptions::default()
            },
        );
        let out = EinkDitherer::new(palette.clone())
            .dither(&mapped, W, H)
            .to_rgb_actual();
        let (buf, ow, oh) = triptych(&pixels, &baseline, &out, W, H);
        write(&format!("gamut-knee-{knee:.1}.png"), &buf, ow, oh);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither gamut -- --ignored --nocapture`
Expected: FAIL to compile — the metric tests reference `six_color_palette()`; if
`domain_tests.rs` names its six-ink helper differently, reuse that name rather
than adding a duplicate.

- [ ] **Step 3: Run the metric tests**

Run: `CARGO_BUILD_JOBS=2 cargo test -p eink-dither test_gamut_mapping -- --ignored --nocapture`
Expected: PASS, 2 tests. A failure here means the mapper reorders hues or
collapses a ramp — fix `mapper.rs` or `knee.rs`, never the assertion.

- [ ] **Step 4: Render the imagery and the real calibration screen, then look**

Run:
```
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture
```

Then render the actual calibration screen end to end, which is what the spec
asks for as the golden and is the only path that exercises the SVG marker, the
rewriter and the mask together. Add the marker to
`screens/builtin/calibration/gamut/screen.svg` by wrapping the patch grid (not
the labels) in `<g data-byonk-tone="continuous"> … </g>`, then:

```
CARGO_BUILD_JOBS=2 cargo run -- render --screen byonk-builtin/calibration/gamut --out /tmp/gamut-mapped.png
```

Check `cargo run -- render --help` for the flag names this build actually uses;
if there is no such subcommand, start the server (`cargo run`) and fetch the
screen through `/dev/render` instead. Render it once with the marker and once
without (`git stash` the screen edit) and compare the two PNGs.

**Then open the images and look.** This is not optional and not a formality: on
this codebase every arm that improved patch dE made the rendered image worse,
and one arm scored the best numbers ever measured here while looking abysmal.
Report to the reviewer what the rendered field actually looks like — banding,
flatness, hue separation — not only that the tests passed. If the mapped output
looks worse, say so and stop; that is a result, not a failure to be tuned away.

Keep the marker in `screens/builtin/calibration/gamut/screen.svg` only if the
mapped render is better; that decision is the owner's, so present both images
rather than deciding alone.

**Then open the images and look.** This is not optional and not a formality: on this codebase every arm that improved patch dE made the rendered image worse, and one arm scored the best numbers ever measured here while looking abysmal. Report to the reviewer what the rendered field actually looks like — banding, flatness, hue separation — not only that the tests passed.

- [ ] **Step 5: Commit**

```bash
git add crates/eink-dither/src/domain_tests.rs crates/eink-dither/tests/visual_compare.rs
git commit -m "test(eink-dither): hue-order and local-contrast metrics plus a visual golden"
```

Commit the calibration-screen marker **separately and only if the owner keeps
it**, so the metrics land either way:

```bash
git add screens/builtin/calibration/gamut/screen.svg
git commit -m "feat(screens): mark the gamut calibration patches as continuous-tone"
```

---

### Task 14: Documentation and changelog

**Files:**
- Create: `docs/src/gamut-mapping.md`
- Modify: `docs/src/SUMMARY.md`, `CHANGES.md`

- [ ] **Step 1: Write the authoring documentation**

Read `docs/src/SUMMARY.md` and one existing page to match voice and structure, then create `docs/src/gamut-mapping.md`:

````markdown
# Continuous-tone regions (gamut mapping)

A six-colour panel can only display colours inside the convex hull of its six
inks. Most of the hue circle falls outside it, so nearest-colour matching —
which is what dithering does by default — renders wide bands of the spectrum as
one flat ink. That keeps each patch as vivid as possible while destroying the
*differences* between neighbouring colours: gradients flatten into bands,
distinct hues collapse onto one ink, and hue ordering can invert.

Marking a region as continuous-tone trades a little accuracy for those
differences. It does **not** enlarge the gamut. Marked regions look **less
saturated** than the unmarked output — today's output is falsely saturated.
Nothing makes a six-ink panel render a vivid rainbow.

## Marking a region

Add `data-byonk-tone="continuous"` to any element or group:

```svg
<g data-byonk-tone="continuous">
  <image href="photo.png" x="0" y="0" width="800" height="480"/>
  <rect fill="url(#skyGradient)" x="0" y="0" width="800" height="120"/>
</g>
```

The value is inherited by descendants, and a descendant may override it back to
`graphic` — which is the default everywhere, so an unmarked document renders
exactly as it always has:

```svg
<g data-byonk-tone="continuous">
  <rect fill="url(#sky)" .../>
  <text data-byonk-tone="graphic">18:42</text>   <!-- stays crisp -->
</g>
```

The override is load-bearing: a chart or photo with a caption over it needs the
background mapped and the label left sharp.

The marker is not photo-specific. It applies to any continuous-tone content:
photographs, gradients, charts, illustrations. Because the SVG is templated
from your script's data, a screen can decide at render time which regions are
continuous-tone.

## Adaptation groups

`data-byonk-tone-group="<id>"` is inherited the same way. Marked regions
sharing an id adapt together; the default group is the whole frame.

Frame-wide is the default because identical colours should map identically
wherever they appear — a photo and a gradient sharing a blue stay consistent.
Use a group when one very vivid region would otherwise drag the compression for
a mild one.

> **Not yet active.** `data-byonk-tone-group` is accepted and validated, but
> every marked region currently adapts together as one frame-wide group. The
> attribute is inert until per-group adaptation ships; nothing breaks if you set
> it, and nothing changes either.

## Tuning

On the script return, alongside `dither` and the other tuning keys:

```lua
return {
  data = data,
  gamut = {
    knee            = 0.8,   -- where compression begins, as a fraction of the limit
    amount          = 1.0,   -- 0 = no mapping, 1 = full
    max_compression = 2.5,   -- never squeeze chroma by more than this
  },
}
```

- **`knee`** — content below this fraction of the reachable limit passes
  through untouched. The default, 0.8, sits in the same band as the ACES
  gamut-compression thresholds. Lowering it protects separation between the
  most out-of-gamut colours, at the cost of desaturating everything vivid:
  at 0.6 the most saturated content in a region renders at 82% of what the
  panel can show, against 91% at 0.8. Raising it toward 1.0 does the reverse
  and eventually compresses the whole out-of-gamut range into a sliver.
- **`amount`** — interpolates between the input and the mapped chroma, so
  `amount = 0` is a clean A/B switch for judging the effect on a real panel.
  Note that only `amount = 1` guarantees in-gamut output; lower values can leave
  chroma above the limit, which the ditherer then clips as it does today. It is
  a comparison and taste control, not a correctness one.
- **`max_compression`** — raising it lets an extremely vivid image adapt
  further, at the cost of flattening everything else; lowering it protects the
  bulk of the image and pushes the extremes into the curve's tail, where they
  stay distinguishable but heavily compressed.

The same keys can be set per-panel and per-device under `dither: gamut:` in
`config.yaml`, with the usual priority chain (script wins over device, device
over panel).

## Greyscale panels

On a four-level grey panel no chroma is reachable at all, so a marked region
desaturates to grey rather than having its colours flung at the nearest ink.
That is the correct result, not a bug.

## Limitations

- An `<image>` is masked over its layout box, so a transparent or letterboxed
  image marks the whole box.
- An element painted `none` only from a CSS rule (rather than a `fill="none"`
  attribute) is treated as painted when building the mask.

Both only ever grow the marked region slightly, and mapping already-in-gamut
content does nothing, so the practical effect is negligible.
````

Add the page to `docs/src/SUMMARY.md` in the section where the other rendering
and dithering pages live.

- [ ] **Step 2: Build the docs**

Run: `make docs`
Expected: builds without warnings about the new page (needs `mdbook-mermaid`)

- [ ] **Step 3: Add the changelog entry**

Add to the `## [Unreleased]` section of `CHANGES.md`, under `### Added`
(user-facing only — no CI, tooling or internal refactor notes):

```markdown
- **Continuous-tone regions**: mark any element or group with
  `data-byonk-tone="continuous"` and its pixels are compressed into the panel's
  physically reachable colour range before dithering, so gradients gradate
  instead of banding and distinct hues stay distinct instead of collapsing onto
  one ink. Opt-in per region and inherited by descendants; a descendant can
  override back to `data-byonk-tone="graphic"` to keep a caption crisp over a
  mapped photo. Unmarked documents are unaffected. Tunable from a script's
  `gamut = { knee, amount, max_compression }` table or per-panel and per-device
  under `dither: gamut:`. Marked regions render **less saturated** than before —
  the previous output was falsely saturated — in exchange for preserving the
  differences between colours. `data-byonk-tone-group` is accepted but currently
  inert — all marked regions adapt together as one frame-wide group. See the
  "Continuous-tone regions" documentation page.
```

- [ ] **Step 4: Run the full check**

Run: `make check` with `timeout: 600000` and `CARGO_BUILD_JOBS=2`
Expected: fmt clean, clippy clean with `-D warnings`, all workspace tests pass

- [ ] **Step 5: Commit**

```bash
git add docs/src/gamut-mapping.md docs/src/SUMMARY.md CHANGES.md
git commit -m "docs: continuous-tone regions and gamut mapping"
```

---

## Deferred to a later iteration

Explicitly **not** in this plan, per the spec's "Not doing":

- Spatial / multiscale local-contrast restoration.
- Cusp lightness migration (real GCUSP migrates L toward the gamut cusp where
  more chroma is available; a second free parameter, and one knob tuned against
  the calibration screen is worth more than two tuned against each other).
- Per-hue `R` — it would stop a vivid red compressing the blues, but it changes
  relative chroma *between* hues, which is precisely what this design exists to
  preserve. Revisit only if real screens show the scalar being hijacked, and
  then measure against the hue-order metric rather than by eye.
- HPMINDE-style clipping as a shipped mode.
- Extending `image_process`'s `palette_aware` beyond its luminance endpoints.

**Adaptation groups are authored but not yet differentiated.** Task 8 recognises
`data-byonk-tone-group` and strips it from the mask document, but
`map_frame` derives a single `R` for all marked pixels — the frame-wide default,
which the spec names as the intended default anyway. Per-group adaptation needs
the mask to carry a group id per pixel rather than a boolean; that is a
mechanical extension of `map_frame` (take `&[u8]` group ids, derive one `R` per
distinct id) and should be a follow-up task once real screens show a case where
one vivid region drags a mild one. **Until then, `data-byonk-tone-group` has no
effect and must be documented as accepted-but-inert** — add that sentence to
`docs/src/gamut-mapping.md` in Task 14 Step 1 under "Adaptation groups", and say
the same in the `CHANGES.md` entry. Shipping an attribute that is silently
ignored is exactly the kind of thing that costs someone an afternoon.

Also still open and **independent of this work** (from the spec's Prerequisites,
last paragraph): the ditherer under-mixes the achromatic entries with the
chromatic ones — dark warm colours at 30°–60° have a computed bound of 0.000 yet
production is off by 0.05–0.09. That is a dithering bug with no gamut excuse.
Gamut mapping is the identity on those targets, so this plan neither fixes nor
worsens it.
