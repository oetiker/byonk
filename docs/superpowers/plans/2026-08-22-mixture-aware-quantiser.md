# Mixture-Aware Quantiser Implementation Plan

> # ⛔ WITHDRAWN — DO NOT IMPLEMENT THIS PLAN
>
> This was built in full, measured, and **removed again** in `36356ca`. Kept only
> as a record of what was tried. Nothing below describes code that exists.
>
> **The defect it targets was never a quantiser defect.** "A flat mid grey
> renders as 87% green ink" was the ditherer correctly following a wrong
> `colors_actual` green. Told green is nearly neutral (`#1E5645`, chroma 0.066)
> it picks green for every dark neutral; told green is a real colour
> (`#00994D`, chroma 0.158) it reserves green for green things. Restoring the
> checked-in value fixed the panel on one line.
>
> Re-tested honestly under Floyd–Steinberg, the wedge fan was harmless at
> `mixture_bias <= 0.25` and useless — the green bias it targets is already
> `+0.0012` with the feature off — and at 0.5 it posterised. ~950 lines with no
> consumer.
>
> See `2026-08-22-mixture-aware-quantiser-design.md` for the design, and the
> handover for the verdict.

> **For agentic workers:** the plan below is DEAD. Do not execute it. The
> original instruction to implement it task-by-task is retained only so the
> record is unedited.

**Goal:** Stop `eink-dither` rendering muted colours and neutral greys as a field of one chromatic ink, by discounting each palette entry's distance in proportion to its share of the pixel's exact mixture.

**Architecture:** Partition the palette's gamut into tetrahedra that all share the black–white edge (Ostromoukhov 1993). Solve a pixel's barycentric coordinates in the containing wedge — those are the exact ink recipe — and subtract `lambda * weight` from each entry's distance before choosing. A neutral's coordinates are `(w, 0, 0, k)` in every wedge, so no chromatic ink can ever receive a discount. `lambda = 0` reproduces today's nearest-neighbour selection bit for bit.

**Tech Stack:** Rust, workspace crate `crates/eink-dither`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-08-22-mixture-aware-quantiser-design.md` (commit `81ba166`)

## Global Constraints

- **Verify with the four commands directly. `make check` has reported exit 0 while tests failed.**
  ```
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd docs && mdbook build
  ```
- **`cargo test --workspace` is red on purpose until Task 6.** `test_neutral_grey_has_no_dominant_chromatic_ink` documents the defect. The expected state in `eink-dither` is 227 passed / 1 failed before this plan, and 227 + new tests passed / 2 failed after Task 1. Any *other* failure is a real regression.
- **Never `git add -A` or `git add .` in this repo.** Add by explicit path and check `git diff --cached --name-only` before every commit. Four files in the working tree belong to the owner and must never be staged: `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`.
- **`docs/src/concepts/content-pipeline.md` is off limits.** It is one of the owner's uncommitted files and it still documents the pre-0.18.0 `error_clamp`. Any doc change needed there goes in the handover as a note for the owner, never as an edit.
- **Nothing in this plan reaches `config.yaml`, byonk's `DitherTuningValues`, or the dev UI's `DITHER_DEFAULTS` table.** Owner ruling 2026-08-22: the lever is internal with one measured default.
- **Linear RGB for all mixture geometry, OKLab only for perceptual distance.** Dot mixing is physical light addition; OKLab is not additive. `gamut/hull.rs:3-7` already states this.
- **Comments and identifiers in English.**
- Commit message trailer on every commit:
  ```
  Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
  ```

## File Structure

| File | Responsibility |
|---|---|
| `crates/eink-dither/src/gamut/wedges.rs` | **New.** `WedgeFan`: build the tetrahedron fan from a palette, return mixture weights for a colour. Pure geometry, no dithering knowledge. |
| `crates/eink-dither/src/gamut/mod.rs` | Register `pub mod wedges;`. |
| `crates/eink-dither/src/palette/palette.rs` | Add `find_nearest_biased`. Widen `CHROMA_DETECTION_THRESHOLD` to `pub(crate)`. |
| `crates/eink-dither/src/dither/options.rs` | Add the `mixture_bias` field, its `Default`, and a builder setter. |
| `crates/eink-dither/src/dither/mod.rs` | Build the fan at the top of `dither_with_kernel_noise`; swap the selection call. |
| `crates/eink-dither/src/api/builder.rs` | Add `EinkDitherer::mixture_bias`, mirroring the other setters. |
| `crates/eink-dither/src/domain_tests.rs` | The in-gamut census gate, and the `#[ignore]`d `lambda` sweep diagnostic. |
| `crates/eink-dither/tests/probe_skin_throwaway.rs` | **Deleted in Task 1.** Throwaway probe that produced the spec's evidence table. |

---

### Task 1: The in-gamut census gate

Write the gate before the fix, and delete the throwaway probe whose evidence it replaces.

**Why two gates and not one.** Owner ruling 2026-08-22: *"any color inside the gamut should be accurate."* Accuracy alone cannot see the reported defect — grey 128 dithers to 77.6% green at dE 0.063, and `skin warm` to 52.8% green at dE 0.037. The average is right and the area looks green, because the eye reads the majority ink as the colour of the region. So the census asserts two independent properties over the same colour set.

**The field-colour rule is derived from the palette, not chosen.** A target whose OKLab chroma is below the *dullest* chromatic ink's chroma cannot legitimately be mostly that ink. On `panel_measured()` the dullest ink is green at chroma 0.068; greys are 0.000 and skin tones 0.03–0.06, so they all fall under the rule, while a saturated in-gamut colour is exempt.

**Files:**
- Modify: `crates/eink-dither/src/domain_tests.rs` (add a helper and one test inside `mod domain_tests`)
- Delete: `crates/eink-dither/tests/probe_skin_throwaway.rs`

**Interfaces:**
- Consumes: `crate::gamut::test_support::panel_measured()`, `crate::gamut::hull::Hull`, the public `EinkDitherer` builder.
- Produces: `fn census_patch(input: Srgb, palette: &Palette, size: usize, skip_rows: usize) -> (f32, Vec<u32>)`, used only by this task's test. Task 5's diagnostic cannot reuse it — it must vary `mixture_bias` per call, and this helper has no parameter for it. Duplicating the few lines there is better than threading a parameter through a helper whose one caller does not want it.

- [ ] **Step 1: Add the measurement helper**

Place it next to `dither_perceptual_accuracy` in `domain_tests.rs`. It differs from that helper in two ways that matter at census scale: a caller-chosen patch size, and discarding the rows where error diffusion is still settling.

```rust
    /// Dither a uniform patch and report `(dE, ink histogram)`.
    ///
    /// `skip_rows` discards the top of the patch, where error diffusion has
    /// not yet settled. At 255x255 that warm-up is ~3% of the image and is
    /// ignored elsewhere in this file; at census patch sizes it is ~12% and
    /// would show up as noise on every reading.
    fn census_patch(
        input: Srgb,
        palette: &Palette,
        size: usize,
        skip_rows: usize,
    ) -> (f32, Vec<u32>) {
        let image = vec![input; size * size];
        let out = EinkDitherer::new(palette.clone())
            .saturation(1.0)
            .contrast(1.0)
            .dither(&image, size, size);
        let indices = &out.indices()[skip_rows * size..];
        let n = indices.len() as f32;
        let mut counts = vec![0u32; palette.len()];
        let (mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32);
        for &i in indices {
            counts[i as usize] += 1;
            let c = palette.actual_linear(i as usize);
            r += c.r;
            g += c.g;
            b += c.b;
        }
        let avg = Oklab::from(LinearRgb::new(r / n, g / n, b / n));
        let target = Oklab::from(LinearRgb::from(input));
        let de = ((avg.l - target.l).powi(2)
            + (avg.a - target.a).powi(2)
            + (avg.b - target.b).powi(2))
        .sqrt();
        (de, counts)
    }
```

