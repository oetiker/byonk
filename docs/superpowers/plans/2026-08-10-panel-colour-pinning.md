# Panel-Colour Pinning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pixel that is already exactly a panel ink, in a region not marked
`data-byonk-tone="continuous"`, renders as that ink instead of being speckled by
error diffused in from saturated neighbours — while the incoming error is carried on
to its neighbours, attenuated by λ per pinned pixel, so it flows around thin elements
and dies inside large ones.

**Architecture:** Exact matches are resolved **before** preprocessing, on the
caller's `Srgb` bytes, into a `Vec<Option<u8>>` of ink indices. The dither loop
consults that map: a pinned pixel outputs its ink, ignores the accumulated error, and
emits `λ · accumulated` instead of its own quantisation error. Everything downstream
— kernel, blue-noise jitter, serpentine direction, `error_clamp` — is untouched.
eink-dither decides *what is a panel colour*; byonk decides *where pinning is
allowed*, from the inverse of the tone mask it already rasterizes.

**Tech Stack:** Rust, `crates/eink-dither` (no-std-ish pure compute crate), `byonk`
binary crate, resvg for SVG rasterization, MiniJinja for screen templates.

**Spec:** `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` — read it
before Task 1. It carries the measurements and the owner rulings; this plan carries
the code.

## Global Constraints

- **Never `git add -A` or `git add .`** in this repository. Add by explicit path and
  check `git diff --cached` before committing. Untracked local files get swept in
  otherwise.
- **Cap build parallelism at 2**: prefix cargo commands with `CARGO_BUILD_JOBS=2`.
  Shared machine.
- **Do not run `make check` in a foreground subagent.** It takes ~10 minutes and the
  subagent stream watchdog fires at 600 s of silence. Task-level verification is
  `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` plus the named
  `cargo test` invocations. The controller runs the full gate, backgrounded.
- **Run `cargo fmt` before committing.** `make check` runs `cargo fmt`, not
  `cargo fmt --check`, so it rewrites files in place and leaves the tree dirty.
  Code transcribed from this plan is usually not rustfmt-clean.
- **λ (`pin_carry`) is not exposed in panel YAML or `DitherTuning`.** It lives in
  `DitherOptions` with a builder method so tests can sweep it. No user knob in this
  spike.
- **`pin_eligible: None` means pinning is OFF**, never "eligible everywhere".
- **Every new guard is mutation-verified in both directions.** A guard that passes
  against a mutant is not a guard. Each task states its mutants explicitly.
- **Do not edit the tracked `config.yaml`.** Copy it and point `CONFIG_FILE` at the
  copy.
- All comments, names and docs in **English**.
- **CHANGES.md is not touched by this plan.** This is a spike; nothing here is a
  released user-visible change yet.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/eink-dither/src/dither/options.rs` | Add `pin_carry: f32` field + builder | 1 |
| `crates/eink-dither/src/dither/mod.rs` | The pinning branch in `dither_with_kernel_noise` | 1 |
| `crates/eink-dither/src/api/builder.rs` | `dither_with_pinning`, exact-match resolution, resize guard | 2 |
| `crates/eink-dither/src/preprocess/preprocessor.rs` | Correct the wrong doc comment | 2 |
| `src/rendering/svg_to_png.rs` | Build the eligibility mask, call `dither_with_pinning` | 3 |
| `screens/builtin/calibration/tone/screen.svg` | Move the backing rect out of the marked group | 4 |
| `src/services/screen_store.rs` | Update the mask-fraction test's recorded value | 4 |
| `crates/eink-dither/tests/pin_diagnostics.rs` | New. The λ sweep and far-edge measurements | 5 |

---

### Task 1: The pinning branch in the dither loop

**Files:**
- Modify: `crates/eink-dither/src/dither/options.rs` (add field, add builder method)
- Modify: `crates/eink-dither/src/dither/mod.rs:266-324` (signature + the branch)
- Test: `crates/eink-dither/src/dither/mod.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Palette::official(idx) -> Srgb`, `Palette::actual_linear(idx) -> LinearRgb`,
  `Srgb::to_bytes(self) -> [u8; 3]`, `Srgb::from_u8(r, g, b) -> Srgb`,
  `apply_error(channel: f32, error: f32, max_error: f32) -> f32`.
- Produces:
  - `DitherOptions::pin_carry: f32`, default `0.9`
  - `DitherOptions::pin_carry(mut self, value: f32) -> Self` (builder, clamps to `[0.0, 1.0]`)
  - `dither_with_kernel_noise(image: &[LinearRgb], width: usize, height: usize, palette: &Palette, kernel: &Kernel, options: &DitherOptions, pinned: Option<&[Option<u8>]>) -> Vec<u8>`

**Background the implementer needs:**

`dither_with_kernel_noise` is `pub(crate)`. It is called from exactly one place,
`crates/eink-dither/src/api/builder.rs` inside `EinkDitherer::dither`. Task 2 changes
that call site; for this task, update it to pass `None` so the crate compiles.

`pinned` is indexed by `idx = y * width + x`, the same index as `image`. `Some(i)`
means "this pixel is exactly palette ink `i`, output it and carry λ of the incoming
error". `None` means "ordinary pixel". `pinned: None` (the outer `Option`) means no
pinning anywhere — the existing behaviour, bit for bit.

- [ ] **Step 1: Add the `pin_carry` field to `DitherOptions`**

In `crates/eink-dither/src/dither/options.rs`, add to the `DitherOptions` struct
(after `hybrid_propagation`):

```rust
    /// Fraction of accumulated error a pinned pixel passes on to its neighbours.
    ///
    /// A pinned pixel is one that already sits exactly on a palette ink in a
    /// region the caller marked as eligible. It outputs that ink and ignores the
    /// error diffused into it, so its own quantisation error is zero. This value
    /// decides what happens to the error that arrived:
    ///
    /// - `1.0` — pass it on unchanged. Total error is conserved, so no seam, but
    ///   error can travel the full width of a large pinned region and dump as a
    ///   fringe at its far edge.
    /// - `0.0` — absorb it. Crisp, but a coincidental match mid-gradient drops
    ///   its neighbours' error and leaves a seam across a smooth ramp.
    /// - between — the error decays geometrically with depth into the region.
    ///   At depth `n` the surviving fraction is `pin_carry^n`. A 2 px grid line
    ///   or a text stroke is crossed in one or two steps and passes error through
    ///   nearly intact; a wide flat area absorbs it within a few pixels of its
    ///   edge.
    ///
    /// The count of pinned pixels the error has crossed IS its distance into the
    /// region, measured along the path the error actually travelled. That is why
    /// no distance transform is needed.
    ///
    /// Has no effect unless the caller supplies a pin map.
    ///
    /// Default: `0.9`
    pub pin_carry: f32,
