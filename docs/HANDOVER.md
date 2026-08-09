# Handover — Byonk

_Last updated: 2026-08-09 (session 8) — the adaptation defect found in session 7
is **fixed and measured**. The owner ruled "redesign the adaptation"; that is
done, green, and committed. **One new owner decision is open: yellow.** Task 14
(docs + `CHANGES.md`) is still not started, deliberately.
`feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| Last code commit | see `git log --oneline -5`; session 8's fix is the adaptation redesign |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` green, tree clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — **contradiction resolved in session 8** |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

---

# What session 8 did

## The fix

`mapper.rs::mapped_chroma` computed `compress_chroma(c / R, c_max, knee)`. The
division by the adaptation factor happened **before** the knee, so it applied to
every pixel unconditionally — including colours already inside the gamut.

`R` now enters **only the input span of the knee's tail**:

```
C <= k*Cmax :  C' = C                                  // identity, at every R
C >  k*Cmax :  C' = k*Cmax + (1-k)*Cmax * shoulder(t)
               t  = (C - k*Cmax) / ((R-k)*Cmax)        // R scales the tail only
```

At `R = 1` this is *exactly* the previous curve, which is the property that makes
the change safe: nothing about the shoulder, `SHOULDER_POWER`, or the `Cmax`
table changed. `compress_chroma` gained an `r` parameter; `map_color` and
`map_frame` signatures are unchanged.

## Measured, not assumed

| | before | after |
|---|---|---|
| red ink `#B50303` (ρ=1.04) | 40% chroma kept | **80%** |
| blue ink `#205497` (ρ=1.03) | 40% | **80%** |
| green ink `#0D876B` (ρ=1.02) | 40% | **81%** |
| mixed field, in-gamut third | mean chroma 0.0297 | **0.0411** (+38%) |
| mixed field, whole frame | 0.0354 | **0.0470** (+33%) |

TDD was followed: both new mapper tests were watched RED first, and the RED run
independently reproduced session 7's "40%" number before any fix was written.

## ⚠️ THE NEW FINDING — yellow, and it needs an owner decision

**The owner's original complaint named yellow. Red and blue are now restored;
yellow is not, and cannot be by this design.**

`ρ(yellow) = 2.112` — the mapper considers the panel's own yellow ink to be 2.1×
out of gamut. Session 8 chased this and it is **not** a bug in the fix, the
`Cmax` table, or the hull:

- `Hull::contains(yellow) == true` — yellow is a hull vertex, as it must be.
- At yellow's own lightness (`L = 0.933`), scanning `Hull::contains` along the
  constant-`L`, constant-hue chroma ray shows it **inside for C ≤ 0.073, outside
  above that, and inside again at exactly one sample** — the vertex itself. A
  fine scan at 1e-4 resolution found a single in-hull point across a 0.008-wide
  window centred on the ink.

So the ray only *grazes* the hull at yellow. `Cmax ≈ 0.073` is geometrically
correct, and the conclusion is about the strategy, not the code: **a chroma-only
mapper, compressing at fixed lightness, cannot reach the yellow ink.** Yellow
maps `#FFEE00 → #F1ECAB`, a pale cream.

The cause is that a constant-`(h, L)` locus is a straight line in Oklab but a
*curved* path in linear RGB, so convexity does not make the in-hull set an
interval. `mapper.rs`'s "Why chroma-only suffices" section is correct that every
`(L, h)` has a non-empty `[0, Cmax]` — but that is a weaker claim than "the
palette's own inks survive", which is what one would want and is false for
yellow.

**Two options for the owner, neither chosen:**

1. **Accept and document.** Marked regions render yellow as pale cream. Cheapest,
   honest, and Task 14 must say so.
2. **Scope a cusp-anchored mapper** that also moves lightness (as CAM16 / ACES
   gamut mappers do), compressing toward a focal point on the achromatic axis
   rather than at fixed `L`. This is a design change of comparable size to the
   whole gamut initiative, not a patch.

## The knee is still not the interesting knob

The knee sweep was *not* re-measured — the owner explicitly deferred ruling 4 and
re-opening it is a separate question. Two related doc corrections were made
instead, because they asserted numbers computed against the superseded curve:

- `GamutOptions::knee`'s "82.4% / 91.2%" figures were **removed, not restated**.
  They have not been re-measured.