- [ ] **Step 2: Write the census test with the dE bound printed, not asserted**

Write it exactly as below **except** set `MAX_DE` to `f32::INFINITY` for this step, and add `println!("worst dE {worst_de:.4}");` before the asserts. The bound gets its value from measurement in Step 4 — do not guess it.

```rust
    /// Every colour the panel can physically reproduce must come back
    /// accurate, and a colour duller than the palette's dullest ink must not
    /// be rendered as a field of that ink.
    ///
    /// Two independent properties, because dE alone cannot see the defect
    /// this test exists for: grey 128 dithers to 77.6% green at dE 0.063.
    /// The average is right and the area looks green, because the eye reads
    /// the majority ink as the colour of the region rather than averaging.
    ///
    /// The field-colour rule is derived from the palette: a target with less
    /// chroma than the dullest chromatic ink cannot legitimately be mostly
    /// that ink. On `panel_measured()` that ink is green at chroma 0.068,
    /// which covers every grey and every skin tone while exempting saturated
    /// colours that genuinely are one ink.
    #[test]
    fn test_in_gamut_census() {
        /// sRGB grid step. 16 yields 677 in-gamut colours; 32 collapses to 78.
        const STEP: usize = 16;
        /// Patch edge. 677 patches of this size is ~2.3s in a debug build.
        const PATCH: usize = 64;
        /// Rows discarded while error diffusion settles.
        const SKIP: usize = 8;
        /// Set from measurement in Task 1 Step 4.
        const MAX_DE: f32 = f32::INFINITY;
        /// An ink covering the majority of a patch is its field colour.
        const MAX_SINGLE_INK_PCT: f32 = 50.0;

        let palette = crate::gamut::test_support::panel_measured();
        let hull = crate::gamut::hull::Hull::from_palette(&palette);

        let dullest_ink_chroma = (2..palette.len())
            .map(|i| {
                let c = palette.actual_oklab(i);
                (c.a * c.a + c.b * c.b).sqrt()
            })
            .fold(f32::MAX, f32::min);

        let mut de_failures = Vec::new();
        let mut field_failures = Vec::new();
        let mut checked = 0usize;
        let mut worst_de = 0.0f32;

        for r in (0..=255usize).step_by(STEP) {
            for g in (0..=255usize).step_by(STEP) {
                for b in (0..=255usize).step_by(STEP) {
                    let src = Srgb::from_u8(r as u8, g as u8, b as u8);
                    if !hull.contains(LinearRgb::from(src)) {
                        continue;
                    }
                    checked += 1;
                    let (de, counts) = census_patch(src, &palette, PATCH, SKIP);
                    worst_de = worst_de.max(de);
                    if de > MAX_DE {
                        de_failures.push(format!("  #{r:02X}{g:02X}{b:02X}: dE={de:.4}"));
                    }

                    let target = Oklab::from(LinearRgb::from(src));
                    let chroma = (target.a * target.a + target.b * target.b).sqrt();
                    if chroma >= dullest_ink_chroma {
                        continue;
                    }
                    let total: u32 = counts.iter().sum();
                    let (idx, count) = counts[2..]
                        .iter()
                        .enumerate()
                        .max_by_key(|&(_, c)| *c)
                        .map(|(i, c)| (i + 2, *c))
                        .expect("palette has chromatic entries");
                    let pct = 100.0 * count as f32 / total as f32;
                    if pct > MAX_SINGLE_INK_PCT {
                        field_failures.push(format!(
                            "  #{r:02X}{g:02X}{b:02X} (chroma {chroma:.3}): entry {idx} \
                             covers {pct:.1}% of the patch (max {MAX_SINGLE_INK_PCT:.0}%) \
                             — a colour rendered as a field of one ink. dE={de:.4} is \
                             fine, which is the point."
                        ));
                    }
                }
            }
        }

        assert!(checked > 600, "census collapsed to {checked} colours");
        assert!(
            de_failures.is_empty(),
            "In-gamut colours the ditherer cannot reproduce ({} of {checked}):\n{}",
            de_failures.len(),
            de_failures.join("\n")
        );
        assert!(
            field_failures.is_empty(),
            "In-gamut colours rendered as a field of one ink ({} of {checked}):\n{}",
            field_failures.len(),
            field_failures.join("\n")
        );
    }
```

- [ ] **Step 3: Run it and read the worst dE**

Run: `cargo test -p eink-dither --lib test_in_gamut_census -- --nocapture`

Expected: the dE assert passes (bound is infinite), the field-colour assert **FAILS** with a long list. Note the printed `worst dE`.

- [ ] **Step 4: Set the dE bound from the measurement**

Replace `const MAX_DE: f32 = f32::INFINITY;` with the printed worst dE rounded **up** to the next 0.01, plus 0.01 of headroom, and record the measured figure in the constant's doc comment:

```rust
        /// Worst in-gamut dE measured before this change was <MEASURED>;
        /// the bound is that rounded up plus 0.01 of headroom. This is a
        /// no-regression gate: the fix must not make any reachable colour
        /// less accurate than it already is.
        const MAX_DE: f32 = <MEASURED_ROUNDED_UP_PLUS_0_01>;
```

Remove the `println!` added in Step 2.

- [ ] **Step 5: Run again and confirm the shape of the failure**

Run: `cargo test -p eink-dither --lib test_in_gamut_census`

Expected: **FAIL**, and the message must be the *field-colour* assert only. If the dE assert also fails, the bound was set too tight — widen it to the measured value and say so in the doc comment.

- [ ] **Step 6: Delete the throwaway probe**

```bash
rm crates/eink-dither/tests/probe_skin_throwaway.rs
```

- [ ] **Step 7: Confirm the rest of the suite is untouched**

Run: `cargo test --workspace 2>&1 | tail -30`

Expected: exactly two failures in `eink-dither` — `test_neutral_grey_has_no_dominant_chromatic_ink` and `test_in_gamut_census`. Everything else passes.

- [ ] **Step 8: Commit**

```bash
git add crates/eink-dither/src/domain_tests.rs
git rm crates/eink-dither/tests/probe_skin_throwaway.rs
git diff --cached --name-only    # must list exactly those two paths
git commit
```