```

Set it in the `Default` impl (`pin_carry: 0.9`) and in any other constructor in that
file that lists fields exhaustively — `grep -n 'hybrid_propagation' options.rs` finds
every one. Add the builder method alongside the existing ones:

```rust
    /// Set the fraction of accumulated error a pinned pixel passes on.
    ///
    /// Clamped to `[0.0, 1.0]`. Values outside that range have no physical
    /// meaning: below zero would invert the error, above one would amplify it
    /// with depth.
    pub fn pin_carry(mut self, value: f32) -> Self {
        self.pin_carry = value.clamp(0.0, 1.0);
        self
    }
```

- [ ] **Step 2: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of
`crates/eink-dither/src/dither/mod.rs`. If the module does not exist, create it.

```rust
    /// Black, white and a saturated red, with `actual` equal to `official` so
    /// the error arithmetic in these tests is exact and readable.
    fn pin_test_palette() -> Palette {
        let inks = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(181, 3, 3),
        ];
        Palette::new(&inks, None).unwrap()
    }

    /// A pinned pixel outputs its own ink no matter how hostile the error
    /// diffused into it is.
    ///
    /// The row is: saturated red, then four pure black pixels. Without pinning
    /// the red's quantisation error plus the black run's own contributions push
    /// at least one of those black pixels off black — that is the reported
    /// defect. With pinning, all four stay black.
    #[test]
    fn a_pinned_pixel_keeps_its_ink_against_hostile_incoming_error() {
        let palette = pin_test_palette();
        // A red that is NOT an exact ink, so it produces real error to diffuse.
        let red = LinearRgb::from(Srgb::from_u8(200, 40, 40));
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let image = vec![red, black, black, black, black];

        let opts = DitherOptions::default().pin_carry(0.9);
        let kernel = DitherAlgorithm::Atkinson.kernel();

        // Pin every pixel that is exactly black (indices 1..=4).
        let pinned: Vec<Option<u8>> = vec![None, Some(0), Some(0), Some(0), Some(0)];

        let out = dither_with_kernel_noise(
            &image, 5, 1, &palette, kernel, &opts, Some(&pinned),
        );

        assert_eq!(
            &out[1..], &[0, 0, 0, 0],
            "a pinned black pixel was taken over by diffused error: {out:?}"
        );
    }

    /// `pin_carry` governs what the pinned pixel hands on. At 0.0 the error is
    /// absorbed, so a long pinned run leaves the pixel after it untouched; at
    /// 1.0 the error survives the run and still reaches it.
    ///
    /// The probe is the FIRST unpinned pixel after the run: a mid-grey that sits
    /// between two inks, so a small nudge flips which ink it picks.
    #[test]
    fn pin_carry_decides_whether_error_survives_a_pinned_run() {
        let palette = pin_test_palette();
        let bright = LinearRgb::from(Srgb::from_u8(255, 250, 250));
        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let probe = LinearRgb::from(Srgb::from_u8(128, 128, 128));

        // bright, then two pinned blacks, then the probe.
        let image = vec![bright, black, black, probe];
        let pinned: Vec<Option<u8>> = vec![None, Some(0), Some(0), None];
        let kernel = DitherAlgorithm::Atkinson.kernel();

        let run = |carry: f32| {
            let opts = DitherOptions::default()
                .serpentine(false)
                .noise_scale(0.0)
                .pin_carry(carry);
            dither_with_kernel_noise(&image, 4, 1, &palette, kernel, &opts, Some(&pinned))
        };

        let absorbed = run(0.0);
        let conserved = run(1.0);

        // Whatever each picks, the point is that the carry changed the outcome:
        // at 0.0 the bright pixel's error never reaches the probe.
        assert_ne!(
            absorbed[3], conserved[3],
            "pin_carry made no difference to the pixel after a pinned run \
             (absorbed={absorbed:?}, conserved={conserved:?}) — the carry is \
             not reaching the kernel"
        );
        // Both pinned pixels stay black regardless of carry.
        assert_eq!(&absorbed[1..3], &[0, 0]);
        assert_eq!(&conserved[1..3], &[0, 0]);
    }

    /// `pinned: None` must reproduce the pre-pinning output exactly. This is the
    /// guard that keeps every other eink-dither test meaningful.
    #[test]
    fn no_pin_map_reproduces_the_unpinned_output_exactly() {
        let palette = pin_test_palette();
        let image: Vec<LinearRgb> = (0..64)
            .map(|i| LinearRgb::from(Srgb::from_u8(i as u8 * 4, 128, 255 - i as u8 * 4)))
            .collect();
        let opts = DitherOptions::default();
        let kernel = DitherAlgorithm::Atkinson.kernel();

        let without = dither_with_kernel_noise(&image, 8, 8, &palette, kernel, &opts, None);
        let all_unpinned: Vec<Option<u8>> = vec![None; 64];
        let with_empty_map =
            dither_with_kernel_noise(&image, 8, 8, &palette, kernel, &opts, Some(&all_unpinned));

        assert_eq!(
            without, with_empty_map,
            "an all-None pin map changed the output; the pinning branch is \
             firing when it must not"
        );
    }
```

Make sure the test module's `use super::*;` brings in `Palette`, `Srgb`,
`LinearRgb`, `DitherAlgorithm` and `DitherOptions`. Add explicit `use` lines if not.

- [ ] **Step 3: Run the tests to verify they fail**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib dither::tests -- --nocapture
```

Expected: compile error — `dither_with_kernel_noise` takes 6 arguments, 7 supplied;
and `pin_carry` not found. That is the correct failure at this point.

- [ ] **Step 4: Add the parameter and the branch**

In `crates/eink-dither/src/dither/mod.rs`, extend the signature:

```rust
pub(crate) fn dither_with_kernel_noise(
    image: &[LinearRgb],
    width: usize,
    height: usize,
    palette: &Palette,
    kernel: &Kernel,
    options: &DitherOptions,
    pinned: Option<&[Option<u8>]>,
) -> Vec<u8> {
```

and add to its doc comment:

