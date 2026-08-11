# Panel-Colour Pinning (Amended) Implementation Plan — Tasks 3–8

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One per-pixel mask selects the colour model, gamut mapping and pinning
eligibility together — unmarked content is matched against the **nominal** palette and
pinned against it, marked `continuous` content keeps the **measured** palette and is not
pinned — and error never crosses the boundary between the two, exactly as it never crosses
the edge of the screen.

**Architecture:** `Palette` already caches both colour sets, so the model is a per-pixel
*choice of which cached array to consult*, not a second palette. The dither loop receives
one `RegionMap` carrying the continuous mask and the resolved pin map; it uses the mask for
three things — which arrays `find_nearest` and the error term consult, whether a pixel may
be pinned, and whether a kernel tap is allowed to cross. The boundary test joins the
existing frame-edge bounds check in the same nested guard.

**Tech Stack:** Rust, `crates/eink-dither` (pure compute), `byonk` binary crate, resvg for
SVG rasterization, MiniJinja for screen templates.

**Spec:** `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` — **read
Amendment 1 and ruling 23 at the end of that file before Task 3.** They supersede parts of
the body.

**Supersedes:** Tasks 3–5 of `docs/superpowers/plans/2026-08-10-panel-colour-pinning.md`.
Tasks 1 and 2 of that plan are **complete, reviewed and committed** (`c74312f`..`24ce479`)
and remain valid. Do not re-run them. Do not use its `task-3-brief.md`.

## Global Constraints

- **Never `git add -A` or `git add .`** in this repository. Add by explicit path and check
  `git diff --cached` before committing. Untracked local files get swept in otherwise.
- **Cap build parallelism at 2**: prefix cargo commands with `CARGO_BUILD_JOBS=2`.
- **Do not run `make check` in a foreground subagent.** ~10 minutes; the subagent stream
  watchdog fires at 600 s of silence. Task verification is
  `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` plus the named `cargo test`
  invocations. The controller runs the full gate, backgrounded.
- **Run `cargo fmt` before committing.** `make check` runs `cargo fmt`, not `--check`.
- **The clippy gate is `-D warnings` across `--workspace --all-targets`.** A single warning
  anywhere fails the gate, including in test modules.
- **Every test body in this plan is a HYPOTHESIS, not a specification.** Nine plan-authored
  tests in Tasks 1–2 measured unfounded. **Run each mutant listed. If a mutant does not
  fail its named test, that is a plan defect: report it, do not tune the test to green.**
- **A test claiming "X rescues this case" must assert, in the same test, that the case
  needed rescuing.** A comparison test must assert its comparison is non-degenerate.
- **A doc comment asserting a mutation property is an unverified claim** — check it against
  the executed mutation table like any other. This bit three times in Task 2.
- **λ (`pin_carry`) is not exposed in panel YAML or `DitherTuning`.** No user knob.
- **`None` means the feature is OFF**, never "everywhere". This applies to `RegionMap` as it
  did to `pin_eligible`.
- **Do not edit the tracked `config.yaml`.** Copy it and point `CONFIG_FILE` at the copy.
- All comments, names and docs in **English**.
- **CHANGES.md is not touched by this plan** (ruling 21). One entry at merge prep.
- **Pre-existing failures, do not chase:** `cargo test -p eink-dither --lib -- --ignored`
  reports 3 failures in `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}` which panic
  at `resize_lanczos` **by design** — this build has no `image` backend, so it cannot
  resample at all.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/eink-dither/src/palette/palette.rs` | `ColourModel`, nominal matching, `official_chroma` cache | 3 |
| `crates/eink-dither/src/dither/mod.rs` | `RegionMap`; per-pixel model, representative colour, hard stop | 4 |
| `crates/eink-dither/src/api/builder.rs` | `dither_with_regions` replacing `dither_with_pinning` | 5 |
| `src/rendering/svg_to_png.rs` | Pass the tone mask itself; rasterize whenever markup exists | 6 |
| `screens/builtin/calibration/tone/screen.svg` | Backing rect out of the marked group | 7 |
| `crates/eink-dither/tests/` + `src/domain_tests.rs` | The measurement pass | 8 |

---

### Task 3: `ColourModel` and nominal matching in `Palette`

**Files:**
- Modify: `crates/eink-dither/src/palette/palette.rs` (struct fields ~105-111, constructor
  ~196-231, accessors ~256-295, `find_nearest` ~434)
- Modify: `crates/eink-dither/src/lib.rs` (re-export `ColourModel`)
- Test: `crates/eink-dither/src/palette/palette.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: existing `Palette` internals — `official_linear: Vec<LinearRgb>`,
  `official_oklab: Vec<Oklab>`, `actual_linear`, `actual_oklab`, `actual_chroma: Vec<f32>`,
  and `distance(&self, color, palette_color, pixel_chroma, palette_idx)`.
- Produces:
  - `pub enum ColourModel { Nominal, Measured }` — `Copy + Clone + Debug + PartialEq + Eq`
  - `Palette::find_nearest(&self, color: Oklab, model: ColourModel) -> (usize, f32)`
  - `Palette::representative_linear(&self, idx: usize, model: ColourModel) -> LinearRgb`

**Background the implementer needs:**

`find_nearest` today scans `self.actual_oklab` unconditionally — that single line is why
unmarked structure is matched against measured inks. Under owner ruling 22 the unmapped
path assumes the inks **are** the nominal colours.

`distance()` uses a chroma-coupling term that indexes a cached per-entry chroma array
(`actual_chroma`, built at `palette.rs:205`). The nominal model needs its own cached
array — computing it per pixel would put a `sqrt` in the inner loop.

`representative_linear` is the accessor the dither loop will use for the **error term**.
The model must select the colour used for *both* the match and the error, or a constant
per-ink bias gets injected into the diffused error. That coupling is the point of having
one accessor rather than open-coding `actual_linear` at the call site.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `palette/palette.rs`. Use the shared fixture
`crate::gamut::test_support::panel_measured()` — **import it, never copy it**; its
measured inks are what the panel actually produces, and `six_colour`'s idealised primaries
cannot reproduce the real hull.