Message:
```
test(dither): every colour the panel can reach, measured two ways

An in-gamut census over 677 colours. dE guards accuracy; a second rule
guards against a colour being rendered as a field of one ink, which dE
cannot see -- grey 128 is 77.6% green at dE 0.063.

The field-colour rule is derived from the palette rather than picked: a
target with less chroma than the dullest chromatic ink cannot legitimately
be mostly that ink.

Red on purpose. Two failing tests in eink-dither is now the expected state.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 2: The wedge fan

**Files:**
- Create: `crates/eink-dither/src/gamut/wedges.rs`
- Modify: `crates/eink-dither/src/gamut/mod.rs` (add `pub mod wedges;` after `pub mod mapper;`)
- Modify: `crates/eink-dither/src/palette/palette.rs` (widen one const)

**Interfaces:**
- Consumes: `Palette::actual_oklab`, `Palette::actual_linear`, `Palette::len`, `crate::gamut::hull::Hull::{from_palette, is_mappable}`, `crate::palette::palette::CHROMA_DETECTION_THRESHOLD`.
- Produces:
  - `pub struct WedgeFan`
  - `pub fn WedgeFan::from_palette(palette: &Palette) -> Option<WedgeFan>`
  - `pub fn WedgeFan::weights(&self, target: LinearRgb, out: &mut [f32])`

- [ ] **Step 1: Widen the chroma threshold constant and re-export it**

One source of truth for "is this ink chromatic": the wedge fan must make the
same call the palette's own metric auto-detection makes.

`mod palette;` is **private** inside `palette/mod.rs`, so making the constant
`pub(crate)` is not enough on its own — it also needs re-exporting, exactly as
the types beside it are.

In `crates/eink-dither/src/palette/palette.rs`, change

```rust
const CHROMA_DETECTION_THRESHOLD: f32 = 0.03;
```

to

```rust
pub(crate) const CHROMA_DETECTION_THRESHOLD: f32 = 0.03;
```

In `crates/eink-dither/src/palette/mod.rs`, add below the existing re-export:

```rust
pub(crate) use palette::CHROMA_DETECTION_THRESHOLD;
```

The path from elsewhere in the crate is then
`crate::palette::CHROMA_DETECTION_THRESHOLD`.

- [ ] **Step 2: Write the failing tests**

Create `crates/eink-dither/src/gamut/wedges.rs` containing only the module doc and this test module, so it compiles to a failure about missing items.

```rust
//! A fan of tetrahedra around the palette's black-white axis.
//!
//! Nearest-neighbour selection picks the single closest ink. On a panel whose
//! chromatic inks are dull they crowd the neutral axis, out-compete black and
//! white on distance, and a muted colour is rendered as a field of one ink
//! with black never chosen at all.
//!
//! Ostromoukhov's construction (SPIE 1909, 1993) partitions the gamut into
//! wedges spanned by two hue-adjacent chromatic inks **plus black and white**,
//! so the black-white segment is a shared edge of every wedge. A neutral's
//! coordinates are then `(w, 0, 0, k)` whichever wedge it lands in, and grey
//! balance is exact by construction rather than by tuning.
//!
//! Delaunay would not do: nothing in the empty-circumsphere criterion
//! guarantees the black-white segment is even an edge.
//!
//! All geometry is in **linear RGB**, where light adds. The palette's colours
//! are not a convex set in OKLab, so the mixture cannot be solved there --
//! `gamut/hull.rs` computes its hull in linear RGB for the same reason.

use crate::color::LinearRgb;
use crate::gamut::hull::Hull;
use crate::palette::CHROMA_DETECTION_THRESHOLD;
use crate::Palette;

#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests name `Oklab`; importing it at module scope would trip
    // `clippy -D warnings` on an unused import.
    use crate::color::Oklab;
    use crate::gamut::test_support::{four_grey, panel_measured};
    use crate::Srgb;

    fn weights_of(fan: &WedgeFan, palette: &Palette, c: Srgb) -> Vec<f32> {
        let mut w = vec![0.0; palette.len()];
        fan.weights(LinearRgb::from(c), &mut w);
        w
    }

    /// Six inks, four hue-adjacent pairs, four wedges.
    #[test]
    fn a_six_ink_panel_yields_four_wedges() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).expect("six-ink panel carries a fan");
        assert_eq!(fan.wedge_count(), 4);
    }

    /// A greyscale palette has no hue circle to fan around.
    #[test]
    fn a_greyscale_palette_carries_no_fan() {
        assert!(WedgeFan::from_palette(&four_grey()).is_none());
    }

    /// The whole point: a neutral's recipe is black and white only.
    #[test]
    fn a_neutral_recruits_only_black_and_white() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        for level in [32u8, 64, 96, 128, 160, 192, 224] {
            let w = weights_of(&fan, &p, Srgb::from_u8(level, level, level));
            let chromatic: f32 = w[2..].iter().sum();
            assert!(
                chromatic < 1e-4,
                "grey {level} recruited {:.4} of chromatic ink: {w:?}",
                chromatic
            );
            assert!((w[0] + w[1] - 1.0).abs() < 1e-4, "grey {level}: {w:?}");
        }
    }

    /// Grey 128 on this panel is 26% white and 74% black, because measured
    /// white is 0.7157 in linear light. The fan must produce that split.
    #[test]
    fn the_neutral_split_matches_the_closed_form() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        let w = weights_of(&fan, &p, Srgb::from_u8(128, 128, 128));
        let target = LinearRgb::from(Srgb::from_u8(128, 128, 128)).r;
        let white = p.actual_linear(1).r;
        let expected_white = target / white;
        assert!(
            (w[1] - expected_white).abs() < 0.01,
            "white share {:.3}, closed form {:.3}",
            w[1],
            expected_white
        );
    }

    /// An ink is its own pure recipe.
    #[test]
    fn every_ink_is_its_own_recipe() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        for i in 0..p.len() {
            let mut w = vec![0.0; p.len()];
            fan.weights(p.actual_linear(i), &mut w);
            assert!(w[i] > 0.99, "ink {i} weighted {:.3} on itself: {w:?}", w[i]);
        }
    }

    /// Weights are a mixture: non-negative and summing to one.
    #[test]
    fn weights_are_a_normalised_mixture() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        for r in (0..=255u32).step_by(51) {
            for g in (0..=255u32).step_by(51) {
                for b in (0..=255u32).step_by(51) {
                    let w = weights_of(&fan, &p, Srgb::from_u8(r as u8, g as u8, b as u8));
                    let sum: f32 = w.iter().sum();
                    assert!((sum - 1.0).abs() < 1e-3, "sum {sum} at {r},{g},{b}");
                    assert!(w.iter().all(|&x| x >= 0.0), "negative weight: {w:?}");
                }
            }
        }
    }

    /// The field must be continuous, or it draws contours in smooth content.
    ///
    /// `tests/spike_simplex.rs` died of exactly this: it restricted candidates
    /// to the support of a *freely optimised* mixture and measured its own
    /// support field jumping 0.090 in one step, against its note that "a
    /// smooth field would stay well under 0.1". A fixed fan has no such jump,
    /// because black and white are in every cell and never drop out, and two
    /// neighbouring wedges agree exactly on their shared face.
    #[test]
    fn the_weight_field_is_continuous() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        let mut worst = 0.0f32;
        let mut worst_at = String::new();

        let mut probe = |a: Srgb, b: Srgb, label: &str, worst: &mut f32, at: &mut String| {
            let wa = weights_of(&fan, &p, a);
            let wb = weights_of(&fan, &p, b);
            let step = wa
                .iter()
                .zip(&wb)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0f32, f32::max);
            if step > *worst {
                *worst = step;
                *at = label.to_string();
            }
        };

        // Lightness ramps at several hues, one 8-bit step apart.
        for &(dr, dg, db) in &[(1.0, 1.0, 1.0), (1.0, 0.6, 0.5), (0.4, 0.7, 1.0), (0.6, 1.0, 0.6)] {
            for v in 8..248u32 {
                let mk = |t: u32| {
                    Srgb::from_u8(
                        (t as f32 * dr) as u8,
                        (t as f32 * dg) as u8,
                        (t as f32 * db) as u8,
                    )
                };
                probe(mk(v), mk(v + 1), &format!("ramp {dr},{dg},{db} at {v}"), &mut worst, &mut worst_at);
            }
        }

        // Hue sweeps at several lightnesses, one degree apart.
        for &l in &[0.25f32, 0.4, 0.55, 0.7] {
            for deg in 0..360u32 {
                let mk = |d: u32| {
                    let rad = (d as f32).to_radians();
                    let c = 0.06f32;
                    let lab = Oklab::new(l, c * rad.cos(), c * rad.sin());
                    Srgb::from(LinearRgb::from(lab))
                };
                probe(mk(deg), mk((deg + 1) % 360), &format!("hue L={l} at {deg}"), &mut worst, &mut worst_at);
            }
        }

        println!("largest single-step weight change: {worst:.4} at {worst_at}");
        assert!(
            worst < 0.02,
            "weight field jumps {worst:.4} in one step at {worst_at} — \
             a discontinuous field draws contours in smooth content"
        );
    }
}
```

- [ ] **Step 3: Register the module and run to verify it fails**

In `crates/eink-dither/src/gamut/mod.rs`, add `pub mod wedges;` in alphabetical position (after `pub mod mapper;`).

Run: `cargo test -p eink-dither --lib wedges`
Expected: **compile error** — `WedgeFan` not found.

- [ ] **Step 4: Implement `WedgeFan`**

Append to `crates/eink-dither/src/gamut/wedges.rs`, above the test module.

```rust
/// One tetrahedron: black, white, and two hue-adjacent chromatic inks.
#[derive(Debug, Clone)]
struct Wedge {
    /// Palette indices in coordinate order: `[black, white, c0, c1]`.
    inks: [usize; 4],
    /// Black's position, the origin the other three are measured from.
    origin: [f32; 3],
    /// Inverse of the matrix whose columns are `white - black`, `c0 - black`
    /// and `c1 - black`. Multiplying it by `p - black` gives the last three
    /// barycentric coordinates; black's is one minus their sum.
    inv: [[f32; 3]; 3],
}