```rust
/// `pinned`, when supplied, is one entry per pixel in the same layout as
/// `image`. `Some(i)` marks a pixel that already sits exactly on ink `i` in a
/// region the caller allows pinning in: it outputs that ink, ignores the error
/// diffused into it, and hands `options.pin_carry` of that error on to its
/// neighbours in place of its own (zero) quantisation error. `None` for the
/// whole slice, or `None` for the outer option, is the unpinned behaviour.
```

Replace the block at the old lines 318-361 (from `// Add accumulated error to input
pixel` through the `strength_error` binding) with:

```rust
            // Add accumulated error to input pixel
            let accumulated = error_buf.get_accumulated(x);

            // A pinned pixel already IS a palette ink. It outputs that ink and
            // ignores the error diffused into it — its own quantisation error is
            // zero. The comment this replaces claimed error diffusion reproduced
            // such a pixel exactly "without a special case"; that is true of the
            // pixel's own error and ignores the error arriving from neighbours,
            // which is what speckles black grid lines and text next to saturated
            // content.
            let pin = pinned.and_then(|p| p[idx]);

            let strength_error = if let Some(ink) = pin {
                output[idx] = ink;
                // Carry the incoming error onward, attenuated. See
                // DitherOptions::pin_carry for why the decay is per pinned pixel.
                [
                    accumulated[0] * options.pin_carry,
                    accumulated[1] * options.pin_carry,
                    accumulated[2] * options.pin_carry,
                ]
            } else {
                let pixel = LinearRgb::new(
                    apply_error(image[idx].r, accumulated[0], options.error_clamp),
                    apply_error(image[idx].g, accumulated[1], options.error_clamp),
                    apply_error(image[idx].b, accumulated[2], options.error_clamp),
                );

                // Chroma of original pixel (for chromatic damping)
                let original_oklab = Oklab::from(image[idx]);
                let original_chroma_sq =
                    original_oklab.a * original_oklab.a + original_oklab.b * original_oklab.b;

                let oklab = Oklab::from(pixel);
                let (nearest_idx, _dist) = palette.find_nearest(oklab);
                output[idx] = nearest_idx as u8;

                let nearest_linear = palette.actual_linear(nearest_idx);
                let error = [
                    pixel.r - nearest_linear.r,
                    pixel.g - nearest_linear.g,
                    pixel.b - nearest_linear.b,
                ];

                // Chromatic error damping
                let damped_error = if options.chroma_clamp < f32::INFINITY {
                    let ratio_sq = (original_chroma_sq / threshold_sq).min(1.0);
                    let alpha = ratio_sq * ratio_sq;
                    let err_mean = (error[0] + error[1] + error[2]) * (1.0 / 3.0);
                    [
                        err_mean + alpha * (error[0] - err_mean),
                        err_mean + alpha * (error[1] - err_mean),
                        err_mean + alpha * (error[2] - err_mean),
                    ]
                } else {
                    error
                };

                // Apply strength scaling
                [
                    damped_error[0] * options.strength,
                    damped_error[1] * options.strength,
                    damped_error[2] * options.strength,
                ]
            };
```

Leave the kernel diffusion loop that follows completely unchanged — it already
consumes `strength_error`.

Then fix the one call site so the crate compiles. In
`crates/eink-dither/src/api/builder.rs`, in `EinkDitherer::dither`, add `None` as the
final argument to `dither_with_kernel_noise`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib -- --nocapture
```

Expected: PASS, including the whole existing eink-dither lib suite. If any
pre-existing test changed its result, stop — `pinned: None` is leaking.

- [ ] **Step 6: Mutation-verify the guards, both directions**

Each mutant is applied, the named test run, the mutant reverted. **Record the actual
observed output in the commit message.** A guard that passes against its mutant is
worthless and must be strengthened before moving on.

| Mutant | Must fail |
|---|---|
| Delete the `if let Some(ink) = pin` branch (always take the `else`) | `a_pinned_pixel_keeps_its_ink_against_hostile_incoming_error` |
| Replace the carry with `[0.0; 3]` (always absorb) | `pin_carry_decides_whether_error_survives_a_pinned_run` |
| Replace the carry with `accumulated` (ignore `pin_carry`) | `pin_carry_decides_whether_error_survives_a_pinned_run` |
| Make `pin` ignore its argument and return `Some(0)` | `no_pin_map_reproduces_the_unpinned_output_exactly` |

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib dither::tests -- --nocapture
```

- [ ] **Step 7: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
git add crates/eink-dither/src/dither/options.rs crates/eink-dither/src/dither/mod.rs crates/eink-dither/src/api/builder.rs
git diff --cached --stat
git commit -m "feat(eink-dither): pin exact palette matches, carry error onward decayed"
```

---

### Task 2: Resolve exact matches before preprocessing

**Files:**
- Modify: `crates/eink-dither/src/api/builder.rs` (new public method, match resolution)
- Modify: `crates/eink-dither/src/preprocess/preprocessor.rs:88-94` (correct the comment)
- Test: `crates/eink-dither/src/api/builder.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: everything Task 1 produced;
  `Preprocessor::process(&self, input: &[Srgb], width: usize, height: usize) -> PreprocessResult`;
  `PreprocessOptions { target_width: Option<u32>, target_height: Option<u32>, saturation: f32, contrast: f32 }`.
- Produces:
  - `EinkDitherer::dither_with_pinning(&self, pixels: &[Srgb], width: usize, height: usize, pin_eligible: Option<&[bool]>) -> DitheredImage`
  - `EinkDitherer::dither(&self, pixels: &[Srgb], width: usize, height: usize) -> DitheredImage` — unchanged signature, now delegates with `None`
  - `EinkDitherer::pin_carry(mut self, value: f32) -> Self` — builder passthrough to
    `DitherOptions::pin_carry`, needed by Task 5's sweep

`EinkDitherer`'s fields are `palette`, `preprocess`, `dither_opts`, `algorithm` and
`error_clamp_explicit` (`builder.rs:44-50`). `DitheredImage::indices(&self) -> &[u8]`
(`output/dithered_image.rs:90`). Both verified — use them as written.

**Background the implementer needs:**

The match is resolved on the caller's `Srgb` bytes, **before** `Preprocessor::process`
runs. Matching after preprocessing would mean matching a saturation- and
contrast-adjusted value, which moves a pure ink off its palette entry and makes the
whole feature a silent no-op. `Palette::official(i)` is the nominal colour, which is
what an SVG author writes; `Palette::actual(i)` is the measured ink and is **not**
what to match against.