- `GamutOptions::max_compression`'s "literally never compress chroma by more than
  this" was literal under `c/R` and is not anymore.

## What still looks subdued, honestly

The mapped output is still markedly less saturated than the source, and the owner
should expect that when looking at the renders. That is the panel's gamut being
genuinely small — the green ink's chroma is 0.106 and the blue's 0.123, so even
modest sRGB saturation is out of gamut — not the defect that was just fixed. The
defect was that content *inside* the gamut was being pulled down too; that part
is now provably fixed.

## Imagery to show the owner

- `target/dither-compare/gamut-mixed-content.png` — **source | OLD curve | NEW
  curve**, with the old curve reconstructed inside the test so the two are
  directly comparable. This is the one to look at.
- `target/dither-compare/gamut-mapping-field.png` — the fully-saturated field.
  Regenerated, but it is a poor demonstration: at `ρ` p50 = 2.87 essentially
  every pixel is in the tail, so it cannot show a change that is about in-gamut
  content.

Regenerate both with
`cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`.

---

## How to resume

1. Read this file, then `git log --oneline -8`.
2. Put the yellow decision to the owner. **Do not pick one unattended.**
3. Task 14 (docs + `CHANGES.md`) is unblocked *for the adaptation*, but its
   user-facing text depends on the yellow ruling — an option-1 answer means the
   docs must state the limitation. Wait for the ruling.

## Tests that now guard this

- `gamut::mapper::tests::sub_knee_chroma_is_untouched_however_large_r_is` — the
  absolute property nobody had written down. Sweeps `R` over `[1, 2.5]`.
- `gamut::mapper::tests::a_colour_on_the_gamut_boundary_keeps_most_of_its_chroma`
  — a colour at exactly `Cmax` must keep >75%.
- `gamut::knee::tests::*` — every pre-existing property (identity below the knee,
  continuity, strict monotonicity, asymptote) is now asserted **across every
  reachable `R`**, not just implicitly at one.
- `gamut::knee::tests::a_larger_adaptation_factor_squeezes_the_tail_harder` — the
  one thing `R` is actually for.
- `gamut::knee::tests::an_adaptation_factor_below_one_is_treated_as_one` — covers
  0, negative and `NaN`.
- `tests/gamut_adaptation_diag.rs` — was the session-7 evidence asserting red
  kept **<50%**; it is now the standing guard asserting red, blue and green keep
  **>70%** while `R` is pinned at its cap. Yellow is excluded on purpose, with
  the reason in the module docs.

## ⚠️⚠️ Read this before dispatching any subagent