/// The fan of wedges around a palette's black-white axis.
#[derive(Debug, Clone)]
pub struct WedgeFan {
    wedges: Vec<Wedge>,
    len: usize,
}

/// A wedge whose matrix determinant is below this is treated as flat.
const DET_EPS: f32 = 1e-9;

fn invert3(m: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < DET_EPS {
        return None;
    }
    let d = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
        ],
    ])
}

impl WedgeFan {
    /// Build the fan, or `None` when the palette cannot carry one.
    ///
    /// Returns `None` when there are fewer than three chromatic inks
    /// (greyscale, black-and-white, a single spot colour), when black and
    /// white coincide, when the hull is not mappable, or when any wedge comes
    /// out flat. Callers fall back to plain nearest-neighbour selection, which
    /// is today's behaviour bit for bit.
    ///
    /// `Hull::is_mappable()` is `shape == Volume && neutral_found` — exactly
    /// "a full 3-D hull in which a reachable neutral was found", which is the
    /// condition under which a fan around the black-white axis means anything.
    pub fn from_palette(palette: &Palette) -> Option<Self> {
        let len = palette.len();
        if len < 5 {
            return None;
        }

        let mut white = 0usize;
        let mut black = 0usize;
        for i in 1..len {
            if palette.actual_oklab(i).l > palette.actual_oklab(white).l {
                white = i;
            }
            if palette.actual_oklab(i).l < palette.actual_oklab(black).l {
                black = i;
            }
        }
        if white == black {
            return None;
        }

        // Chromatic inks, ordered around the hue circle. `atan2` returns
        // (-pi, pi]; sorting on it walks the circle once, and the last pair
        // wraps to the first.
        let mut chromatic: Vec<(f32, usize)> = (0..len)
            .filter(|&i| i != white && i != black)
            .filter_map(|i| {
                let c = palette.actual_oklab(i);
                let chroma = (c.a * c.a + c.b * c.b).sqrt();
                (chroma > CHROMA_DETECTION_THRESHOLD).then_some((c.b.atan2(c.a), i))
            })
            .collect();
        if chromatic.len() < 3 {
            return None;
        }
        chromatic.sort_by(|a, b| a.0.total_cmp(&b.0));

        if !Hull::from_palette(palette).is_mappable() {
            return None;
        }

        let k = palette.actual_linear(black);
        let origin = [k.r, k.g, k.b];
        let edge = |i: usize| {
            let c = palette.actual_linear(i);
            [c.r - origin[0], c.g - origin[1], c.b - origin[2]]
        };
        let e_white = edge(white);

        let mut wedges = Vec::with_capacity(chromatic.len());
        for pair in 0..chromatic.len() {
            let c0 = chromatic[pair].1;
            let c1 = chromatic[(pair + 1) % chromatic.len()].1;
            let e0 = edge(c0);
            let e1 = edge(c1);
            // Rows are the coordinate axes; columns are the edge vectors.
            let m = [
                [e_white[0], e0[0], e1[0]],
                [e_white[1], e0[1], e1[1]],
                [e_white[2], e0[2], e1[2]],
            ];
            wedges.push(Wedge {
                inks: [black, white, c0, c1],
                origin,
                inv: invert3(m)?,
            });
        }

        Some(Self { wedges, len })
    }

    /// How many wedges the fan holds. One per hue-adjacent pair.
    pub fn wedge_count(&self) -> usize {
        self.wedges.len()
    }

    /// The mixture that reproduces `target`, one weight per palette entry.
    ///
    /// Weights are non-negative and sum to one. `out.len()` must equal the
    /// palette's length.
    ///
    /// The wedge whose smallest coordinate is largest is kept. Inside the fan
    /// that is the containing wedge and no coordinate is negative, so the
    /// clamp below does nothing. Outside it — an out-of-gamut colour, which
    /// the error-loaded path can produce — it is the nearest wedge, and
    /// clamping then renormalising projects the point onto it. Renormalising
    /// matters: without it the weights of an out-of-gamut colour would sum to
    /// less than one and the bias built on them would silently weaken.
    ///
    /// Two neighbouring wedges share the face on which the absent ink's
    /// coordinate is exactly zero, and they agree there on every other
    /// coordinate. The field is therefore continuous across the whole fan.
    pub fn weights(&self, target: LinearRgb, out: &mut [f32]) {
        debug_assert_eq!(out.len(), self.len);
        out.fill(0.0);

        let p = [target.r, target.g, target.b];
        let mut best_min = f32::NEG_INFINITY;
        let mut best_coords = [0.0f32; 4];
        let mut best_inks = self.wedges[0].inks;

        for w in &self.wedges {
            let d = [
                p[0] - w.origin[0],
                p[1] - w.origin[1],
                p[2] - w.origin[2],
            ];
            let cw = w.inv[0][0] * d[0] + w.inv[0][1] * d[1] + w.inv[0][2] * d[2];
            let c0 = w.inv[1][0] * d[0] + w.inv[1][1] * d[1] + w.inv[1][2] * d[2];
            let c1 = w.inv[2][0] * d[0] + w.inv[2][1] * d[1] + w.inv[2][2] * d[2];
            let ck = 1.0 - cw - c0 - c1;
            let smallest = ck.min(cw).min(c0).min(c1);
            if smallest > best_min {
                best_min = smallest;
                best_coords = [ck, cw, c0, c1];
                best_inks = w.inks;
            }
        }

        let mut sum = 0.0f32;
        for c in best_coords.iter_mut() {
            if *c < 0.0 {
                *c = 0.0;
            }
            sum += *c;
        }
        if sum <= 0.0 {
            return;
        }
        for (slot, &ink) in best_inks.iter().enumerate() {
            out[ink] = best_coords[slot] / sum;
        }
    }
}
```

- [ ] **Step 5: Run the wedge tests**

Run: `cargo test -p eink-dither --lib wedges -- --nocapture`
Expected: all seven PASS. Note the printed *largest single-step weight change*.

If `the_weight_field_is_continuous` fails, do **not** widen the bound to make it pass — a discontinuous field is the failure mode that killed the earlier spike. Report the number and the location instead.

- [ ] **Step 6: Record the measured continuity figure**

Replace the `0.02` bound with the measured worst rounded up to the next 0.005, and put the measurement in the assert's doc comment. If the measured value is already above 0.02, stop and report.

- [ ] **Step 7: Verify nothing else moved**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | tail -30
```
Expected: still exactly two failures, the same two as Task 1.