Pinning and resize are incompatible: resampling destroys exact matches and breaks
index correspondence. When `target_width`/`target_height` are set, refuse to pin.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in
`crates/eink-dither/src/api/builder.rs`. It already has a `test_palette()` helper
whose official red is `(255, 0, 0)` and whose actual red is `(200, 50, 50)` — that
difference is exactly what the nominal-vs-actual test needs.

```rust
    /// Eligibility gates pinning. The same pure-black pixel is pinned where the
    /// caller allows it and dithered normally where it does not.
    #[test]
    fn eligibility_decides_where_pinning_applies() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).noise_scale(0.0).serpentine(false);

        // A vivid pixel that must diffuse error, followed by pure blacks.
        let px = vec![
            Srgb::from_u8(250, 10, 10),
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(0, 0, 0),
        ];

        let eligible = vec![true, true, true, true];
        let ineligible = vec![false, false, false, false];

        let pinned = ditherer.dither_with_pinning(&px, 4, 1, Some(&eligible));
        let unpinned = ditherer.dither_with_pinning(&px, 4, 1, Some(&ineligible));

        assert!(
            pinned.indices()[1..].iter().all(|&i| i == 0),
            "pinning was allowed but a black pixel was not held: {:?}",
            pinned.indices()
        );
        assert_ne!(
            pinned.indices(), unpinned.indices(),
            "eligibility made no difference — the mask is not reaching the loop"
        );
    }

    /// The match is against the NOMINAL palette entry, not the measured ink.
    /// test_palette()'s red is official (255,0,0) / actual (200,50,50).
    #[test]
    fn the_exact_match_is_against_the_nominal_entry() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).noise_scale(0.0).serpentine(false);
        let eligible = vec![true; 2];

        let nominal = vec![Srgb::from_u8(255, 0, 0), Srgb::from_u8(255, 0, 0)];
        let out = ditherer.dither_with_pinning(&nominal, 2, 1, Some(&eligible));
        assert!(
            out.indices().iter().all(|&i| i == 2),
            "an author-written nominal red was not recognised as ink 2: {:?}",
            out.indices()
        );
    }

    /// A pixel that is not exactly an ink is never pinned, however eligible.
    #[test]
    fn a_near_miss_is_not_pinned() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).noise_scale(0.0).serpentine(false);
        let eligible = vec![true; 4];

        // One byte off black in one channel.
        let px = vec![Srgb::from_u8(1, 0, 0); 4];
        let with = ditherer.dither_with_pinning(&px, 4, 1, Some(&eligible));
        let without = ditherer.dither_with_pinning(&px, 4, 1, None);
        assert_eq!(
            with.indices(), without.indices(),
            "a near-miss pixel was pinned; the match is not exact"
        );
    }

    /// dither() is dither_with_pinning(None) and neither pins.
    #[test]
    fn plain_dither_is_unchanged_by_this_feature() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette);
        let px: Vec<Srgb> = (0..64).map(|i| Srgb::from_u8(i * 4, 128, 255 - i * 4)).collect();

        let a = ditherer.dither(&px, 8, 8);
        let b = ditherer.dither_with_pinning(&px, 8, 8, None);
        assert_eq!(a.indices(), b.indices());
    }

    /// Resize destroys exact matches and index correspondence, so pinning is
    /// refused rather than silently misaligned.
    #[test]
    fn pinning_is_refused_when_resizing() {
        let palette = test_palette();
        let ditherer = EinkDitherer::new(palette).resize(2, 2);
        let px = vec![Srgb::from_u8(0, 0, 0); 16];
        let eligible = vec![true; 16];

        let with = ditherer.dither_with_pinning(&px, 4, 4, Some(&eligible));
        let without = ditherer.dither_with_pinning(&px, 4, 4, None);
        assert_eq!(
            with.indices(), without.indices(),
            "pinning was applied across a resize"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib api::builder -- --nocapture
```

Expected: compile error, `no method named dither_with_pinning`.

- [ ] **Step 3: Implement `dither_with_pinning`**

In `crates/eink-dither/src/api/builder.rs`, replace the body of `dither` and add the
new method:

```rust
    /// Dither, holding pixels that already sit exactly on a palette ink.
    ///
    /// `pin_eligible`, when supplied, is one `bool` per input pixel: `true`
    /// where the caller permits pinning. A pixel is pinned when it is eligible
    /// AND its bytes equal a nominal palette entry exactly. Such a pixel renders
    /// as that ink and hands `DitherOptions::pin_carry` of the error diffused
    /// into it on to its neighbours.
    ///
    /// `None` means no pinning at all — identical output to [`Self::dither`]. A
    /// caller wanting frame-wide pinning passes an all-`true` slice.
    ///
    /// The match is resolved on these `Srgb` bytes, BEFORE preprocessing:
    /// saturation or contrast at anything but identity would move a pure ink off
    /// its palette entry and the match would silently never fire. A pinned pixel
    /// is therefore not enhanced — it renders the colour the author wrote, which
    /// is the right answer for the structural content pinning exists for.
    ///
    /// Pinning is refused when a resize is configured: resampling destroys exact
    /// matches and breaks the index correspondence between `pixels` and the
    /// preprocessed frame.
    pub fn dither_with_pinning(
        &self,
        pixels: &[Srgb],
        width: usize,
        height: usize,
        pin_eligible: Option<&[bool]>,
    ) -> DitheredImage {
        let resizing =
            self.preprocess.target_width.is_some() || self.preprocess.target_height.is_some();

        let pin_map: Option<Vec<Option<u8>>> = match pin_eligible {
            Some(mask) if !resizing && mask.len() == pixels.len() => {
                let inks: Vec<[u8; 3]> = (0..self.palette.len())
                    .map(|i| self.palette.official(i).to_bytes())
                    .collect();
                Some(
                    pixels
                        .iter()
                        .zip(mask.iter())
                        .map(|(px, &ok)| {
                            if !ok {
                                return None;
                            }
                            let bytes = px.to_bytes();
                            inks.iter().position(|ink| *ink == bytes).map(|i| i as u8)
                        })
                        .collect(),
                )
            }
            _ => None,
        };

        let preprocessor = Preprocessor::new(self.preprocess.clone());
        let result = preprocessor.process(pixels, width, height);

        let dither_opts = self.dither_opts.clone();
        let photo_palette = self.palette.for_error_diffusion();
        let kernel = self.algorithm.kernel();
        let indices = dither_with_kernel_noise(
            &result.pixels,
            result.width,
            result.height,
            &photo_palette,
            kernel,
            &dither_opts,
            pin_map.as_deref(),
        );

        DitheredImage::new(indices, result.width, result.height, self.palette.clone())
    }
```