```rust
    /// The two models disagree about a pure primary, and that disagreement is the
    /// entire point of ruling 22. Nominal green IS green; measured green is a dark
    /// teal that pure green does not resemble.
    #[test]
    fn the_two_models_disagree_about_a_pure_primary() {
        let palette = panel_measured();
        let green_idx = (0..palette.len())
            .find(|&i| palette.official(i).to_bytes() == [0, 255, 0])
            .expect("fixture must carry a nominal pure green");

        let pure_green = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 255, 0)));

        let (nominal_hit, _) = palette.find_nearest(pure_green, ColourModel::Nominal);
        let (measured_hit, _) = palette.find_nearest(pure_green, ColourModel::Measured);

        assert_eq!(
            nominal_hit, green_idx,
            "under the nominal model a pure green must match the green ink exactly"
        );
        // Non-degeneracy: if both models agreed, this test could not detect a
        // mutant that ignores the model argument.
        assert_ne!(
            nominal_hit, measured_hit,
            "the models returned the same index, so this test cannot discriminate — \
             the fixture's measured green is too close to nominal green to test with"
        );
    }

    /// The nominal model matches every nominal ink to itself at distance zero.
    /// This is the property the unmapped path depends on.
    #[test]
    fn every_nominal_ink_matches_itself_under_the_nominal_model() {
        let palette = panel_measured();
        for i in 0..palette.len() {
            let ink = Oklab::from(LinearRgb::from(palette.official(i)));
            let (hit, dist) = palette.find_nearest(ink, ColourModel::Nominal);
            assert_eq!(hit, i, "nominal ink {i} did not match itself");
            assert!(dist < 1e-6, "nominal ink {i} matched itself at distance {dist}");
        }
    }

    /// representative_linear returns the colour the model says the ink IS.
    #[test]
    fn representative_linear_follows_the_model() {
        let palette = panel_measured();
        let red = (0..palette.len())
            .find(|&i| palette.official(i).to_bytes() == [255, 0, 0])
            .expect("fixture must carry a nominal pure red");

        let nominal = palette.representative_linear(red, ColourModel::Nominal);
        let measured = palette.representative_linear(red, ColourModel::Measured);

        assert_eq!(nominal, palette.official_linear(red));
        assert_eq!(measured, palette.actual_linear(red));
        assert_ne!(
            nominal, measured,
            "fixture's nominal and measured red coincide, so this cannot discriminate"
        );
    }

    /// The chroma-coupling term must use the model's own chroma cache.
    ///
    /// NOTE: a RANKING-based probe (comparing which entry comes second under each
    /// model) was tried first and MEASURED UNFOUNDED on this fixture at every grey
    /// level — kchroma=10 keeps every chromatic entry's distance above black/white's
    /// regardless of which chroma cache is read, so the ranking can never flip.
    /// This probe calls `distance()` directly instead, holding pixel, palette colour,
    /// pixel chroma and palette index fixed and varying only `model`, which isolates
    /// exactly the chroma-cache lookup the property is about. Do not "simplify" it
    /// back into a ranking comparison.
    #[test]
    fn the_chroma_coupling_term_follows_the_model() {
        let palette = panel_measured();
        let grey = Oklab::from(LinearRgb::from(Srgb::from_u8(128, 128, 128)));
        let chroma = (grey.a * grey.a + grey.b * grey.b).sqrt();

        let green_idx = (0..palette.len())
            .find(|&i| palette.official(i).to_bytes() == [0, 255, 0])
            .expect("fixture must carry a nominal pure green");
        // Fix `b` to a single Oklab value so only the chroma-cache index (driven
        // by `model`) can move the result — the base positions are identical.
        let b = palette.official_oklab(green_idx);

        let nominal_dist = palette.distance(grey, b, chroma, green_idx, ColourModel::Nominal);
        let measured_dist = palette.distance(grey, b, chroma, green_idx, ColourModel::Measured);

        assert_ne!(
            nominal_dist, measured_dist,
            "distance() gave the same result under both models for identical \
             pixel/palette positions, so it did not consult the model-specific \
             chroma cache"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib palette:: -- --nocapture
```

Expected: compile error — `ColourModel` not found, `find_nearest` takes 1 argument.

**If `the_two_models_disagree_about_a_pure_primary` or `the_chroma_coupling_term_follows_the_model`
fails for a reason other than compilation once implemented — i.e. the models genuinely
agree — that is a plan defect. Report the measured indices. Do not weaken the assertion.**

- [ ] **Step 3: Add the enum and the nominal chroma cache**

In `palette/palette.rs`:

```rust
/// Which set of colours the palette's inks are taken to BE.
///
/// A panel's inks are measured (`Measured`) — that is what the hardware
/// produces, and it is what continuous-tone content must be matched against so
/// gamut mapping and dithering aim at reachable colours.
///
/// Outside a continuous-tone region the pipeline assumes instead that the inks
/// ARE their nominal values (`Nominal`), because that is what an SVG author
/// writes. A rect filled `#00FF00` is meant to be the green ink, not an
/// approximation of an unreachable pure green. Owner ruling 22.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourModel {
    /// The palette's nominal (`official`) colours.
    Nominal,
    /// The panel's measured (`actual`) colours.
    Measured,
}
```

Add the field beside `actual_chroma` in the struct, and build it in the constructor
immediately after `official_oklab` is built (mirror the existing `actual_chroma` code at
`palette.rs:205`):

```rust
        let official_chroma: Vec<f32> = official_oklab
            .iter()
            .map(|c| (c.a * c.a + c.b * c.b).sqrt())
            .collect();
```

Store it in the struct literal alongside the other fields.

- [ ] **Step 4: Thread the model through matching**

`distance()` gains a `model` parameter and indexes the matching chroma cache:

```rust
    fn distance(
        &self,
        color: Oklab,
        palette_color: Oklab,
        pixel_chroma: f32,
        palette_idx: usize,
        model: ColourModel,
    ) -> f32 {
        let palette_chroma = match model {
            ColourModel::Nominal => self.official_chroma[palette_idx],
            ColourModel::Measured => self.actual_chroma[palette_idx],
        };
        // ... existing body, using `palette_chroma` where it used
        // self.actual_chroma[palette_idx]
    }