**`make check` exceeds 600 seconds in this tree.** The subagent stream watchdog
fires at 600 s of silence, so **an implementer that runs `make check` in the
foreground dies mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets`
  then `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib`. Say so in the brief.
- **The controller runs the full gate**, in a **backgrounded** Bash call
  (`run_in_background: true`), and polls.

When an implementer stalls, **do not resume it blindly a second time** — assess
the abandoned working tree first.

## What landed

| Commit | What |
|---|---|
| `7bfe866`…`57bb440` | **Tasks 1-9** — `Oklch`, `gamut::{hull,cmax,adapt,knee}`, `GamutMapper`, oracle validation, the tone-mask rewriter, `rasterize_tone_mask` |
| `9b1d3e7`, `4a53c09`, `dcfcfba` | **Task 9b** — stroke-evidence stack + fixes |
| `82e7330` | **Task 10** — gamut mapping wired into `render_to_palette_png` |
| `e5d639e` | **Task 11** — `GamutTuningValues` + the Lua `gamut` table |
| `a3a3e7f` | **Task 12** — knobs threaded through the whole display path |
| `c415219` | **Task 12 fix** — regression test for the one compiler-invisible copy site |
| `5d14fd3` | **Ruling 14** — `amount` clamped to `[0,1]` in `mapped_chroma` |
| `e0d85b7` | **Task 13** — hue-order + local-contrast metrics and the visual goldens |
| `2f4a2a6` | **The adaptation diagnostic** — evidence for session 7's finding |
| session 8 | **The adaptation redesign** — `R` moved into the tail's input span |

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma` — **signature changed, now takes `r`**.
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey}` — import, never copy.

**The feature is end-to-end live** but reaches nothing: it applies only where an
SVG marks a region `data-byonk-tone="continuous"`, and **no shipping screen does**.
So neither the old defect nor the new fix changes any rendered output today.

## Open owner decisions

**1. Yellow.** See the finding above. New, and the one that matters.

**2. The gamut calibration screen's marker — the owner has already given the
answer, it is not yet built.** Asked whether
`screens/builtin/calibration/gamut/screen.svg` should keep
`data-byonk-tone="continuous"`, the owner's answer was: *"for a test screen I
would suggest that it contains the same content twice, once with marker and once
without to show the difference."*

Two shapes were put to the owner and **no reply came**:

- **Split cells** — each patch halved, left unmapped / right marked. Best
  comparison (no eye travel), labels unchanged; but the tone-mask boundary then
  runs through all 144 patches, so error diffusion bleeds across each one.
- **Stacked grids** — six raw rows above six marked rows. One boundary instead of
  144, cleaner diffusion; but halves patch height and separates the comparison.

`screen.svg` is currently **unmarked and clean**.

## ⚠️ The lesson, now proven eight sessions running

**The plan's code and constants are not evidence.** Measure before believing the
plan, your own diagnosis, a reviewer's "harmless", a reviewer's "correct" — **or
the spec.**

Session 8's additions:

- **Fixing the code is not fixing the bug's cause.** The spec said two
  incompatible things for six sessions and nobody noticed. Session 8 rewrote the
  spec's "Content adaptation" and "Per pixel" sections so the prose, the
  guarantee and the formula now agree, and recorded *why* the old form was wrong
  in both the spec and `knee.rs`. Leaving a self-contradictory spec in place
  would have re-armed the trap for the next reader.
- **A surprising number is a lead, not noise.** Red, blue and green all landed at
  80–81%; yellow at 42%. It would have been easy to average that away as "mostly
  fixed". Chasing the outlier is what surfaced a design limitation that the
  headline claim would otherwise have papered over.
- **Prove the geometry before blaming the code.** The yellow investigation ran
  through four hypotheses — bin resolution, a broken bisection, a lossy OKLCh
  round-trip, hull re-entry — and only the last survived contact with a scan of
  `Hull::contains`. Each was cheap to test and would have been wrong to assert.
- **Show the comparison, not the result.** The first mixed-content render looked
  subdued and proved nothing. Reconstructing the *old* curve inside the test, so
  old and new sit side by side on identical input, is what made the change
  judgeable — and it turned "looks a bit better" into "+38% in the in-gamut
  third".

Session 7's still-true lessons:

- **Every task passed review. The feature was still wrong.** What saved it was
  the owner looking at a picture. **Ask what the tests do not assert** — when a
  suite is all-relative (monotonicity, hue order, "differences preserved"), find
  the absolute claim nobody wrote down.
- **A test can pass for the right reason and still guard nothing.**
  `test_gamut_mapping_preserves_local_contrast` went on passing with
  `compress_chroma` replaced by pure clipping. Fixed in `e0d85b7`.
  **Mutation-test the assertions you inherit.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.**

Session 6's still-true additions: a green suite proves nothing about a site the
compiler cannot reach (`CachedContent::with_tuning` copies fields by hand;
deleting one left all 448 tests passing); grepping for a sibling field finds
*fields and literals*, not hand-written *copies* between structs.

## Fifteen standing rulings — carry these forward

> **Provenance matters here.** Rulings **1-9** are genuine owner rulings.
> **10-12** were made in session 6 by task reviewers and the controller **while
> the owner was absent** — do not present them as settled. **13-15** are genuine
> owner rulings from sessions 7 and 8.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. **Knee default 0.6 → 0.8** (`3fd9ab8`). Measured at a 1.7% effect under the
   *old* curve. **Not re-measured under the new one; deliberately deferred.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). `linear_to_srgb`
   has an epsilon-free `debug_assert!` — unclamped panics under `cargo test`,
   behaves identically in release. **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`, `BEYOND_LIMIT_MIN_RATIO = 0.3`.
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too** (`ba8859c`).
8. **Task 9b — the mask must not invent a stroke** (`297b10a`). Stroke-evidence stack.
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it
    must be **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B confirmed — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session
    8). Chosen over lowering `max_compression` (partial) and over accepting the
    limitation (narrow). Sub-knee identity is now a tested invariant.

Standing: **the branch is HELD** — no PR, no merge to `main`.