- [ ] **Step 8: Commit**

```bash
git add crates/eink-dither/src/gamut/wedges.rs \
        crates/eink-dither/src/gamut/mod.rs \
        crates/eink-dither/src/palette/palette.rs
git diff --cached --name-only
git commit
```

Message:
```
feat(dither): a fan of wedges that all share the black-white axis

Ostromoukhov's construction (SPIE 1909, 1993): each wedge is two
hue-adjacent chromatic inks plus black and white, so the black-white
segment is a shared edge of every one. A neutral's coordinates are then
(w, 0, 0, k) whichever wedge it lands in.

Delaunay would not give this -- nothing in the empty-circumsphere
criterion makes the black-white segment an edge.

Geometry only; nothing consumes it yet.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 3: Biased selection on `Palette`

**Files:**
- Modify: `crates/eink-dither/src/palette/palette.rs` (new method after `find_nearest`, tests in the existing `mod tests`)

**Interfaces:**
- Consumes: `Palette::{distance, find_nearest, is_euclidean, len}`, `ColourModel`.
- Produces: `pub fn Palette::find_nearest_biased(&self, color: Oklab, model: ColourModel, lambda: f32, bias: &[f32]) -> (usize, f32)`

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` in `palette.rs`.

```rust
    /// `lambda == 0` must be the old code path exactly, so the retreat from
    /// this whole feature is a property of the code and not a claim.
    #[test]
    fn a_zero_lambda_is_plain_nearest_neighbour() {
        let p = make_6_color_palette();
        let bias = vec![0.9, 0.1, 0.5, 0.2, 0.7, 0.3];
        for r in (0..=255u32).step_by(37) {
            for g in (0..=255u32).step_by(41) {
                for b in (0..=255u32).step_by(43) {
                    let c = Oklab::from(LinearRgb::from(Srgb::from_u8(
                        r as u8, g as u8, b as u8,
                    )));
                    for model in [ColourModel::Nominal, ColourModel::Measured] {
                        assert_eq!(
                            p.find_nearest_biased(c, model, 0.0, &bias),
                            p.find_nearest(c, model),
                            "diverged at {r},{g},{b}"
                        );
                    }
                }
            }
        }
    }

    /// A large enough discount moves the choice to the favoured ink.
    #[test]
    fn a_bias_can_move_the_choice() {
        let p = make_6_color_palette();
        let grey = Oklab::from(LinearRgb::from(Srgb::from_u8(128, 128, 128)));
        let (plain, _) = p.find_nearest(grey, ColourModel::Measured);

        let mut bias = vec![0.0; p.len()];
        let other = (plain + 1) % p.len();
        bias[other] = 1.0;
        let (biased, _) = p.find_nearest_biased(grey, ColourModel::Measured, 10.0, &bias);
        assert_eq!(biased, other, "a discount of 10 dE failed to move the choice");
    }

    /// The returned distance keeps `find_nearest`'s units -- squared for
    /// Euclidean -- so callers reading it do not have to know about the bias.
    #[test]
    fn the_returned_distance_is_unbiased_and_in_find_nearest_units() {
        let p = make_6_color_palette();
        let grey = Oklab::from(LinearRgb::from(Srgb::from_u8(128, 128, 128)));
        let bias = vec![0.0; p.len()];
        let (bi, bd) = p.find_nearest_biased(grey, ColourModel::Measured, 0.5, &bias);
        let (pi, pd) = p.find_nearest(grey, ColourModel::Measured);
        assert_eq!(bi, pi);
        assert!((bd - pd).abs() < 1e-6, "biased {bd}, plain {pd}");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p eink-dither --lib palette::tests`
Expected: **compile error** — no method `find_nearest_biased`.

- [ ] **Step 3: Implement the method**

Insert directly after `find_nearest` in `palette.rs`.

```rust
    /// Like [`find_nearest`](Self::find_nearest), but entry `i`'s distance is
    /// reduced by `lambda * bias[i]` before the comparison.
    ///
    /// `bias` is a mixture: one non-negative weight per palette entry, summing
    /// to one — see [`WedgeFan::weights`](crate::gamut::wedges::WedgeFan::weights).
    /// `lambda` is the discount, in OKLab dE, that a full-weight entry earns.
    ///
    /// **Why this is not the `kchroma` mistake.** HyAB's chroma coupling is
    /// biased for error diffusion at every non-zero weight, because it
    /// penalises a *property of the answer*: no mixture escapes the penalty,
    /// so the achieved average itself shifts and diffusion cannot correct it.
    /// This bias instead chooses *between mixtures that all hit the same
    /// target* — six inks and three equations leave a two-parameter family of
    /// exact solutions. The error term is untouched, so the average still
    /// converges; only which of the exact recipes is used changes.
    ///
    /// With `lambda == 0.0` this delegates to `find_nearest` and is bit-for-bit
    /// identical, which is what makes turning the feature off a property of
    /// the code rather than a claim.
    ///
    /// The returned distance is the winner's **unbiased** distance, in the same
    /// units `find_nearest` returns (squared, for Euclidean).
    ///
    /// # Panics
    ///
    /// Debug builds assert `bias.len() == self.len()`.
    pub fn find_nearest_biased(
        &self,
        color: Oklab,
        model: ColourModel,
        lambda: f32,
        bias: &[f32],
    ) -> (usize, f32) {
        if lambda == 0.0 {
            return self.find_nearest(color, model);
        }
        debug_assert_eq!(bias.len(), self.len());

        let pixel_chroma = (color.a * color.a + color.b * color.b).sqrt();
        let entries = match model {
            ColourModel::Nominal => &self.official_oklab,
            ColourModel::Measured => &self.actual_oklab,
        };
        // Euclidean distances are squared; the bias is in dE, so bring them
        // into the same units before subtracting. `is_euclidean`'s doc already
        // tells callers needing linear distances to do exactly this.
        let squared = self.is_euclidean();

        let mut best_idx = 0;
        let mut best_score = f32::MAX;
        let mut best_dist = f32::MAX;

        for (i, &palette_color) in entries.iter().enumerate() {
            let raw = self.distance(color, palette_color, pixel_chroma, i, model);
            let linear = if squared { raw.sqrt() } else { raw };
            let score = linear - lambda * bias[i];
            if score < best_score {
                best_score = score;
                best_idx = i;
                best_dist = raw;
            }
        }

        (best_idx, best_dist)
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p eink-dither --lib palette::tests`
Expected: PASS, including the three new tests.

- [ ] **Step 5: Verify and commit**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | tail -30
```
Expected: still exactly the same two failures.

```bash
git add crates/eink-dither/src/palette/palette.rs
git diff --cached --name-only
git commit
```

Message:
```
feat(palette): discount an ink by its share of the exact mixture

find_nearest_biased subtracts lambda * weight from each entry's distance.
At lambda 0 it delegates to find_nearest, so switching the feature off is
a property of the code rather than a claim in a commit message.

Unlike HyAB's chroma coupling this is not a bias error diffusion cannot
correct: it chooses between mixtures that all hit the same target, rather
than penalising a property of the answer.

Nothing calls it yet.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 4: Wire it into the dither loop, switched off

**Files:**
- Modify: `crates/eink-dither/src/dither/options.rs` (field, `Default`, setter)
- Modify: `crates/eink-dither/src/dither/mod.rs` (fan construction + the selection call, and a new test)
- Modify: `crates/eink-dither/src/api/builder.rs` (builder setter, and a test)

**Interfaces:**
- Consumes: `WedgeFan::{from_palette, weights}`, `Palette::find_nearest_biased`.
- Produces: `DitherOptions::mixture_bias` (field and `fn mixture_bias(self, v: f32) -> Self`), `EinkDitherer::mixture_bias(self, v: f32) -> Self`.