and reduce `dither` to a delegation, keeping its existing doc comment above it:

```rust
    pub fn dither(&self, pixels: &[Srgb], width: usize, height: usize) -> DitheredImage {
        self.dither_with_pinning(pixels, width, height, None)
    }
```

Keep the long "There used to be a greyscale override..." comment that sits above
`let dither_opts` — move it with the code, do not delete it.

Add the builder passthrough alongside the other `EinkDitherer` builder methods, so
Task 5's sweep can set λ without reaching into `DitherOptions`:

```rust
    /// Set the fraction of accumulated error a pinned pixel passes on.
    ///
    /// See [`DitherOptions::pin_carry`]. Has no effect without a pin map.
    pub fn pin_carry(mut self, value: f32) -> Self {
        self.dither_opts = self.dither_opts.pin_carry(value);
        self
    }
```

- [ ] **Step 4: Correct the wrong comment in the preprocessor**

`crates/eink-dither/src/preprocess/preprocessor.rs:88-94` currently claims an
exact-match pixel needs no special case. Replace that paragraph with:

```rust
/// Enhancement is applied uniformly to every pixel. Pixels that already sit
/// exactly on a palette colour used to be detected here and passed through
/// untouched, to keep text and logos crisp. That detection is gone from this
/// stage, but the concern was real and the reason first given for dropping it
/// was wrong: such a pixel does have zero quantisation error of its own, but
/// error diffused INTO it from saturated neighbours still takes it over. In the
/// tone calibration screen, pure-black grid lines abutting saturated patches
/// came back only 73.2% black.
///
/// The half of that reasoning which does hold is the seam: pinning also caught
/// pixels whose value merely happened to coincide with a palette entry
/// mid-gradient, and discarding their error left a seam across a smooth ramp.
///
/// Both are addressed where the error actually moves, not here — see
/// [`crate::api::EinkDitherer::dither_with_pinning`] and
/// [`crate::dither::DitherOptions::pin_carry`], which hold the exact-match pixel
/// AND carry its incoming error onward rather than dropping it.
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib -- --nocapture
```

Expected: PASS, whole lib suite.

- [ ] **Step 6: Mutation-verify, both directions**

| Mutant | Must fail |
|---|---|
| Match against `self.palette.actual(i)` instead of `official(i)` | `the_exact_match_is_against_the_nominal_entry` |
| Ignore the mask: `.map(\|px\| ...)` without the `if !ok` guard | `eligibility_decides_where_pinning_applies` |
| Drop the `!resizing` condition | `pinning_is_refused_when_resizing` |
| Make `dither` build an all-true mask instead of passing `None` | `plain_dither_is_unchanged_by_this_feature` |
| Compare with a tolerance instead of `==` | `a_near_miss_is_not_pinned` |

- [ ] **Step 7: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
git add crates/eink-dither/src/api/builder.rs crates/eink-dither/src/preprocess/preprocessor.rs
git diff --cached --stat
git commit -m "feat(eink-dither): resolve pin matches pre-preprocess; fix wrong exact-match comment"
```

---

### Task 3: byonk supplies the eligibility mask

**Files:**
- Modify: `src/rendering/svg_to_png.rs:128-180`
- Test: `src/rendering/svg_to_png.rs` (in-file `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `EinkDitherer::dither_with_pinning` from Task 2;
  `crate::rendering::tone_mask::has_tone_markup(svg: &[u8]) -> bool`;
  `SvgRenderer::rasterize_tone_mask(&self, svg_data: &[u8], spec: DisplaySpec) -> Result<Vec<bool>, RenderError>`.
- Produces: no new public API. `render_to_palette_png` keeps its signature.

**Background the implementer needs:**

Today the tone mask is rasterized only when the document has markup **and**
`gamut.amount != 0.0` (`svg_to_png.rs:134-137`), and it is consumed by the gamut
mapper and dropped. Pinning needs the same mask for a different reason, so the
`amount != 0.0` gate has to move inside — the mask is rasterized whenever the
document carries markup, and only the *mapping* is skipped when amount is zero.

The eligibility mask is the **inverse** of the tone mask. On a document with no tone
markup at all there is no mask to invert and every pixel is eligible — per the owner
ruling of 2026-08-10, pinning applies in every document, not only marked ones.

The existing length-mismatch hard error stays exactly as it is.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `src/rendering/svg_to_png.rs`.

```rust
    /// A pure-black bar next to a saturated bar keeps its black, in a document
    /// with no tone markup at all. This is the reported defect, reduced to its
    /// smallest form: without pinning, error diffused out of the saturated bar
    /// speckles the black one.
    #[test]
    fn pure_ink_survives_beside_saturated_content() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="8">
  <rect x="0" y="0" width="20" height="8" fill="#C06020"/>
  <rect x="20" y="0" width="20" height="8" fill="#000000"/>
</svg>"#;
        let spec = DisplaySpec { width: 40, height: 8, ..Default::default() };
        let palette: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (181, 3, 3),
            (13, 135, 107),
            (32, 84, 151),
        ];

        let renderer = SvgRenderer::new();
        let png = renderer
            .render_to_palette_png(svg, spec, &palette, None, false, None, None)
            .expect("render failed");

        // Decode and count how many pixels in the right half are exactly black.
        let img = image::load_from_memory(&png).expect("decode failed").to_rgb8();
        let mut black = 0usize;
        let mut total = 0usize;
        for y in 0..8u32 {
            for x in 22..38u32 {
                total += 1;
                let p = img.get_pixel(x, y);
                if p.0 == [0, 0, 0] {
                    black += 1;
                }
            }
        }
        let share = black as f64 / total as f64;
        assert!(
            share > 0.99,
            "only {:.1}% of the pure-black bar stayed black — error is being \
             diffused into pinned pixels",
            share * 100.0
        );
    }
```