**Constants inherited from the plan and never challenged:**
`PERCENTILE = 0.99`, `MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`,
`C_SEARCH_HI = 0.5`. `max_compression = 2.5` was challenged in session 7; under
the new curve it is no longer the dominant knob it was, because it can no longer
touch sub-knee chroma.

## Deferred minors — triage list for the final whole-branch review

Session 8:

- `mapper.rs`'s "Why chroma-only suffices" module section is *true* but weaker
  than it reads: a non-empty `[0, Cmax]` at every `(L, h)` does not imply the
  palette's own inks survive. Yellow is the counterexample. Worth a sentence once
  the yellow ruling lands.
- The `Cmax` table under-reports at a hull vertex where the constant-`L` ray
  grazes (yellow: exact 0.073 vs bilinear sample 0.093 — the *sample* overshoots
  here, harmlessly). Bin resolution is adequate; noted only so the next reader
  does not re-derive it.

Session 7:

- `test_gamut_mapping_preserves_hue_order` would also pass against an **identity**
  mapper. Weak guard, kept deliberately.

Session 6:

- **Task 10:** the unreachable mask-length-mismatch branch returns
  `RenderError::Dither`, a misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)`
  **cannot** detect a restated-constant violation — manual-review-only.
- **Task 11:** `PanelDitherConfig` accepts a `gamut:` key in panel YAML; verify it
  is live now that Task 12 has landed.
- **Task 12 (inherited):** `resolve_effective_tuning` replaces the **whole** struct
  when any override field is set, so an active dev-UI query override resets the
  previewed gamut to default and diverges from production.

Earlier sessions:

- **Task 7:** the winning dilute start was `eps = 0.005`. Optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`;
  traced — failure mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()`
  while `rewrite_start` propagates them. Style wart.
- **Task 8:** element names matched as raw bytes, so `<svg:image>` would be
  mis-handled and `<symbol>` gets no `<defs>`-style stripping. Dormant.
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`.
- **Task 9b:** `resolve_stroke` cannot see stylesheet-only strokes. Deliberate.
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three
  epsilons want a comment; `adapt.rs`'s `max_compression < 1.0` collapse is
  untested; no test exercises literal `NaN`.

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour; every
artifact that matters is at a boundary *between* colours. Render the field and
look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**And looking is not sufficient either** (session 7): "which of these is more
saturated" is not a judgement the eye makes reliably at small differences.
**Look to find what to measure; measure to decide.** Session 8's corollary: when
comparing an old behaviour to a new one, **render both from the same input in the
same image** — a single "after" picture is not judgeable.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run.
Equally, **never take a subagent's "all green" at face value**.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Exceeds 600 s — background it.**
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **byonk lib suite is 449 tests** (+1 ignored); eink-dither lib **199** (+19 ignored). Re-measure, don't inherit.
- `make docs` needs `mdbook-mermaid`.
- **`make check` does not run the `#[ignore]` tests**, and the gamut metrics,
  the diagnostic and the visuals are all ignored. Run them explicitly:
  - `cargo test -p eink-dither test_gamut_mapping -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture`
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- **`cargo test -p eink-dither --lib -- --ignored` takes ~5 minutes** and reports
  **3 pre-existing failures unrelated to this work**:
  `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`
  panic at `preprocess/resize.rs:26`. `resize_lanczos()` panics **by design**.
  Not a regression — dead tests guarding a dead code path.
- **Rendering a builtin screen needs a device, and the plan's CLI is wrong.**
  There is no `render --screen X --out Y`; it is `render --mac <MAC> --output
  <PATH>`, resolved through config. Do **not** edit the tracked `config.yaml` —
  copy it, point `CONFIG_FILE` at the copy, and add a throwaway device:
  ```yaml
    "AA:BB:CC:DD:EE:01":
      panel: reterminal_e1002          # WITHOUT this you get a GREYSCALE render
      screen: byonk-builtin/calibration/gamut
  ```
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
  **The missing-panel trap is silent** — it renders happily, just in greyscale,
  which looks like a gamut result and is not one.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the
  workspace cannot resolve `eink-dither`. Releases unaffected
  (`Dockerfile.release`, CI-built binaries). Out of scope, untouched.

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale. **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. An unmapped
   100×100 `#ff00aa` frame dithers to a PLTE of a single colour.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does
not work. `AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on
both axes — changing the default alters rendering for every device, so it is the
owner's call.