```

`find_nearest` selects the array it scans:

```rust
    pub fn find_nearest(&self, color: Oklab, model: ColourModel) -> (usize, f32) {
        let pixel_chroma = (color.a * color.a + color.b * color.b).sqrt();
        let entries = match model {
            ColourModel::Nominal => &self.official_oklab,
            ColourModel::Measured => &self.actual_oklab,
        };

        let mut best_idx = 0;
        let mut best_dist = f32::MAX;
        for (i, &palette_color) in entries.iter().enumerate() {
            let dist = self.distance(color, palette_color, pixel_chroma, i, model);
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        (best_idx, best_dist)
    }
```

Add the accessor:

```rust
    /// The colour this model says ink `idx` IS.
    ///
    /// The dither loop must use this for the diffused error term with the SAME
    /// model it matched under. Matching under one model and subtracting the
    /// other injects a constant per-ink bias that error diffusion then spreads
    /// across the whole region.
    #[inline]
    pub fn representative_linear(&self, idx: usize, model: ColourModel) -> LinearRgb {
        match model {
            ColourModel::Nominal => self.official_linear[idx],
            ColourModel::Measured => self.actual_linear[idx],
        }
    }
```

Re-export `ColourModel` from `lib.rs` beside the other palette exports.

- [ ] **Step 5: Update every existing call site to `ColourModel::Measured`**

This preserves today's behaviour exactly. **`grep -rn "find_nearest" crates/eink-dither/src crates/eink-dither/tests`
and pass `ColourModel::Measured` at every site.** There are call sites in the dither loop
and the test modules (the gamut code has NONE — verified); the compiler will find them all, but grep first so you
know how many to expect and can tell a missed one from a mis-edited one.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib -- --nocapture
```

Expected: PASS, whole lib suite, no behaviour change from Step 5's mechanical edit.

- [ ] **Step 7: Mutation-verify, both directions**

| Mutant | Must fail |
|---|---|
| `find_nearest` ignores `model`, always scans `actual_oklab` | `the_two_models_disagree_about_a_pure_primary`, `every_nominal_ink_matches_itself_under_the_nominal_model` |
| `representative_linear` always returns `actual_linear` | `representative_linear_follows_the_model` |
| `distance` always indexes `actual_chroma` | `the_chroma_coupling_term_follows_the_model` |
| `ColourModel::Nominal` arm returns the measured array | all four |

- [ ] **Step 8: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
CARGO_BUILD_JOBS=2 cargo clippy -p eink-dither --lib --tests
git add crates/eink-dither/src/palette/palette.rs crates/eink-dither/src/lib.rs
git diff --cached --stat
git commit -m "feat(eink-dither): ColourModel selects nominal or measured matching"
```

---

### Task 4: `RegionMap` — per-pixel model, representative colour, and the hard stop

**Files:**
- Modify: `crates/eink-dither/src/dither/mod.rs` (signature ~273-281, pin branch ~336-348,
  match/error ~356-368, distribution loop ~393-427)
- Test: `crates/eink-dither/src/dither/mod.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: Task 3's `ColourModel`, `find_nearest(color, model)`,
  `representative_linear(idx, model)`.
- Produces:
  ```rust
  pub(crate) struct RegionMap<'a> {
      /// One entry per pixel of the frame: true where the content is
      /// continuous-tone (marked `data-byonk-tone="continuous"`).
      pub continuous: &'a [bool],
      /// One entry per pixel: `Some(ink)` where the pixel is to be pinned.
      pub pinned: &'a [Option<u8>],
  }
  ```
  `dither_with_kernel_noise(image, width, height, palette, kernel, options, regions: Option<&RegionMap>)`
  — **replaces** the `pinned: Option<&[Option<u8>]>` parameter added by Task 1.

**Background the implementer needs:**

One mask drives three behaviours (owner rulings 22 and 23), so they travel together in one
struct rather than as three parameters that could disagree.

`regions: None` means **measured model everywhere, no pinning, no boundary stops** — today's
behaviour, bit-for-bit. This is the same ruling that governed `pinned: None` and it is
load-bearing: the inverse silently changes every existing caller's output.

The hard stop is ruling 23, in the owner's words: *"it should behave as if the border was a
border — nothing from one side goes through to the other, like the border of the screen."*
The error at a stopped tap is **dropped**, not redistributed, because the screen border
does not conserve error either. The per-pixel accumulated buffer is the only state carried
between pixels, so skipping the crossing taps is sufficient — a scanline that leaves and
re-enters a region resumes with zero inherited error automatically.

### ⚠️ The fixture trap — read before writing a single test

`dither/mod.rs`'s test module **already has** two palette helpers, and **neither can test
this feature**:

- `pin_test_palette()` (`dither/mod.rs:680`) — `Palette::new(&inks, None)`
- `panel_palette()` (`dither/mod.rs:692`) — `Palette::from_hex(&[measured…], None)`

`Palette::new(x, None)` sets `actual = official` (`palette.rs:167`). **Under either helper
the two colour models are identical, so every model test silently passes against every
mutant.** A fresh implementer will reach for the module's own helper by default; that is
the trap.

**Use `crate::gamut::test_support::panel_measured()`** (`gamut/mod.rs:40`) — it is the only
fixture in the tree whose official set (pure primaries) and actual set (`#B50303`,
`#FFEE00`, `#205497`, `#0D876B`) genuinely differ. Import it, never copy it.

The tests below assert non-degeneracy for exactly this reason. **If one of those assertions
fires, do not swap in a different fixture to make it pass — report it.**

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `dither/mod.rs`.

```rust
    /// The representative-colour rule. A flat field of a NOMINAL ink in an
    /// unmarked region has zero quantisation error under the nominal model, so
    /// nothing is diffused and every pixel is that ink. Matching under one model
    /// and subtracting the other would inject a constant bias per pixel.
    #[test]
    fn a_flat_nominal_ink_diffuses_nothing_in_an_unmarked_region() {
        let palette = panel_measured();
        let red_idx = (0..palette.len())
            .find(|&i| palette.official(i).to_bytes() == [255, 0, 0])
            .expect("fixture must carry a nominal pure red");

        const W: usize = 16;
        const H: usize = 16;
        let field = vec![LinearRgb::from(palette.official(red_idx)); W * H];
        let continuous = vec![false; W * H];
        let pinned = vec![None; W * H]; // pinning OFF: this tests matching, not pinning
        let regions = RegionMap { continuous: &continuous, pinned: &pinned };

        let opts = DitherOptions::default().serpentine(false).noise_scale(0.0);
        let out = dither_with_kernel_noise(
            &field, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, Some(&regions),
        );

        assert!(
            out.iter().all(|&i| i == red_idx as u8),
            "a flat nominal ink did not render as that ink with pinning off — the \
             model is not being applied consistently to match and error term"
        );

        // Non-degeneracy: the same field under the MEASURED model must NOT come
        // out uniform, or this test would pass against a mutant that ignores the
        // model entirely.
        let all_marked = vec![true; W * H];
        let measured_regions = RegionMap { continuous: &all_marked, pinned: &pinned };
        let measured_out = dither_with_kernel_noise(
            &field, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, Some(&measured_regions),
        );
        assert!(
            measured_out.iter().any(|&i| i != red_idx as u8),
            "the measured model also rendered this field uniformly, so the test \
             cannot discriminate — report this rather than adjusting it"
        );
    }

    /// Ruling 23. A model boundary is a screen border: no error crosses it in
    /// either direction. The unmarked field beyond the boundary must be
    /// bit-identical regardless of what sits on the marked side.
    #[test]
    fn no_error_crosses_a_model_boundary() {
        let palette = panel_measured();
        const W: usize = 32;
        const H: usize = 16;
        const SPLIT: usize = 16; // left half marked, right half unmarked

        let continuous: Vec<bool> =
            (0..W * H).map(|i| i % W < SPLIT).collect();
        let pinned = vec![None; W * H];
        let regions = RegionMap { continuous: &continuous, pinned: &pinned };
        let opts = DitherOptions::default().serpentine(false).noise_scale(0.0);

        // Same unmarked right half; two very different marked left halves.
        let build = |left: Srgb| -> Vec<LinearRgb> {
            (0..W * H)
                .map(|i| {
                    if i % W < SPLIT {
                        LinearRgb::from(left)
                    } else {
                        LinearRgb::from(Srgb::from_u8(120, 120, 120))
                    }
                })
                .collect()
        };
        let a = build(Srgb::from_u8(200, 30, 30));
        let b = build(Srgb::from_u8(30, 30, 200));

        let out_a = dither_with_kernel_noise(
            &a, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, Some(&regions),
        );
        let out_b = dither_with_kernel_noise(
            &b, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, Some(&regions),
        );

        let right = |out: &[u8]| -> Vec<u8> {
            (0..W * H).filter(|i| i % W >= SPLIT).map(|i| out[i]).collect()
        };
        assert_eq!(
            right(&out_a), right(&out_b),
            "the unmarked half changed when only the MARKED half changed — error \
             crossed the boundary"
        );

        // Non-degeneracy: without the stop these must differ, or the test proves
        // nothing. `regions: None` is the no-stop path (measured everywhere).
        let no_stop_a = dither_with_kernel_noise(
            &a, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, None,
        );
        let no_stop_b = dither_with_kernel_noise(
            &b, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, None,
        );
        assert_ne!(
            right(&no_stop_a), right(&no_stop_b),
            "without the boundary stop the two right halves were ALSO identical, so \
             this geometry cannot detect bleeding — report it, do not adjust it"
        );
    }

    /// The pinned carry obeys the stop too, at any lambda including 1.0.
    #[test]
    fn a_pinned_pixel_on_the_boundary_emits_nothing_across_it() {
        let palette = panel_measured();
        let black_idx = (0..palette.len())
            .find(|&i| palette.official(i).to_bytes() == [0, 0, 0])
            .expect("fixture must carry black");

        const W: usize = 32;
        const H: usize = 8;
        const SPLIT: usize = 16;

        // Unmarked left (pinned black column at the seam), marked right.
        let continuous: Vec<bool> = (0..W * H).map(|i| i % W >= SPLIT).collect();
        let pinned: Vec<Option<u8>> = (0..W * H)
            .map(|i| if i % W == SPLIT - 1 { Some(black_idx as u8) } else { None })
            .collect();
        let regions = RegionMap { continuous: &continuous, pinned: &pinned };

        let build = |left: Srgb| -> Vec<LinearRgb> {
            (0..W * H)
                .map(|i| {
                    if i % W < SPLIT { LinearRgb::from(left) }
                    else { LinearRgb::from(Srgb::from_u8(120, 120, 120)) }
                })
                .collect()
        };
        let opts = DitherOptions::default().serpentine(false).noise_scale(0.0).pin_carry(1.0);

        let out_a = dither_with_kernel_noise(
            &build(Srgb::from_u8(200, 30, 30)), W, H, &palette,
            DitherAlgorithm::Atkinson.kernel(), &opts, Some(&regions),
        );
        let out_b = dither_with_kernel_noise(
            &build(Srgb::from_u8(30, 200, 30)), W, H, &palette,
            DitherAlgorithm::Atkinson.kernel(), &opts, Some(&regions),
        );
        let right = |out: &[u8]| -> Vec<u8> {
            (0..W * H).filter(|i| i % W >= SPLIT).map(|i| out[i]).collect()
        };
        assert_eq!(
            right(&out_a), right(&out_b),
            "a pinned pixel at lambda 1.0 pushed its carry across the boundary"
        );
    }

    /// regions: None is today's behaviour, bit-for-bit.
    #[test]
    fn regions_none_reproduces_the_measured_unpinned_output_exactly() {
        let palette = panel_measured();
        const W: usize = 24;
        const H: usize = 24;
        let img: Vec<LinearRgb> = (0..W * H)
            .map(|i| LinearRgb::from(Srgb::from_u8((i % 256) as u8, 90, 200)))
            .collect();
        let opts = DitherOptions::default().serpentine(false).noise_scale(0.0);

        let none_out =
            dither_with_kernel_noise(&img, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, None);
        let all_marked = vec![true; W * H];
        let no_pins = vec![None; W * H];
        let explicit = RegionMap { continuous: &all_marked, pinned: &no_pins };
        let explicit_out = dither_with_kernel_noise(
            &img, W, H, &palette, DitherAlgorithm::Atkinson.kernel(), &opts, Some(&explicit),
        );

        assert_eq!(
            none_out, explicit_out,
            "regions:None diverged from an all-marked unpinned map; None must be \
             exactly the measured, unpinned, unstopped path"
        );
        // Non-degeneracy: the output must not be a constant frame, or a mutant
        // that zeroes everything satisfies this trivially.
        assert!(
            none_out.iter().any(|&i| i != none_out[0]),
            "reference output is uniform, so the comparison is degenerate"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib dither:: -- --nocapture
```

Expected: compile error — `RegionMap` not found.

- [ ] **Step 3: Add `RegionMap` and thread it through**

Replace the `pinned` parameter. In `dither/mod.rs`:

```rust
/// The per-pixel region information the dither loop needs, travelling together
/// because one mask drives all three behaviours (owner rulings 22 and 23).
///
/// `continuous[i]` is true where the content is continuous-tone. That single bit
/// selects the colour model for the pixel, decides whether the pixel may be
/// pinned, and decides whether a kernel tap may cross to it.
pub(crate) struct RegionMap<'a> {
    pub continuous: &'a [bool],
    pub pinned: &'a [Option<u8>],
}

impl RegionMap<'_> {
    /// The colour model for a pixel: unmarked content is taken to BE its
    /// nominal colours (ruling 22).
    #[inline]
    fn model(&self, idx: usize) -> ColourModel {
        if self.continuous[idx] {
            ColourModel::Measured
        } else {
            ColourModel::Nominal
        }
    }
}
```

In the loop body, resolve the model once per pixel, before the pin branch:

```rust
            let model = regions
                .map(|r| r.model(idx))
                .unwrap_or(ColourModel::Measured);
            let pin = regions.and_then(|r| r.pinned[idx]);
```

In the non-pinned arm, use the model for **both** the match and the error term:

```rust
                let (nearest_idx, _dist) = palette.find_nearest(oklab, model);
                output[idx] = nearest_idx as u8;

                let nearest_linear = palette.representative_linear(nearest_idx, model);
```

- [ ] **Step 4: Add the hard stop to the distribution loop**

The boundary test joins the existing frame-edge guard — a tap that crosses a model boundary
is treated exactly as a tap that leaves the frame. Inside the `if ny < height {` block,
before computing `w`:

```rust
                        // Ruling 23: a model boundary IS a screen border. Nothing
                        // crosses, in either direction, and the error at a stopped
                        // tap is dropped rather than redistributed — the frame edge
                        // does not conserve error either.
                        let nidx = ny * width + nx as usize;
                        let crosses = regions
                            .map(|r| r.continuous[idx] != r.continuous[nidx])
                            .unwrap_or(false);
                        if crosses {
                            continue;
                        }
```

- [ ] **Step 5: Update the two call sites**

`api/builder.rs` currently passes `pin_map.as_deref()`. Task 5 rewrites it properly; for
this task, pass `None` there so the crate compiles, and note it in your report. Also update
the ~19 call sites in `crates/eink-dither/src/domain_tests.rs`, which pass `None` today —
they keep passing `None`.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib -- --nocapture
```

- [ ] **Step 7: Mutation-verify, both directions**

| Mutant | Must fail |
|---|---|
| `model()` always returns `Measured` | `a_flat_nominal_ink_diffuses_nothing_in_an_unmarked_region` |
| Error term uses `actual_linear` while match uses `model` | `a_flat_nominal_ink_diffuses_nothing_in_an_unmarked_region` |
| Drop the `if crosses { continue; }` | `no_error_crosses_a_model_boundary` |
| `crosses` compares `pinned` instead of `continuous` | `no_error_crosses_a_model_boundary` |
| Pinned carry skips the boundary test (emit before the guard) | `a_pinned_pixel_on_the_boundary_emits_nothing_across_it` |
| `regions: None` defaults `model` to `Nominal` | `regions_none_reproduces_the_measured_unpinned_output_exactly` |

- [ ] **Step 8: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
CARGO_BUILD_JOBS=2 cargo clippy -p eink-dither --lib --tests
git add crates/eink-dither/src/dither/mod.rs crates/eink-dither/src/api/builder.rs crates/eink-dither/src/domain_tests.rs
git diff --cached --stat
git commit -m "feat(eink-dither): per-pixel colour model and a hard stop at region boundaries"
```

---

### Task 5: `dither_with_regions` on the builder

**Files:**
- Modify: `crates/eink-dither/src/api/builder.rs` (`dither_with_pinning` → `dither_with_regions`)
- Test: `crates/eink-dither/src/api/builder.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: Task 4's `RegionMap`; Task 2's existing exact-match resolution against
  `Palette::official(i).to_bytes()` before preprocessing, and its resize refusal.
- Produces: `EinkDitherer::dither_with_regions(&self, pixels: &[Srgb], width: usize, height: usize, continuous: Option<&[bool]>) -> DitheredImage`.
  `dither()` delegates with `None`. **`dither_with_pinning` is removed** — byonk is the
  only consumer and this branch is unreleased.

**Background the implementer needs:**

Task 2 shipped `pin_eligible`, which byonk was to build as the **inverse** of the tone mask.
Ruling 22 makes the same mask drive three behaviours, so the crate now takes the tone mask
**as rasterized** and derives eligibility internally. **`continuous` and `pin_eligible` are
boolean inverses with adjacent call sites — the polarity slip is silent and produces a
plausible image either way. It gets its own guard.**

Keep Task 2's hard-won structure: the match resolves on the caller's `Srgb` bytes **before**
preprocessing; pinning is refused across a resize; the length `debug_assert!` stays.

- [ ] **Step 1: Write the failing tests**

Keep every existing test in this module that still applies, updating call sites to the new
name and inverted mask polarity. **`a_near_miss_is_not_pinned`, `plain_dither_is_unchanged_by_this_feature`
and `pinning_is_refused_when_resizing` carry non-degeneracy assertions added in Task 2's fix
rounds — preserve them; they exist because those three tests were degenerate as first
written.** Add:

```rust
    /// The polarity guard. `continuous` is the tone mask, NOT its inverse: an
    /// all-true mask means "all continuous-tone", which must disable pinning.
    /// Inverting this is silent and produces a plausible image either way.
    #[test]
    fn an_all_continuous_mask_disables_pinning() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).noise_scale(0.0).serpentine(false);
        // Existing helper, already in this module: a 32x32 field of `field`
        // with a 2 px vertical line of `line` at HOSTILE_LINE_COLS.
        let px = hostile_line_field(Srgb::from_u8(0xC0, 0x60, 0x20), Srgb::from_u8(0, 0, 0));

        let all_continuous = vec![true; px.len()];
        let none_continuous = vec![false; px.len()];

        let marked =
            ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, Some(&all_continuous));
        let unmarked =
            ditherer.dither_with_regions(&px, HOSTILE_W, HOSTILE_H, Some(&none_continuous));

        let marked_share = line_ink_share(&marked, 0);
        let unmarked_share = line_ink_share(&unmarked, 0);

        assert!(
            unmarked_share > 0.99,
            "an unmarked line was not pinned ({:.1}%) — polarity may be inverted",
            unmarked_share * 100.0
        );
        assert!(
            marked_share < 0.99,
            "an all-continuous mask still pinned the line ({:.1}%) — the mask is \
             being read as pin_eligible rather than as the tone mask",
            marked_share * 100.0
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib api::builder -- --nocapture
```

Expected: compile error — no method `dither_with_regions`.

- [ ] **Step 3: Implement**

```rust
    /// Dither, honouring per-pixel continuous-tone regions.
    ///
    /// `continuous`, when supplied, is one `bool` per input pixel: `true` where
    /// the content is continuous-tone. That bit selects three behaviours at
    /// once (owner rulings 22 and 23):
    ///
    /// - **Colour model.** Unmarked content is matched against the NOMINAL
    ///   palette entries and is taken to BE them; marked content is matched
    ///   against the measured inks.
    /// - **Pinning.** Unmarked pixels whose bytes equal a nominal entry exactly
    ///   are pinned; marked pixels never are.
    /// - **Error diffusion.** No error crosses between marked and unmarked
    ///   pixels, in either direction, exactly as none crosses the frame edge.
    ///
    /// `None` means none of the three: measured model everywhere, no pinning,
    /// no boundary stops — identical output to [`Self::dither`].
    ///
    /// NOTE the polarity: this is the tone mask as rasterized, not the
    /// pin-eligibility mask. An all-`true` slice means "everything is
    /// continuous-tone", which disables pinning.
    pub fn dither_with_regions(
        &self,
        pixels: &[Srgb],
        width: usize,
        height: usize,
        continuous: Option<&[bool]>,
    ) -> DitheredImage {
        let resizing =
            self.preprocess.target_width.is_some() || self.preprocess.target_height.is_some();

        let maps: Option<(Vec<bool>, Vec<Option<u8>>)> = match continuous {
            Some(mask) if !resizing && mask.len() == pixels.len() => {
                let inks: Vec<[u8; 3]> = (0..self.palette.len())
                    .map(|i| self.palette.official(i).to_bytes())
                    .collect();
                let pinned: Vec<Option<u8>> = pixels
                    .iter()
                    .zip(mask.iter())
                    .map(|(px, &is_continuous)| {
                        if is_continuous {
                            return None;
                        }
                        let bytes = px.to_bytes();
                        inks.iter().position(|ink| *ink == bytes).map(|i| i as u8)
                    })
                    .collect();
                Some((mask.to_vec(), pinned))
            }
            _ => {
                debug_assert!(
                    continuous.is_none()
                        || resizing
                        || continuous.map(|m| m.len()) == Some(pixels.len()),
                    "continuous mask length {:?} does not match pixel count {}",
                    continuous.map(|m| m.len()),
                    pixels.len()
                );
                None
            }
        };

        let preprocessor = Preprocessor::new(self.preprocess.clone());
        let result = preprocessor.process(pixels, width, height);

        let dither_opts = self.dither_opts.clone();
        let photo_palette = self.palette.for_error_diffusion();
        let kernel = self.algorithm.kernel();
        let regions = maps
            .as_ref()
            .map(|(c, p)| RegionMap { continuous: c, pinned: p });
        let indices = dither_with_kernel_noise(
            &result.pixels,
            result.width,
            result.height,
            &photo_palette,
            kernel,
            &dither_opts,
            regions.as_ref(),
        );

        DitheredImage::new(indices, result.width, result.height, self.palette.clone())
    }
```

Reduce `dither` to `self.dither_with_regions(pixels, width, height, None)` and delete
`dither_with_pinning`.

- [ ] **Step 4: Run to verify it passes**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib -- --nocapture
```

- [ ] **Step 5: Mutation-verify, both directions**

| Mutant | Must fail |
|---|---|
| Invert the polarity: pin where `is_continuous` is true | `an_all_continuous_mask_disables_pinning` |
| Match against `self.palette.actual(i)` | `the_exact_match_is_against_the_nominal_entry` |
| Drop the `!resizing` condition | `pinning_is_refused_when_resizing` |
| `dither` builds an all-false mask instead of passing `None` | `plain_dither_is_unchanged_by_this_feature` |
| Compare bytes with a tolerance instead of `==` | `a_near_miss_is_not_pinned` |

- [ ] **Step 6: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
CARGO_BUILD_JOBS=2 cargo clippy -p eink-dither --lib --tests
git add crates/eink-dither/src/api/builder.rs
git diff --cached --stat
git commit -m "feat(eink-dither): dither_with_regions takes the tone mask itself"
```

---

### Task 6: byonk passes the tone mask

**Files:**
- Modify: `src/rendering/svg_to_png.rs` (the `has_tone_markup` block ~133-159, the dither
  call ~180)
- Test: `src/rendering/svg_to_png.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: Task 5's `dither_with_regions`; `crate::rendering::tone_mask::has_tone_markup`;
  `SvgRenderer::rasterize_tone_mask`.
- Produces: no new public API. `render_to_palette_png` keeps its signature.

**Background the implementer needs:**

Today the tone mask is rasterized only when the document has markup **and**
`gamut.amount != 0.0`. The mask now has three consumers, so the `amount != 0.0` gate moves
inside — the mask is rasterized whenever the document carries markup, and only the
*mapping* is skipped when amount is zero.

**byonk passes the mask through unchanged — no inversion.** A document with no markup gets
an all-`false` mask: every pixel is structure, so nominal model plus pinning everywhere.
That is ruling 22 plus ruling 18.

**This changes the rendering of every unmarked screen**, because unmarked content now
matches nominal rather than measured inks. That is intended and is the point of ruling 22;
Task 8 measures it.

Note `DisplaySpec` is `{width, height, max_size_bytes}` (`src/models/display_spec.rs:5`) —
construct it the way neighbouring tests in this file do rather than assuming a `Default`.

- [ ] **Step 1: Write the failing test**

`src/rendering/svg_to_png.rs:864` already has a marked-vs-unmarked comparison test — copy
its construction shape. The marked variant is the in-test control: marking makes the bar
ineligible, so it must stay eroded.

```rust
    /// The reported defect at its smallest: a pure-black bar beside a saturated
    /// one, in a document with no tone markup. Unmarked, the bar is pinned and
    /// keeps its black. The same document with the bar marked continuous is the
    /// control — marking makes it ineligible, so it must NOT stay pure.
    #[test]
    fn pure_ink_survives_beside_saturated_content_unless_marked() {
        let unmarked = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="8">
  <rect x="0" y="0" width="20" height="8" fill="#C06020"/>
  <rect x="20" y="0" width="20" height="8" fill="#000000"/>
</svg>"#;
        let marked = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="8">
  <rect x="0" y="0" width="20" height="8" fill="#C06020"/>
  <rect data-byonk-tone="continuous" x="20" y="0" width="20" height="8" fill="#000000"/>
</svg>"#;

        let black_share = |svg: &[u8]| -> f64 {
            let spec = DisplaySpec { width: 40, height: 8, max_size_bytes: 200_000 };
            let palette: Vec<(u8, u8, u8)> = vec![
                (0, 0, 0), (255, 255, 255), (255, 0, 0),
                (255, 255, 0), (0, 0, 255), (0, 255, 0),
            ];
            let renderer = SvgRenderer::new();
            let png = renderer
                .render_to_palette_png(svg, spec, &palette, None, false, None, None)
                .expect("render failed");
            let img = image::load_from_memory(&png).expect("decode failed").to_rgb8();
            let mut black = 0usize;
            let mut total = 0usize;
            for y in 0..8u32 {
                for x in 22..38u32 {
                    total += 1;
                    if img.get_pixel(x, y).0 == [0, 0, 0] {
                        black += 1;
                    }
                }
            }
            black as f64 / total as f64
        };

        let unmarked_share = black_share(unmarked);
        let marked_share = black_share(marked);

        assert!(
            unmarked_share > 0.99,
            "only {:.1}% of the unmarked black bar stayed black — it is not being pinned",
            unmarked_share * 100.0
        );
        assert!(
            marked_share < unmarked_share,
            "marking the bar continuous made no difference ({:.1}% vs {:.1}%), so this \
             test cannot tell pinning from a bar that was never eroded — report it",
            marked_share * 100.0,
            unmarked_share * 100.0
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib rendering::svg_to_png -- --nocapture
```

Expected: FAIL on the first assertion, with a share well under 99%. **Record the observed
number — it is this task's before-measurement.**

- [ ] **Step 3: Restructure the mask block**

Replace the whole `if has_tone_markup` block:

```rust
        // The tone mask has three consumers: gamut mapping acts INSIDE marked
        // regions; outside them the colour model is nominal and exact matches
        // are pinned; and no error crosses between the two. So it is rasterized
        // whenever the document carries markup, and only the mapping is skipped
        // when amount is zero. An unmarked document skips the second
        // rasterization entirely: every pixel is structure.
        let tone_mask: Option<Vec<bool>> =
            if crate::rendering::tone_mask::has_tone_markup(svg_data) {
                let mask = self.rasterize_tone_mask(svg_data, spec)?;
                if mask.len() != pixels.len() {
                    // Cannot happen: both rasterize to `spec`. Loud rather than
                    // silently skipped.
                    return Err(RenderError::Dither(format!(
                        "tone mask length {} does not match frame {}",
                        mask.len(),
                        pixels.len()
                    )));
                }
                Some(mask)
            } else {
                None
            };

        if let Some(mask) = tone_mask.as_ref() {
            let gamut_opts = tuning.and_then(|t| t.gamut).unwrap_or_default();
            if gamut_opts.amount != 0.0 {
                let marked = mask.iter().filter(|m| **m).count();
                tracing::debug!(
                    marked_pixels = marked,
                    total_pixels = pixels.len(),
                    knee = gamut_opts.knee,
                    amount = gamut_opts.amount,
                    max_compression = gamut_opts.max_compression,
                    "applying gamut mapping to continuous-tone regions"
                );
                GamutMapper::new(&eink_palette).map_frame(&mut pixels, mask, gamut_opts);
            }
        }

        // Passed through unchanged: this IS the tone mask, not its inverse.
        // A document with no markup is all-structure.
        let continuous: Vec<bool> = match tone_mask {
            Some(mask) => mask,
            None => vec![false; pixels.len()],
        };
```

Change the dither call:

```rust
        let result = ditherer.dither_with_regions(
            &pixels,
            spec.width as usize,
            spec.height as usize,
            Some(&continuous),
        );
```

- [ ] **Step 4: Run to verify it passes**

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib rendering:: -- --nocapture
```

Record the new share.

- [ ] **Step 5: Run the wider byonk suite**

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib -- --nocapture
```

**Any render-snapshot test that now differs is a real finding — report the diff rather
than updating the expectation.** Unmarked content changing model is expected in principle;
which specific tests move is information the controller needs.

- [ ] **Step 6: Mutation-verify, both directions**

| Mutant | Must fail |
|---|---|
| Invert: `Some(mask) => mask.iter().map(\|m\| !m).collect()` | `pure_ink_survives_beside_saturated_content_unless_marked` |
| `None => vec![true; pixels.len()]` (unmarked doc treated as continuous) | same test's first assertion |
| Move `rasterize_tone_mask` back inside the `amount != 0.0` gate | same test (marked control renders with no mask at all) |

- [ ] **Step 7: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
git add src/rendering/svg_to_png.rs
git diff --cached --stat
git commit -m "feat(render): one tone mask drives colour model, mapping and pinning"
```

---

### Task 7: The tone screen's backing rect leaves the marked group

**Files:**
- Modify: `screens/builtin/calibration/tone/screen.svg:45` (the `<g data-byonk-tone>` block)
- Test: measurement only — see Step 3.

**Interfaces:** none. This is content, not code.

**Background the implementer needs:**

The grid is not drawn as lines; it is a black backing rect showing through 2 px gaps
between the patches drawn over it. Inside the marked group it is continuous-tone as far as
the pipeline is concerned, so it gets the measured model and no pinning — the marked column
would keep a speckled grid while the unmarked column got a crisp one, an artifact of the
authoring rather than the design.

**The rect moves out of the group, kept immediately before it** so document order and
therefore z-order are unchanged. The patches stay marked.

Pure black is in gamut, so this cannot move the mapped patches. It does remove those pixels
from the **adaptation group**, and `R` is a 99th percentile over the marked set
(`PERCENTILE = 0.99`). **Measure `R` before and after; do not assume it is unchanged.**

- [ ] **Step 1: Record `R` before the change**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib gamut::adapt -- --ignored --nocapture
```

There is no existing diagnostic that prints `R` for a real screen —
`crates/eink-dither/tests/gamut_adaptation_diag.rs` exercises `adaptation_factor` on
synthetic `rhos` only. Follow that file's pattern: add a `#[ignore]` diagnostic that
rasterizes this screen's tone mask, collects `ρ` over the marked pixels, and prints
`adaptation_factor(&mut rhos, max_compression)`. **Record the number in your report before
touching the SVG** — this is the before-measurement and it cannot be recovered afterwards.

- [ ] **Step 2: Move the rect**

Take the `<rect ... fill="#000000"/>` backing rect out of the
`<g data-byonk-tone="continuous">` element and place it immediately **before** the `<g>`,
so it still paints under the patches. Leave the photo, the hue sweep and the patches inside.

- [ ] **Step 3: Re-measure `R` and the grid**

Re-run Step 1's command. Then render the screen and measure the black share of the grid
lines in **both** columns:

```bash
CONFIG_FILE=/tmp/tone.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/tone.png
```

with a throwaway config carrying `panel: reterminal_e1002` and
`screen: byonk-builtin/calibration/tone`. **Without the `panel:` line you get a greyscale
render and the trap is silent.**

Report: `R` before, `R` after, grid black share in the marked column, grid black share in
the unmarked column. **The marked column's grid should now be crisp; if it is not, stop and
report — that means the rect is still being caught by the mask.**

- [ ] **Step 4: Commit**

```bash
git add screens/builtin/calibration/tone/screen.svg
git diff --cached --stat
git commit -m "fix(screens): the tone grid is structure, not continuous tone"
```

---

### Task 8: The measurement pass

**Files:**
- Create: `crates/eink-dither/tests/region_model_diag.rs`
- Modify: `crates/eink-dither/src/domain_tests.rs` (λ sweep)

**Interfaces:** none produced. This task exists to produce numbers.

**Background the implementer needs:**

**These diagnostics are non-asserting by owner ruling 20** — their printed output is the
deliverable. Asserting a threshold now would defend a constant with a test derived from the
same unvalidated plan. Mark them `#[ignore]` and follow the established pattern in
`gamut_adaptation_diag.rs` and `ray_geometry_diagnostic`.

**Measure the pixels the change touches, never a whole-image mean.** On the portrait, all
four gamut anchors scored 0.0545–0.0550 mean chroma and looked identical because only 7% of
pixels were out of gamut and the untouched 93% swamped them. Restricted to the affected
pixels the spread was 68% to 90%.

Use byonk's own shipping assets — `screens/builtin/calibration/color/photo.png` (portrait,
7% out of gamut) and `screens/builtin/default/background.jpg` (12%). Synthetic fields at
full saturation are unrepresentative.