**Note: no signature change.** `Palette` is already an argument to `dither_with_kernel_noise`, so the fan is built inside it. All 44 existing call sites compile and run unmodified.

- [ ] **Step 1: Add the option field**

In `crates/eink-dither/src/dither/options.rs`, add after `pin_carry` in the struct:

```rust
    /// Discount, in OKLab dE, that a palette entry earns for occupying the
    /// whole of a pixel's exact mixture.
    ///
    /// Nearest-neighbour selection picks the single closest ink. Where a
    /// panel's chromatic inks are dull they crowd the neutral axis, beat black
    /// and white on distance at every muted colour, and the area is rendered
    /// as a field of one ink — a grey that looks green, a brown face that
    /// looks green — at a dE that looks fine, because the eye reads the
    /// majority ink as the colour of the region rather than averaging.
    ///
    /// This discounts each entry by its share of the mixture that actually
    /// reproduces the colour (see
    /// [`WedgeFan`](crate::gamut::wedges::WedgeFan)), so black stops being
    /// unreachable.
    ///
    /// - `0.0` — plain nearest-neighbour selection, bit for bit.
    /// - large — approaches pure argmax-barycentric, maximum grey component
    ///   replacement, and the E Ink patent rule that E Ink themselves later
    ///   backed out of (US 11,527,216) for compromising image quality inside
    ///   the hull. The dial exists so the trade can be measured rather than
    ///   assumed.
    ///
    /// Internal: deliberately absent from byonk's config surface. The right
    /// value is a property of how dull a panel's inks are, not a taste
    /// setting. Owner ruling 2026-08-22.
    ///
    /// Default: `0.0`
    pub mixture_bias: f32,
```

In `impl Default for DitherOptions`, add `mixture_bias: 0.0,` after `pin_carry: 0.9,`.

Add the setter alongside the others in `impl DitherOptions`:

```rust
    /// Set the mixture bias.
    ///
    /// # Arguments
    /// * `lambda` - Discount in OKLab dE for a full-weight entry. `0.0`
    ///   disables the feature; see [`DitherOptions::mixture_bias`].
    #[inline]
    pub fn mixture_bias(mut self, lambda: f32) -> Self {
        self.mixture_bias = lambda;
        self
    }
```

- [ ] **Step 2: Add the builder setter**

In `crates/eink-dither/src/api/builder.rs`, after the `pin_carry` setter:

```rust
    /// Set the mixture bias.
    ///
    /// See [`DitherOptions::mixture_bias`]. `0.0` is plain nearest-neighbour
    /// selection.
    pub fn mixture_bias(mut self, lambda: f32) -> Self {
        self.dither_opts = self.dither_opts.mixture_bias(lambda);
        self
    }
```

- [ ] **Step 3: Write the failing wiring tests**

In `crates/eink-dither/src/dither/mod.rs`, inside the existing `mod tests`:

```rust
    /// The feature ships off. Task 6 of the plan sets the real value.
    #[test]
    fn the_mixture_bias_defaults_to_off() {
        assert_eq!(DitherOptions::default().mixture_bias, 0.0);
    }

    /// A non-zero bias must actually reach the selection, and must recruit
    /// the black that plain selection never picks on a dull-ink panel.
    #[test]
    fn a_mixture_bias_recruits_black_on_a_neutral() {
        let palette = crate::gamut::test_support::panel_measured();
        let image = vec![LinearRgb::from(Srgb::from_u8(128, 128, 128)); 64 * 64];
        let kernel = &ATKINSON;

        let black_share = |lambda: f32| {
            let opts = DitherOptions {
                mixture_bias: lambda,
                ..Default::default()
            };
            let out = dither_with_kernel_noise(&image, 64, 64, &palette, kernel, &opts, None);
            out.iter().filter(|&&i| i == 0).count() as f32 / out.len() as f32
        };

        let off = black_share(0.0);
        let on = black_share(1.0);
        assert!(off < 0.05, "plain selection already reaches black: {off:.3}");
        assert!(
            on > 0.4,
            "a bias of 1.0 dE recruited only {on:.3} black on a mid grey; \
             the exact recipe is 74% black"
        );
    }
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p eink-dither --lib dither::tests::a_mixture_bias`
Expected: **FAIL** — `on` is the same as `off`, because nothing consumes the option yet.

- [ ] **Step 5: Build the fan in the dither loop**

In `dither_with_kernel_noise`, immediately after `let mut error_buf = ErrorBuffer::new(width, kernel.max_dy + 1);`:

```rust
    // The fan depends only on the palette, so it is built once per call.
    // Four 3x3 inversions -- far cheaper than `Hull::from_palette`, which the
    // crate already runs per palette resolve, and skipped entirely when the
    // feature is off.
    let fan = (options.mixture_bias > 0.0)
        .then(|| crate::gamut::wedges::WedgeFan::from_palette(palette))
        .flatten();
    let mut mixture = vec![0.0f32; palette.len()];
```

- [ ] **Step 6: Swap the selection call**

Replace this line (currently `dither/mod.rs:409`):

```rust
                let (nearest_idx, _dist) = palette.find_nearest(oklab, model);
```

with:

```rust
                // The mixture comes from the SOURCE pixel, not the
                // error-loaded one: it is the recipe for the content, and
                // taking it from the error-loaded value would make the
                // guidance drift with the error it bounds. `Nominal` pixels
                // are flat SVG fills meant to BE an ink (ruling 22) and a fan
                // built from measured colours does not describe them.
                let (nearest_idx, _dist) = match (&fan, model) {
                    (Some(f), ColourModel::Measured) => {
                        f.weights(image[idx], &mut mixture);
                        palette.find_nearest_biased(
                            oklab,
                            model,
                            options.mixture_bias,
                            &mixture,
                        )
                    }
                    _ => palette.find_nearest(oklab, model),
                };
```

- [ ] **Step 7: Run the wiring tests**

Run: `cargo test -p eink-dither --lib dither::tests -- --nocapture`
Expected: PASS, including both new tests.

- [ ] **Step 8: Add a builder test**

In `crates/eink-dither/src/api/builder.rs`'s `mod tests`:

```rust
    #[test]
    fn the_builder_carries_the_mixture_bias() {
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        let ditherer =
            EinkDitherer::new(Palette::new(&colors, None).unwrap()).mixture_bias(0.5);
        assert!((ditherer.dither_opts.mixture_bias - 0.5).abs() < f32::EPSILON);
    }
```

- [ ] **Step 9: Verify the default really changed nothing**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | tail -30
```
Expected: **still exactly two failures** — `test_neutral_grey_has_no_dominant_chromatic_ink` and `test_in_gamut_census`. If any other test moved, the `lambda == 0.0` short-circuit is not doing its job; stop and investigate rather than adjusting the test.

- [ ] **Step 10: Commit**

```bash
git add crates/eink-dither/src/dither/options.rs \
        crates/eink-dither/src/dither/mod.rs \
        crates/eink-dither/src/api/builder.rs
git diff --cached --name-only
git commit
```

Message:
```
feat(dither): the ditherer can consult the mixture, and does not yet

mixture_bias reaches the selection through find_nearest_biased, with the
recipe taken from the source pixel rather than the error-loaded one so the
guidance does not drift with the error it bounds. Nominal pixels keep
plain selection: they are flat SVG fills meant to BE an ink.

Shipped at 0.0, so every test moves exactly as much as it did before --
which is not at all. The value lands in a separate commit, so a regression
can only be attributed to one number.