`DisplaySpec`'s real field set may differ; `grep -n 'struct DisplaySpec' -A 15
src/models/*.rs src/rendering/*.rs` and construct it the way neighbouring tests in
this file do. Likewise copy the palette-argument shape from an existing test in the
same module rather than inventing it.

- [ ] **Step 2: Run the test to verify it fails**

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib rendering::svg_to_png -- --nocapture
```

Expected: FAIL, with a share well under 99% — record the observed number, it is the
before-measurement for this task.

- [ ] **Step 3: Restructure the mask block and call `dither_with_pinning`**

Replace `src/rendering/svg_to_png.rs:131-159` (the whole `if has_tone_markup` block)
with:

```rust
        // The tone mask has two consumers: gamut mapping acts INSIDE marked
        // regions, exact-match pinning acts OUTSIDE them. So it is rasterized
        // whenever the document carries markup, and only the mapping is skipped
        // when amount is zero. An unmarked document skips the second
        // rasterization entirely and every pixel is eligible for pinning.
        let tone_mask: Option<Vec<bool>> =
            if crate::rendering::tone_mask::has_tone_markup(svg_data) {
                let mask = self.rasterize_tone_mask(svg_data, spec)?;
                if mask.len() != pixels.len() {
                    // Cannot happen: both rasterize to `spec`. Loud rather
                    // than silently skipped.
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

        // Pinning is eligible wherever the document does NOT mark the content as
        // continuous-tone — structure, not gradients. Owner ruling 2026-08-10:
        // this applies in every document, including one with no markup, where
        // the whole frame is eligible.
        let pin_eligible: Vec<bool> = match tone_mask.as_ref() {
            Some(mask) => mask.iter().map(|m| !m).collect(),
            None => vec![true; pixels.len()],
        };
```

Then change the dither call (currently `svg_to_png.rs:180`) to:

```rust
        let result = ditherer.dither_with_pinning(
            &pixels,
            spec.width as usize,
            spec.height as usize,
            Some(&pin_eligible),
        );
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib rendering:: -- --nocapture
```

Expected: PASS at >99%. Record the new share.

- [ ] **Step 5: Mutation-verify, both directions**

| Mutant | Must fail |
|---|---|
| `None => vec![false; pixels.len()]` (unmarked documents not eligible) | `pure_ink_survives_beside_saturated_content` |
| `Some(mask) => mask.clone()` (forget the inverse) | Task 4's tone-screen measurement — note it here and confirm in Task 4 |

- [ ] **Step 6: Run the wider byonk suite**

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib -- --nocapture
```

Expected: PASS. Any render-snapshot test that now differs is a **real** finding —
report the diff rather than updating the expectation.

- [ ] **Step 7: Format and commit**

```bash
cargo fmt
CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets
git add src/rendering/svg_to_png.rs
git diff --cached --stat
git commit -m "feat(render): pin pure panel inks outside continuous-tone regions"
```

---

### Task 4: Mark the tone screen by content, not by column

**Files:**
- Modify: `screens/builtin/calibration/tone/screen.svg:45` region
- Modify: `src/services/screen_store.rs:2466-2482` (the recorded mask fraction)
- Test: existing `src/services/screen_store.rs` mask-geometry test, plus the
  integration render below

**Interfaces:**
- Consumes: Task 3's eligibility plumbing.
- Produces: nothing new. This is an authoring change plus a re-measurement.

**Background the implementer needs:**

The screen currently marks by layout region: `<g data-byonk-tone="continuous">` wraps
the whole right column, including the black rect that backs the patch grid. The grid
is not drawn as lines — it is that rect showing through the 2 px gaps between the
patches drawn over it. Inside the group it is ineligible for pinning, so the marked
column would keep a speckled grid while the unmarked column got a crisp one.

The principle, which is what the spec establishes: **the mask marks content that is
continuous-tone, not regions of the layout.** Structure stays unmarked wherever it
sits.

Pure black is in gamut, so moving the rect out cannot change the mapped patches. It
does remove those pixels from the **adaptation group**, and `R` is a 99th percentile
over the marked set. Small pixel count, but a percentile is exactly what moves when
the set changes. Measure it.

- [ ] **Step 1: Record the before-state**

```bash
cp config.yaml /tmp/pin-check.yaml
```

Add a throwaway device to `/tmp/pin-check.yaml` under the devices map. The `panel:`
key is **mandatory** — without it the render is silently greyscale:

```yaml
  "AA:BB:CC:DD:EE:01":
    panel: reterminal_e1002
    screen: byonk-builtin/calibration/tone
```

```bash
CONFIG_FILE=/tmp/pin-check.yaml RUST_LOG=eink_dither=debug,byonk=debug \
  CARGO_BUILD_JOBS=2 cargo run -- render --mac AA:BB:CC:DD:EE:01 \
  --output /tmp/tone-before.png 2>/tmp/tone-before.log
grep -i 'adaptation\|marked_pixels\|percentile' /tmp/tone-before.log
```

Record `marked_pixels` and any logged `R` / adaptation factor. If `R` is not logged,
add a `tracing::debug!` for it in `crates/eink-dither/src/gamut/adapt.rs` where the
factor is computed, keep it, and note it in the commit — a value this plan asks to be
compared must be observable.

- [ ] **Step 2: Move the backing rect out of the marked group**

In `screens/builtin/calibration/tone/screen.svg`, move the
`<rect ... fill="#000000"/>` line that precedes the right-column patch loop to
*immediately before* the `<g data-byonk-tone="continuous">` opening tag, so document
order — and therefore z-order — is unchanged. Replace the group's comment block with:

```xml
  <!-- RIGHT COLUMN — the continuous-tone content, marked.

       The mask marks CONTENT THAT IS CONTINUOUS-TONE, not a region of the
       layout. Structure stays outside it wherever it sits: the header text
       above, and the black rect below that backs the patch grid. Both have two
       reasons to stay out — the mapper buys nothing on glyph edges or on pure
       black, and exact-match pinning only holds pixels the mask leaves
       unmarked, so a grid inside this group would come back speckled on this
       column while the control column's came back crisp.

       Pure black is in gamut, so moving the backing rect out cannot change the
       mapped patches. It does remove those pixels from the adaptation group,
       over which R is a 99th percentile — measured, not assumed.

       One group, not three, because the mask is frame-level: there is exactly
       one adaptation group, so R is derived from all of these pixels together.
       `data-byonk-tone-group` exists as an attribute but is not implemented,
       and this screen must not become the reason it gets built. -->
```

- [ ] **Step 3: Re-render and measure both columns**

```bash
CONFIG_FILE=/tmp/pin-check.yaml RUST_LOG=eink_dither=debug,byonk=debug \
  CARGO_BUILD_JOBS=2 cargo run -- render --mac AA:BB:CC:DD:EE:01 \
  --output /tmp/tone-after.png 2>/tmp/tone-after.log
grep -i 'adaptation\|marked_pixels\|percentile' /tmp/tone-after.log
```

Report, as a table in the commit message:

| | before | after |
|---|---|---|
| grid black share, unmarked column | 73.2% | ? |
| grid black share, marked column | 71.4% | ? |
| `marked_pixels` | ? | ? |
| adaptation `R` | ? | ? |
| mapped patch colours (sample 3) | ? | ? |

Both grid shares should approach 100%. `R` and the mapped patch colours should be
unchanged or negligibly changed — **if they are not, stop and report**; that is a
finding, not a nuisance.

To count the grid share, first **read `screens/builtin/calibration/tone/script.lua`
and write down the patch grid geometry for each column** — the patch band's origin,
the patch pitch, and the gap width. The script computes `data.left.patches` and
`data.right.patches` as `{x, y, width, height}`; the gap pixels are the columns and
rows *between* consecutive patches. Record those coordinate ranges in the commit
message so the next session can re-run the measurement without re-deriving them.

Then count with a throwaway script over the two PNGs. `oxipng`/`image` are already
workspace dependencies, so a small Rust bin is easiest, but any language is fine —
this is a measurement, not shipped code, and it is not committed:

```
for each column in {left, right}:
    for each gap pixel (x, y) from the geometry above:
        histogram img.get_pixel(x, y)
    report share of #000000, and the top three non-black inks
```

Report the black share **and the non-black breakdown** — the original finding was
10.7% red / 8.6% blue / 7.6% green, and which inks intrude is what tells you whether
a residue is diffused chroma or something else.

- [ ] **Step 4: Update the recorded mask fraction**

`src/services/screen_store.rs:2466` documents the measured fraction as 0.4605. Moving
the rect out reduces the marked area by the visible gap pixels. Run the test:

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --lib screen_store -- --nocapture
```

The assertion band is `0.35..=0.55` and deliberately wide, so it should still pass.
**Update the comment's stated value to the newly measured fraction** and add a
sentence recording that the backing rect is now outside the group. Do not widen the
band; do not loosen the assertion.

- [ ] **Step 5: Run the inventory guards**

Adding or altering a builtin screen has a fan-out. These two hardcode the shipped
inventory as an exact count and are strict on purpose:

```bash
CARGO_BUILD_JOBS=2 cargo test -p byonk --test builtin_package -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p byonk --test screen_schemas_test -- --nocapture
```

This task adds no screen, so both should pass untouched. If either fails, that is a
real finding — report it rather than adjusting the count.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add screens/builtin/calibration/tone/screen.svg src/services/screen_store.rs
git diff --cached --stat
git commit -m "fix(tone): mark by content type, not by column, so the grid pins"
```

Include the measurement table from Step 3 in the commit body.

---

### Task 5: Measure — λ sweep, far-edge dump, the photograph, text, cost

**Files:**
- Create: `crates/eink-dither/tests/pin_diagnostics.rs`
- Test: that file

**Interfaces:**
- Consumes: the full public surface from Tasks 1–3.
- Produces: measured numbers, and a recommended `pin_carry` default.

**Background the implementer needs:**

These are `#[ignore]` diagnostics, in the pattern of
`crates/eink-dither/tests/gamut_adaptation_diag.rs`. `make check` does not run
`#[ignore]` tests; they are run explicitly and their output is the deliverable.

Integration tests cannot see the crate's `#[cfg(test)]` fixtures, so build the
palette locally from hex. The measured panel inks are: black `#000000`, white
`#FFFFFF`, red `#B50303`, green `#0D876B`, blue `#205497`, yellow `#D8C40E`.

**Read all measurements in linear light.** Every visual dither comparison in this tree
reads about 30% too dark, because an image viewer downscaling a PNG without
linearising averages sRGB bytes directly. Relative comparisons between λ values stay
valid; absolute judgements need linear-light means.

Do not use whole-image means. Measure the pixels the change touches.

- [ ] **Step 1: Write the far-edge and λ-sweep diagnostic**

Create `crates/eink-dither/tests/pin_diagnostics.rs`:

```rust
//! Diagnostics for exact-match pinning. `#[ignore]` by design — these print
//! measurements rather than asserting thresholds, and are the evidence behind
//! the chosen `pin_carry` default.
//!
//! Run: cargo test -p eink-dither --test pin_diagnostics -- --ignored --nocapture

use eink_dither::{DitherAlgorithm, EinkDitherer, Palette, Srgb};

fn panel() -> Palette {
    Palette::from_hex(
        &["#000000", "#FFFFFF", "#B50303", "#0D876B", "#205497", "#D8C40E"],
        None,
    )
    .expect("palette")
}

/// A wide pure-black bar abutting saturated content. If carried error survives
/// the crossing it dumps as a fringe at the far edge, which is what pin_carry
/// exists to prevent. Prints black share in the first 10 px, the middle, and the
/// last 10 px of the bar, for each carry value.
#[test]
#[ignore]
fn far_edge_dump_across_a_wide_pinned_bar() {
    let width = 320usize;
    let height = 16usize;
    let bar_start = 20usize;

    // Saturated orange on the left, pure black bar for the rest.
    let mut px = vec![Srgb::from_u8(0, 0, 0); width * height];
    for y in 0..height {
        for x in 0..bar_start {
            px[y * width + x] = Srgb::from_u8(192, 96, 32);
        }
    }
    let eligible = vec![true; width * height];

    println!("carry |  first10 |   middle |   last10");
    for carry in [0.0f32, 0.5, 0.8, 0.9, 0.95, 1.0] {
        let d = EinkDitherer::new(panel())
            .algorithm(DitherAlgorithm::Atkinson)
            .pin_carry(carry);
        let out = d.dither_with_pinning(&px, width, height, Some(&eligible));
        let idx = out.indices();

        let share = |x0: usize, x1: usize| {
            let mut black = 0usize;
            let mut total = 0usize;
            for y in 0..height {
                for x in x0..x1 {
                    total += 1;
                    if idx[y * width + x] == 0 {
                        black += 1;
                    }
                }
            }
            black as f64 / total as f64
        };

        println!(
            " {carry:.2} |  {:.4}  |  {:.4}  |  {:.4}",
            share(bar_start, bar_start + 10),
            share(width / 2 - 5, width / 2 + 5),
            share(width - 10, width),
        );
    }
}

/// A 2 px pinned line between two saturated patches — the reported defect's
/// geometry. Prints the line's black share against carry, and the total error
/// leaving the frame, so absorption (a seam) is visible as a drop in the latter.
#[test]
#[ignore]
fn thin_line_between_saturated_patches() {
    let width = 64usize;
    let height = 32usize;
    let mut px = vec![Srgb::from_u8(192, 96, 32); width * height];
    for y in 0..height {
        for x in 31..33 {
            px[y * width + x] = Srgb::from_u8(0, 0, 0);
        }
    }
    let eligible: Vec<bool> = px.iter().map(|p| p.to_bytes() == [0, 0, 0]).collect();

    println!("carry | line black share");
    for carry in [0.0f32, 0.5, 0.8, 0.9, 0.95, 1.0] {
        let d = EinkDitherer::new(panel())
            .algorithm(DitherAlgorithm::Atkinson)
            .pin_carry(carry);
        let out = d.dither_with_pinning(&px, width, height, Some(&eligible));
        let idx = out.indices();
        let mut black = 0usize;
        for y in 0..height {
            for x in 31..33 {
                if idx[y * width + x] == 0 {
                    black += 1;
                }
            }
        }
        println!(" {carry:.2} | {:.4}", black as f64 / (height * 2) as f64);
    }
}

/// Cost. Expected negligible — one slice lookup per pixel — but stated.
#[test]
#[ignore]
fn pinning_cost_on_a_panel_sized_frame() {
    let (w, h) = (800usize, 480usize);
    let px: Vec<Srgb> = (0..w * h)
        .map(|i| Srgb::from_u8((i % 256) as u8, ((i / 7) % 256) as u8, ((i / 13) % 256) as u8))
        .collect();
    let eligible = vec![true; w * h];
    let d = EinkDitherer::new(panel()).algorithm(DitherAlgorithm::Atkinson);

    let t0 = std::time::Instant::now();
    let _ = d.dither(&px, w, h);
    let plain = t0.elapsed();

    let t1 = std::time::Instant::now();
    let _ = d.dither_with_pinning(&px, w, h, Some(&eligible));
    let pinning = t1.elapsed();

    println!("plain {plain:?}, pinning {pinning:?}");
}
```

- [ ] **Step 2: Run the diagnostics and record the output**

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --test pin_diagnostics -- --ignored --nocapture
CARGO_BUILD_JOBS=2 cargo test --release -p eink-dither --test pin_diagnostics \
  pinning_cost -- --ignored --nocapture
```

Paste both tables verbatim into the commit message. Do not summarise them away.

- [ ] **Step 3: Measure the photograph**

`screens/builtin/calibration/color/photo.png` is now eligible for pinning frame-wide.
Count how many of its pixels are exact matches for a panel ink at all:

```bash
CONFIG_FILE=/tmp/pin-check.yaml CARGO_BUILD_JOBS=2 cargo run -- render \
  --mac AA:BB:CC:DD:EE:01 --output /tmp/color-after.png
```

(point that device's `screen:` at `byonk-builtin/calibration/color` first).

Count the exact-match share of the **source** photo directly — it needs no render:

```
load screens/builtin/calibration/color/photo.png as RGB8
inks = {000000, FFFFFF, B50303, 0D876B, 205497, D8C40E}
count pixels whose bytes are exactly in inks
report count, total, share, and the breakdown per ink
```

**If the share is under ~0.5%, say so and stop.** The seam question is moot — there
is almost nothing to pin — and no further measurement is needed. Report the number
either way; a negligible result is a real answer, not a skipped step.

If it is material, render the screen with `pin_carry` at 0.0 and at 0.9, and compare
8×8 block means **in linear light**, restricted to blocks that contain at least one
pinned pixel. Never a whole-image mean: on this photograph only 7% of pixels are even
out of gamut, and the untouched majority swamped exactly this kind of signal in the
gamut work.

- [ ] **Step 4: Measure text on a real screen**

The stated motive is text and logos, not the calibration grid — so measure it.

Point the throwaway device at a screen with black text over or beside saturated
content. `byonk-builtin/default` renders `background.jpg` (a station concourse) with
text over it and is the obvious candidate; confirm by rendering it and looking.

```bash
CONFIG_FILE=/tmp/pin-check.yaml CARGO_BUILD_JOBS=2 cargo run -- render \
  --mac AA:BB:CC:DD:EE:01 --output /tmp/text-after.png
```

Compare against the same render from before Task 3 (`git stash` the branch or render
from `8d54ae7`). Report the black share of glyph-interior pixels before and after, and
say plainly whether the difference is visible at 1:1 — **not** on a downscaled view,
which reads about 30% too dark in this tree and would flatter any change that darkens.

If `default` has no black text against saturated content, say so and name a screen
that does, or state that no shipping screen exercises the motivating case — which is
itself worth knowing before this graduates.

- [ ] **Step 5: Recommend a `pin_carry` default**

Write the recommendation into the commit message, citing the numbers from Steps 2–3.
The current default is 0.9 and was chosen from arithmetic, not measurement. If the
sweep says otherwise, change the default in `options.rs` and say why.

If a value cannot be justified from these measurements, say that plainly rather than
defending 0.9 with a threshold derived from the same reasoning that produced it.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add crates/eink-dither/tests/pin_diagnostics.rs crates/eink-dither/src/dither/options.rs
git diff --cached --stat
git commit -m "test(eink-dither): pin diagnostics — lambda sweep, far-edge, cost"
```

---

## Controller gate (not a subagent task)

After Task 5, the controller runs the full gate in a **backgrounded** Bash call and
polls it:

```bash
make check
```

~10 minutes. It runs `cargo fmt` (not `--check`), so expect a dirty tree afterwards;
commit any reformatting separately.

Then run the `#[ignore]` gamut evidence, which `make check` does not cover, to confirm
this work did not disturb it:

```bash
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib gamut::mapper::tests::ray_geometry_diagnostic -- --ignored --nocapture
CARGO_BUILD_JOBS=2 cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture
```

Note the three **pre-existing** failures in
`preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
test_resize_full_pipeline_with_photo_preset}` — they panic in `resize_lanczos`, by
design, and are unrelated to this work. Do not "fix" them.

Record the final test counts (byonk lib was 451, eink-dither lib 202 at `6c555de`) —
re-measure, do not inherit.