- [ ] **Step 1: The λ sweep**

For λ ∈ {0.0, 0.5, 0.8, 0.9, 0.95, 1.0}, on the 2 px-line-in-a-hostile-field geometry from
Task 2's tests and on a real screen render, print: line ink purity, and the ink distribution
of the 4 px band on each side of the line. **`pin_carry = 0.9` is provisional and this sweep
is what chooses it.**

- [ ] **Step 2: The unmarked-photograph cost**

This is the feature's main downside under ruling 22 and nobody has looked at it. Render
`photo.png` twice — once with an all-`false` mask (unmarked: nominal model, the new
behaviour) and once all-`true` (marked: measured model, today's behaviour). Print, over the
whole photo and separately over the out-of-gamut pixels only:

- mean OKLab ΔE between the two renders' reconstructed images
- the ink histogram of each
- the share of pixels that changed ink

**Also render both to PNG for the owner to look at**, into `target/dither-compare/`. Note
that a viewer that does not linearise reads these ~30% too dark; the owner's judgement is on
the panel.

- [ ] **Step 3: The boundary artefact**

Ruling 23 makes each region dither as its own frame, so a seam is possible where they meet.
Render a marked photograph directly abutting an unmarked flat field, and print the ink
distribution of the 4 px band on each side of the boundary. **Look for a visible
discontinuity; this risk did not exist before ruling 23.**

- [ ] **Step 4: The swatch win**

The headline case. Render a nominal `#00FF00` and `#0000FF` patch in an unmarked region and
print their ink purity. Today's measured-model behaviour is 51% black / 27% red / 17% teal
for green and 81% black / 13% white / 5% blue for blue; these should now be solid ink.

- [ ] **Step 5: Cost**

```bash
CARGO_BUILD_JOBS=2 cargo test --release -p eink-dither --test region_model_diag -- --ignored --nocapture
```

Print per-frame cost for a worst-case 800×480 frame, with and without a `RegionMap`. The
boundary test adds a comparison per kernel tap; gamut mapping's equivalent measured 218 ms
and was judged acceptable.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add crates/eink-dither/tests/region_model_diag.rs crates/eink-dither/src/domain_tests.rs
git diff --cached --stat
git commit -m "test(eink-dither): measurement pass for the region colour model"
```

---

## After Task 8

Report every measurement to the owner and stop. Open decisions that this plan deliberately
does not make:

1. **λ's shipping value** — Step 1's sweep informs it; `0.9` is provisional.
2. **Whether the unmarked-photograph cost is acceptable** as measured (Step 2). The tradeoff
   is already accepted in principle; the magnitude is not yet known.
3. **Whether `calibration/tone`'s unmarked control column remains legible enough** to serve
   as a control now that it renders under the nominal model.
4. **The authoring documentation** (`docs/src/`) — marking goes from optimisation to
   requirement under ruling 22, which is a user-facing obligation this plan does not
   discharge.