No signature change: Palette was already an argument, so the fan is built
inside the loop and all 44 call sites are untouched.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 5: The `lambda` sweep diagnostic

**Files:**
- Modify: `crates/eink-dither/src/domain_tests.rs` (one `#[ignore]`d test)

**Interfaces:**
- Consumes: `census_patch` from Task 1, `EinkDitherer::mixture_bias` from Task 4, `crate::gamut::hull::Hull`.
- Produces: printed evidence only. No API.

- [ ] **Step 1: Write the diagnostic**

Add to `mod domain_tests` in `domain_tests.rs`.

```rust
    /// Where does `mixture_bias` earn its keep, and where does it start to
    /// cost?
    ///
    /// Four columns, four different questions:
    ///
    /// - `mean dE` / `max dE` — did accuracy survive. This is the gate the
    ///   in-gamut census enforces.
    /// - `worst 1ink%` — did the field colour go away. Measured over
    ///   near-neutral targets only, the ones a majority chromatic ink is
    ///   definitionally wrong for.
    /// - `K+W%` — the direct measure of the defect. Before the change, eight
    ///   of twelve muted colours used 0% black.
    /// - `banding` — the largest single-step change in the chosen ink's share
    ///   along a lightness ramp. `tests/spike_simplex.rs` died of contouring;
    ///   this is the number that would show it coming back.
    ///
    /// Run: `cargo test -p eink-dither --lib lambda_sweep -- --ignored --nocapture --release`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn lambda_sweep() {
        const STEP: usize = 16;
        const PATCH: usize = 64;
        const SKIP: usize = 8;

        for (pname, palette) in [
            ("measured", crate::gamut::test_support::panel_measured()),
            ("idealised", crate::gamut::test_support::six_colour()),
        ] {
            let hull = crate::gamut::hull::Hull::from_palette(&palette);
            let dullest = (2..palette.len())
                .map(|i| {
                    let c = palette.actual_oklab(i);
                    (c.a * c.a + c.b * c.b).sqrt()
                })
                .fold(f32::MAX, f32::min);

            let mut targets = Vec::new();
            for r in (0..=255usize).step_by(STEP) {
                for g in (0..=255usize).step_by(STEP) {
                    for b in (0..=255usize).step_by(STEP) {
                        let src = Srgb::from_u8(r as u8, g as u8, b as u8);
                        if hull.contains(LinearRgb::from(src)) {
                            targets.push(src);
                        }
                    }
                }
            }

            println!("\n=== {pname}: {} in-gamut targets ===", targets.len());
            println!(
                "{:>6}  {:>8} {:>8}  {:>10}  {:>7}  {:>8}",
                "lambda", "mean dE", "max dE", "worst 1ink", "K+W%", "banding"
            );

            for step in 0..=12u32 {
                let lambda = step as f32 * 0.1;
                let mut sum_de = 0.0f32;
                let mut max_de = 0.0f32;
                let mut worst_ink = 0.0f32;
                let mut sum_kw = 0.0f32;
                let mut n_neutral = 0usize;

                for &src in &targets {
                    let image = vec![src; PATCH * PATCH];
                    let out = EinkDitherer::new(palette.clone())
                        .saturation(1.0)
                        .contrast(1.0)
                        .mixture_bias(lambda)
                        .dither(&image, PATCH, PATCH);
                    let idx = &out.indices()[SKIP * PATCH..];
                    let n = idx.len() as f32;
                    let mut counts = vec![0u32; palette.len()];
                    let (mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32);
                    for &i in idx {
                        counts[i as usize] += 1;
                        let c = palette.actual_linear(i as usize);
                        r += c.r;
                        g += c.g;
                        b += c.b;
                    }
                    let avg = Oklab::from(LinearRgb::new(r / n, g / n, b / n));
                    let target = Oklab::from(LinearRgb::from(src));
                    let de = ((avg.l - target.l).powi(2)
                        + (avg.a - target.a).powi(2)
                        + (avg.b - target.b).powi(2))
                    .sqrt();
                    sum_de += de;
                    max_de = max_de.max(de);

                    let chroma = (target.a * target.a + target.b * target.b).sqrt();
                    if chroma < dullest {
                        n_neutral += 1;
                        let top = *counts[2..].iter().max().unwrap() as f32;
                        worst_ink = worst_ink.max(100.0 * top / n);
                        sum_kw += 100.0 * (counts[0] + counts[1]) as f32 / n;
                    }
                }

                // Banding: walk a neutral ramp and watch the black share.
                let mut banding = 0.0f32;
                let mut prev: Option<f32> = None;
                for level in 8..248u8 {
                    let image = vec![Srgb::from_u8(level, level, level); PATCH * PATCH];
                    let out = EinkDitherer::new(palette.clone())
                        .saturation(1.0)
                        .contrast(1.0)
                        .mixture_bias(lambda)
                        .dither(&image, PATCH, PATCH);
                    let idx = &out.indices()[SKIP * PATCH..];
                    let share =
                        idx.iter().filter(|&&i| i == 0).count() as f32 / idx.len() as f32;
                    if let Some(p) = prev {
                        banding = banding.max((share - p).abs());
                    }
                    prev = Some(share);
                }

                println!(
                    "{lambda:>6.1}  {:>8.4} {:>8.4}  {:>9.1}%  {:>6.1}%  {:>8.4}",
                    sum_de / targets.len() as f32,
                    max_de,
                    worst_ink,
                    sum_kw / n_neutral.max(1) as f32,
                    banding
                );
            }
        }
    }
```

- [ ] **Step 2: Confirm it compiles and is skipped by default**

Run: `cargo test -p eink-dither --lib lambda_sweep`
Expected: `0 passed; 0 failed; 1 ignored`.

- [ ] **Step 3: Verify and commit**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | tail -30
```
Expected: the same two failures, unchanged.

```bash
git add crates/eink-dither/src/domain_tests.rs
git diff --cached --name-only
git commit
```

Message:
```
test(dither): sweep the mixture bias and print what it costs

Four columns for four questions: did accuracy survive, did the field
colour go away, is black finally being used, and is the output starting to
band. Ignored by default; run it with --release.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 6: Choose `lambda` and turn it on

This is the task that changes what byonk renders. It is one number.

**Files:**
- Modify: `crates/eink-dither/src/dither/options.rs` (the default)
- Modify: `crates/eink-dither/src/domain_tests.rs` (only if a bound needs restating, with the reason)

**Interfaces:**
- Consumes: everything from Tasks 1–5.
- Produces: a `DitherOptions::default().mixture_bias` that is not zero.

- [ ] **Step 1: Run the sweep and capture the table**

Run and save the output:
```
cargo test -p eink-dither --lib lambda_sweep -- --ignored --nocapture --release 2>&1 | tee /tmp/lambda-sweep.txt
```

- [ ] **Step 2: Pick the value against stated criteria, in this order**

1. `worst 1ink` on the **measured** palette must be at or below 50%.
2. `max dE` on **both** palettes must stay at or below the census bound from Task 1 Step 4.
3. `banding` must stay at or below its value at `lambda = 0` plus 0.02.
4. Among the values that satisfy all three, take the **smallest** — the least intervention that fixes the defect.

The spec predicts the winner sits between 0.3 and 0.8 (black loses to green by ~0.35 dE at a black weight near 0.7). **If the winner is far outside that range, the model in the spec is wrong; say so in the commit message rather than tuning until it fits.**

**If no value satisfies all three, stop and report.** Do not relax a gate to make a number fit. Write up which criterion each candidate breaks and hand it to the owner.

- [ ] **Step 3: Set the default**

In `crates/eink-dither/src/dither/options.rs`, change `mixture_bias: 0.0,` in `impl Default` to the chosen value, and replace the `Default: 0.0` line in the field's doc comment with the chosen value plus the evidence:

```rust
    /// Default: `<CHOSEN>` — the smallest value at which no in-gamut
    /// near-neutral is a field of one ink, measured by `lambda_sweep` on the
    /// measured E1002 palette. At `0.0` the worst was <BEFORE>% (green on a
    /// mid grey); at `<CHOSEN>` it is <AFTER>%. Accuracy over 677 in-gamut
    /// colours moved from <DE_BEFORE> to <DE_AFTER> worst-case dE.
```

- [ ] **Step 4: Run the two gates**

Run: `cargo test -p eink-dither --lib test_in_gamut_census test_neutral_grey_has_no_dominant_chromatic_ink`
Expected: **both PASS.**

- [ ] **Step 5: Run the whole workspace**

Run: `cargo test --workspace 2>&1 | tail -40`
Expected: **zero failures.**

Two tests are the ones most likely to move, and both are legitimate regression signals rather than tests to adjust:
- `test_dither_perceptual_accuracy_photo` — runs on the *idealised* palette, whose mixtures were already near-equal thirds; the bias should barely touch it.
- `test_photo_muted_color_accuracy` — muted photo colours, the case the E Ink reversal warns about.

If either fails, **lower `lambda`** and repeat from Step 4. Do not widen their bounds; they are the photograph side of the trade, and lowering `lambda` is the retreat the whole design exists to provide.

- [ ] **Step 6: Compare the ignored diagnostics before and after**

```
git stash
cargo test -p eink-dither --lib -- --ignored --nocapture --release 2>&1 | tee /tmp/diag-before.txt
git stash pop
cargo test -p eink-dither --lib -- --ignored --nocapture --release 2>&1 | tee /tmp/diag-after.txt
diff /tmp/diag-before.txt /tmp/diag-after.txt
```

Read `test_dither_versus_gamut_bound` and `test_ink_histogram_versus_optimal_recipe` in particular. Record what moved; it goes in the commit message.

- [ ] **Step 7: Check the cost claim**

The spec's §4.5 claims under 0.1 s of added work on a 1200x1600 frame. Confirm
the order of magnitude rather than leave the claim unmeasured:

```
cargo test -p eink-dither --lib test_in_gamut_census --release -- --nocapture
```

Compare the reported test time against the same run with the default put back
to `0.0`. 677 patches of 64x64 is 2.8M pixels; if the difference exceeds a
second, the per-pixel cost is an order of magnitude above the estimate — record
the real figure and correct §4.5 of the spec.

- [ ] **Step 8: Full verification**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd docs && mdbook build
```

- [ ] **Step 9: Commit**

```bash
git add crates/eink-dither/src/dither/options.rs
git diff --cached --name-only
git commit
```

Message — fill the bracketed figures from the actual measurements:
```
feat(dither)!: black is reachable again, so greys stop being green

One number. mixture_bias goes from 0.0 to <CHOSEN>, the smallest value at
which no in-gamut near-neutral is rendered as a field of one ink.

Before, on the measured E1002 palette, eight of twelve muted colours used
0% black: the panel's inks are dull enough to crowd the neutral axis and
beat black and white on plain distance. Grey 128 came out 77.6% green and
a brown face came out a red/green speckle field, both at a dE that looked
fine. Now the worst single ink over 677 in-gamut colours is <AFTER>%.

Accuracy: worst-case dE over the same census went <DE_BEFORE> -> <DE_AFTER>.
Photographs: <what moved in the two idealised-palette tests>.

Both tests that were red on purpose are green.

BREAKING CHANGE: every six-colour panel renders differently. The output is
more accurate and less colourful on neutrals; set mixture_bias to 0.0 for
the previous behaviour.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 7: Real-panel renders, changelog, handover

Numbers decide accuracy. The owner decides the look, from renders — do not
declare this finished on test output alone.

**Files:**
- Modify: `CHANGES.md` (the `Unreleased` / `### Fixed` section)
- Modify: `docs/HANDOVER.md` (rewrite fresh)

- [ ] **Step 1: Render before and after on the real panel**

Through the `byonk-g18` MCP `render_screen`. Pass **explicit `width` and `height`** — `image_max_width` resamples, and resampling averages away the very clumping under test.

`mixture_bias` is internal, so `render_screen` cannot vary it. Get the pair this way:

- **before** — render against the add-on already deployed on `root@10.46.18.3`, which is 0.19.0 and predates this branch;
- **after** — build this branch as `local_byonk` on the VM and render against that. The recipe is in the `ha-vm-from-source-addon-build` notes: Debian rust base, `BUILD_VERSION` cache-bust, then `ha supervisor restart`.

Save both sets before switching, so the comparison survives the rebuild.

Four screens:
1. the calibration photo with the face — the reported symptom;
2. a neutral ramp;
3. a smooth gradient — the banding check;
4. a text-and-chart dashboard screen — the readability check.

- [ ] **Step 2: Hand the paths to the owner and wait**

Give absolute `file:///` links to every render and ask for the visual call. **Do not proceed until the owner has looked.** If they say photographs got worse, go back to Task 6 Step 3 and lower `lambda`.

- [ ] **Step 3: Add the changelog entry**

In `CHANGES.md` under `## Unreleased`, in a `### Fixed` section (create it after `### New` if absent). User-facing only — no mention of internals, test names or the parameter:

```markdown
- **Greys and muted colours no longer come out tinted.** On six-colour panels,
  flat greys and soft tones such as skin were being drawn mostly in one
  coloured ink — a grey area could read as green, and a face as greenish —
  even though the overall colour was correct. Byonk now works out the actual
  recipe of inks that reproduces a colour and gives black and white their
  proper share, so neutral areas look neutral. Photographs and saturated
  colours are unchanged.
```

- [ ] **Step 4: Rewrite the handover**

Overwrite `docs/HANDOVER.md`. It must state: the branch and HEAD, that `cargo test --workspace` is now fully green (the two deliberately-red tests are fixed), what shipped, the chosen `lambda` and the evidence for it, and what is left.

Carry these forward, still true and still open:

- **byonk defects 5.1 and 5.3–5.7** from the previous handover, unchanged.
- **Defect 5.2 is the owner's to fix:** `docs/src/concepts/content-pipeline.md:261` still documents `error_clamp` with a stale `0.05 – 0.5` range. It is one of the owner's four uncommitted files and was deliberately not touched. **The owner must fix that line before committing that file.**
- **The follow-up from this spec's §2:** probabilistic selection (HANS/PARAWACS) as a second halftoning method, reusing `WedgeFan`. Owner ruling: it should surface to the user as another entry in the dither list. Decide from Task 6's measurements whether it earns its keep.
- **Restore the g18 device to `examples/gphoto`** — it is currently on `local/calibration/color`.
- **Open the PR.** This branch now carries the three fixes from the previous session, this initiative, and the TRMNL X ghosting fixes that were never PR'd (`0fb5c47` is a data-loss fix). Consider splitting.
- The deployed box's `panels.reterminal_e1004.dither.sierra-light` still holds a stale `error_clamp: 0.11`, now ignored and announced at startup. Delete the key.

- [ ] **Step 5: Verify and commit**

```
cd docs && mdbook build
```

```bash
git add CHANGES.md docs/HANDOVER.md
git diff --cached --name-only    # must be exactly these two
git commit
```

Message:
```
docs: the quantiser is fixed; the follow-ups are written down

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

## Task Dependency Order

Tasks run in order 1 → 7. Task 1 is the gate; Tasks 2 and 3 are independent of
each other and both must land before Task 4; Task 5 needs Task 4; Task 6 needs
Task 5; Task 7 needs Task 6 and the owner.
