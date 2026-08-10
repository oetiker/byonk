# Handover — Byonk

_Last updated: 2026-08-10 (session 9). **Rulings 16 and 17 are both implemented,
measured and committed**, and the gamut work now **reaches a shipping screen for
the first time**: the new `calibration/tone` calibration screen marks a region,
so the feature is no longer inert. The mapper compresses along a mid-grey ray and
the knee default is 0.99. `make check` is green.
`feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| Last production commit | `41ffe40` — the tone screen and its inventory-guard fix |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | **`make check` green (exit 0)** at `41ffe40`, tree clean |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — **current**, rewritten onto the ray geometry this session |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` — **superseded in part**, carries a header saying so |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## What session 9 landed

| Commit | What |
|---|---|
| `8e30e24` | **Ruling 16** — compression runs along a ray converging on mid-grey |
| `140ac3e` | Re-derived the ray-geometry guards; yellow folded back into the standing guard |
| `868544c` | **Ruling 17** — knee default 0.8 → 0.99 |
| `6c555de` | Spec brought onto the ray geometry; plan marked superseded |
| `18d0f31`…`41ffe40` | **The `calibration/tone` screen** — see below |

**Ruling 16.** `GamutMapper` no longer compresses chroma at fixed lightness. It
bisects the hull for `t_max` along the ray from mid-grey (`ANCHOR_L = 0.5`,
clamped into the hull's lightness range) through the colour, compresses the ray
parameter with `compress_chroma(1.0, t_max, knee, r)` — the curve is homogeneous,
so it applies to a ray parameter exactly as to a chroma — and reads the mapped
point back off the ray. `rho` is now `1 / t_max`.

All four measured panel inks come back at **`t_max = 1.000` and the knee's design
point exactly**. Yellow, which the fixed-`L` mapper stranded at 42%, is now
indistinguishable from red, blue and green.

**Ruling 17.** Re-measured on the new geometry before landing, because the
handover's table had been taken on `Anchor::HalfWay` — not the anchor that was
ruled — and both rulings act on the same tail:

| knee | inks keep | tail span | distinct outputs |
|---|---|---|---|
| 0.80 | 82% | 0.0222 | 76.4% |
| 0.90 | 91% | 0.0220 | 78.8% |
| 0.95 | 95% | 0.0219 | 80.6% |
| **0.99** | **99%** | 0.0218 | **81.3%** |

The case is slightly *stronger* under mid-grey than the numbers it replaced
(distinct 76.4→81.3 against 74.5→77.2). The whole tail span is about one JND, so
the 0.0004 given up is invisible.

On the real photographs, out-of-gamut pixels keep **90%** (portrait) and **84%**
(background) of their chroma under mid-grey + knee 0.99, against fixed-`L`'s 82%
and 68%, at mean `|dL|` of 0.0015 and 0.0041. Visually confirmed in
`target/dither-compare/photo-background-mapped.png` (grid: source, fixed-L,
cusp-L / mid-grey, half-way): the departure boards' orange, visibly muddy under
fixed-`L`, is close to source under mid-grey, with no highlight crush.

**The port is proven equivalent to the prototype, pointwise.**
`cusp_anchored_vs_fixed_lightness` carries the self-check the previous handover
demanded, now repointed from `Anchor::FixedL` to `Anchor::MidGrey` — production
is mid-grey, so comparing it against fixed-`L` would have printed a huge
divergence reading as "the prototype is broken". Swept over **5832 colours
across the sRGB cube: worst channel diff 0**. Its old 6/255 tolerance was the
`CmaxTable` bilinear error, from when production read the table and the harness
bisected; both bisect the same ray now, so the tolerance is 1/255 for rounding
and nothing else. **Keep this check working** — it is what makes every other
number in that file trustworthy.

### The one thing to know about the new geometry

**The ray's liability is near-white tints, and the knee is what bounds it.** A
high-`L`, low-chroma colour's ray exits the hull at the *white point*, so it
reads as boundary-saturated even though its chroma was never out of gamut, and
the knee pulls it toward mid-grey. Everything with `t_max > 1/knee` is returned
untouched, so:

| pixel | `t_max` | dL @ knee 0.80 | dL @ knee 0.99 |
|---|---|---|---|
| grey 250 tint 4 | 1.003 | **−0.084** | −0.0035 |
| grey 230 tint 4 | 1.140 | −0.032 | **0.0000** |
| grey 16 tint 12 | 1.146 | +0.027 | **0.0000** |
| grey 250 tint 12 *(really out of gamut)* | 0.954 | −0.097 | −0.025 |

At 0.8 this would have darkened every highlight in a photograph — and been
*contagious*, since one vivid flower pushes `R > 1` for the whole region, which
is the defect ruling 15 existed to kill. **Ruling 16 and ruling 17 are only safe
together. Do not lower the knee without re-measuring this table.**
`gamut::mapper::tests::ray_geometry_diagnostic` prints it;
`a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee` guards it at 0.99.

**Whole-image mean `|dL|` hides this completely** (0.009–0.012, all anchors) —
it is the handover's own "measure the pixels the change touches" trap, and it
nearly let the defect through.

### Risks that were retired by measurement, not design

- **Per-pixel cost.** The handover called the hull bisection the main
  implementation risk. Measured: **218 ms for a worst-case 800×480 frame**
  (0.57 µs/px, release, every pixel saturated and masked). An e-ink refresh has
  minutes. `map_frame_cost_on_a_panel_sized_frame` records it.
- **No `t_max` lookup table was built, deliberately.** A `(hue, angle)` sibling
  to `CmaxTable` would have inherited its documented bilinear *overshoot at the
  pinch* (yellow: exact 0.073 vs sampled 0.093) — and an overshot `t_max` maps
  pixels outside the hull in exactly the region this change exists to fix.
- **`t_max` is bisected once per masked pixel**, shared between the adaptation
  pass and the mapping pass. The prototype did it twice.
- **Task 7's oracle needed no re-derivation.** The handover expected
  `IN_LIMIT_MAX_RATIO` / `BEYOND_LIMIT_MIN_RATIO` to be fixed-`L`-specific. They
  are not: the oracle validates `CmaxTable` against a reachability optimiser, not
  the mapper's direction. Re-ran identical — 360 bins, 0.0128 / 0.4582.

## ⚠️ Two inherited tests were guarding nothing

Both were rewritten, and **both were mutation-verified in each direction** —
a clipping mutant (`let t = 1.0f32.min(t_max)`) must fail them.

- `test_gamut_mapping_preserves_local_contrast` swept a ramp at **fixed
  lightness**. Under the ray that gives every step its own direction, its own
  boundary and its own compression factor, so **a clipping mutant kept the steps
  accidentally distinct and the test passed**. It now sweeps along a single ray,
  where clipping sends every out-of-gamut sample to the same point: real mapper
  min separation 1.7e-5, clipping mutant collapses 7 steps to exactly 0.
- `chroma_map_is_strictly_monotonic` → `the_map_is_strictly_monotonic_along_a_compression_ray`.
  Chroma alone is **not** monotonic under the ray and correctly so: past the
  shoulder it asymptotes and drifts back ~2e-5 as the flattening ray meets a
  nearer boundary. It now makes two claims — never backwards (to a few ulps), and
  strictly increasing at visible steps. `a_saturation_ramp_never_collapses_two_colours_onto_one`
  keeps the fixed-`L` sweep for what it can still honestly assert.

**Bound synthetic sweeps to sRGB's reachable chroma (~0.33 in Oklab).** Both
tests initially failed on samples at chroma 0.5–0.7 that no input can produce; at
a high knee those all sit on the asymptote and tie in `f32`, so the test compares
two colours that have already collapsed. The prototype file had already learned
this once and written it down.

## The `calibration/tone` screen (new, session 9)

**RESOLVED: the long-open "how should the calibration screen show the marker"
question.** The owner chose a new screen rather than converting Gamut Patches:
`byonk-builtin/calibration/tone`, three bands (photograph, hue sweep, patch grid)
rendered in two columns, **only the right column marked**. Split-cells was
rejected — it puts a mask boundary through every patch. No gamut knobs are
exposed: the screen shows what a real screen gets, and a default restated in YAML
would drift from the Rust constants.

Spec: `docs/superpowers/specs/2026-08-09-tone-calibration-screen-design.md`.
Plan: `docs/superpowers/plans/2026-08-09-tone-calibration-screen.md`.

**Measured end-to-end**, comparing the unmarked render against the marked one
through the real CLI path at 800×480:

| | pixels changed |
|---|---|
| left column (control) | **0 / 190,560** |
| right column (marked) | 68,207 / 190,560 (35.8%) |
| overall diff bounding box | `(403, 18) → (796, 476)` — exactly the right column's content area |

The mask geometry test measures a marked fraction of **0.4605**, the value
predicted when the plan was written, with zero leak into the control.

Visually: the hue sweep goes from large flat collapsed blocks to a smooth
dithered gradient, and washed-out cream patches come back as distinct colours —
the yellow fix, visible. No highlight crush in the photograph.

**Caveat on reading it.** With mapping *off*, 27.4% of pixels already differ
between the two columns, because the columns sit at different x offsets and error
diffusion lands differently in each. The columns are not pixel-comparable; they
are *perceptually* comparable (8×8 block delta 2.42 off vs 9.71 on, a 4× margin
over the dither noise floor). The docs say "visibly differs" for this reason.

## Open owner decisions

**1. Should a real content screen be marked?** That is the first change that
alters output a user actually looks at, so it stays the owner's call. The tone
screen exists to inform exactly this decision — look at it on the panel first.

**2. The branch.** Still HELD. Nine sessions of gamut work plus this screen are
sitting unmerged on `feat/screen-store-authoring-core`.

## ⚠️⚠️ Read this before trusting any dithering picture

**Every visual dither comparison in this initiative reads about 30% too dark, and
it is the viewer's fault, not the ditherer's.** Measured on the same buffers:

| | mean LINEAR luminance | mean GAMMA-SPACE byte |
|---|---|---|
| portrait | **+10.2%** vs source | −32.4% vs source |
| background | **+4.4%** vs source | −29.3% vs source |

Error diffusion preserves brightness in linear light. An image viewer downscaling
a PNG without linearising averages sRGB bytes directly, under-weighting the
bright pixels in a black/ink speckle. On the panel the eye averages optically, in
linear light.

- **Relative** comparisons between variants remain valid — the artefact hits them
  equally. Use the `-mapped` (undithered) renders for absolute judgements.
- **Open defect 1 below was diagnosed this way and should be re-measured in
  linear light before anyone chases it.**

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading** — a flat patch is a single colour; every
artifact that matters is at a boundary *between* colours.

**Whole-image means are equally misleading for gamut work.** On the portrait all
four anchors scored 0.0545–0.0550 mean chroma and looked identical, because only
7% of pixels are out of gamut and the untouched 93% swamped them. Restricted to
the pixels the mapper acts on, the spread was 68% to 90%. The same trap hid the
near-white crush this session. **Measure the pixels the change touches.**

**Look to find what to measure; measure to decide.** When comparing an old
behaviour to a new one, **render both from the same input in the same image**.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run. Never
take a subagent's "all green" at face value.

## ⚠️⚠️ Read this before dispatching any subagent

**`make check` takes ~10 minutes in this tree.** The subagent stream watchdog
fires at 600 s of silence, so **an implementer that runs `make check` in the
foreground dies mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets`
  then `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib`. Say so in the brief.
- **The controller runs the full gate** in a **backgrounded** Bash call
  (`run_in_background: true`) and polls. That worked cleanly this session.

When an implementer stalls, **do not resume it blindly** — assess the abandoned
working tree first.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **~10 min — background it.** Green at `6c555de`.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **byonk lib 451 tests** (+1 ignored); **eink-dither lib 202** (+21 ignored) as
  of `6c555de`. Re-measure, don't inherit.
- **`make check` does not run the `#[ignore]` tests**, and most gamut evidence is
  ignored. Run explicitly:
  - `cargo test -p eink-dither --lib gamut::mapper::tests::ray_geometry_diagnostic -- --ignored --nocapture`
  - `cargo test --release -p eink-dither --lib map_frame_cost -- --ignored --nocapture`
  - `cargo test -p eink-dither --lib test_gamut_mapping -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture`
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- Output PNGs land in `target/dither-compare/`.
- **`cargo test -p eink-dither --lib -- --ignored` takes ~5 minutes** and reports
  **3 pre-existing failures unrelated to this work**:
  `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`
  panic at `preprocess/resize.rs:26`. `resize_lanczos()` panics **by design**.
- **Production applies no preprocessing before dithering** — `src/rendering/svg_to_png.rs`
  goes `rgba → Srgb → (gamut map) → dither`.
- **Rendering a builtin screen needs a device, and the old plan's CLI is wrong.**
  It is `render --mac <MAC> --output <PATH>`, resolved through config. Do **not**
  edit the tracked `config.yaml` — copy it, point `CONFIG_FILE` at the copy, and
  add a throwaway device:
  ```yaml
    "AA:BB:CC:DD:EE:01":
      panel: reterminal_e1002          # WITHOUT this you get a GREYSCALE render
      screen: byonk-builtin/calibration/gamut
  ```
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
  **The missing-panel trap is silent** — it renders happily, in greyscale.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the
  workspace cannot resolve `eink-dither`. Releases unaffected
  (`Dockerfile.release`, CI-built binaries). Out of scope, untouched.
- `make docs` needs `mdbook-mermaid`.

## Useful test assets

`screens/builtin/calibration/color/photo.png` (portrait, 7% out of gamut) and
`screens/builtin/default/background.jpg` (station concourse, 12%) are byonk's own
shipping assets and are what the panel actually renders. Synthetic fields at full
saturation are unrepresentative (`ρ` p50 = 2.87 against a photo's 1.2–1.5).

## Public surface

`eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma` (takes `r`).
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey, panel_measured}`
— **import, never copy**. `panel_measured` is new: `six_colour`'s idealised
primaries do not reproduce the hull's pinch, so they cannot guard it.

**The feature reaches exactly one shipping screen: `calibration/tone`.** Every
other screen is untouched, because the mapping applies only where an SVG marks
a region `data-byonk-tone="continuous"`. So rendered output changes for that one
calibration screen and nothing else — still no user impact on real content, and
the screen exists precisely so the effect can be judged on a panel before anyone
marks a content screen.

## The lesson, now proven nine sessions running

**The plan's code and constants are not evidence.** Measure before believing the
plan, your own diagnosis, a reviewer's "harmless", the spec — or your own eyes on
a downscaled PNG.

Session 9's additions, from the `calibration/tone` build:

- **Adding a builtin screen has a fan-out nobody mapped.** Two separate tests
  hardcode the shipped inventory as an exact count —
  `tests/builtin_package.rs:44` and `tests/screen_schemas_test.rs:128`. Each cost
  a ten-minute `make check` cycle, discovered one at a time, because the first
  failure hid the second. **Before adding a builtin screen, grep for what
  enumerates the inventory.** Both guards are strict `assert_eq!` on purpose;
  update them, never loosen them.
- **`make check` runs `cargo fmt`, not `cargo fmt --check`.** It rewrites files
  in place and leaves the tree dirty. Code transcribed verbatim from a plan is
  usually not rustfmt-clean, and no task brief thought to say "run cargo fmt".
  Add it to the implementer's command list.
- **Subagent task briefs scoped to `cargo test -p byonk --lib` cannot see
  integration tests.** That restriction exists for the watchdog, and it is why
  both inventory guards survived three clean task reviews. The controller's full
  gate is not a formality.
- **A reviewer that fact-checks docs against code earns its cost.** Two factual
  errors in one short docs section — a parameter attributed to the wrong band,
  then an off-by-one introduced *by the fix for the first one*. Docs that name
  the wrong knob are worse than absent docs.
- **Measure the claim you are actually making.** "The two columns differ only by
  the mapping" is false at pixel level (27.4% differ with mapping off) and true
  perceptually. Both measurements were right; they answered different questions.

Session 9's gamut-mapping additions:

- **A ruling can carry a latent defect that only a second ruling masks.** The
  mid-grey ray crushes near-white tints by 0.084 at knee 0.8 and 0.0035 at 0.99.
  Ruling 16 measured alone looked broken; ruling 16 measured at the knee the
  owner had *already accepted* was fine. **Measure a change at the settings it
  will actually ship with**, especially when two rulings touch the same
  mechanism.
- **A rewritten test is an unverified test.** Both inherited guards were rewritten
  for the new geometry, and one of them — before the rewrite — already passed
  against a clipping mutant. Mutate every guard you touch, in both directions.
- **Bound synthetic sweeps to inputs that can occur.** Three separate test
  failures this session were samples outside sRGB sitting on the asymptote,
  tying in `f32`. None was a defect.
- **"Strictly increasing" is a claim about maths, not about `f32`.** At the
  asymptote, increments fall below one ulp and adjacent samples tie or jitter
  backwards by one. Split the claim: never backwards to a few ulps, strictly
  increasing at visible steps.
- **The risk the handover flags loudest may be the cheapest to retire.** The
  bisection's per-pixel cost was the stated main risk; one benchmark (218 ms)
  ended it, and the optimisation that would have "fixed" it was the one thing
  guaranteed to reintroduce the bug.

Session 8's:

- **The confident recommendation was wrong.** Cusp anchoring was the principled,
  cited fix and measured 40% against mid-grey's 82%. **Prototype before
  recommending; a citation is not a measurement.**
- **A surprising number is a lead, not noise.** Red, blue and green at 80–81%,
  yellow at 42%.
- **Prove the geometry before blaming the code.**
- **The owner's question was better than the controller's plan.** "Why do panel
  colours not dither to themselves?" produced ruling 17.
- **Fixing the code is not fixing the cause.** The spec said two incompatible
  things for six sessions.

Session 7's still-true lessons:

- **Every task passed review. The feature was still wrong.** What saved it was
  the owner looking at a picture. **Ask what the tests do not assert.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.**

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448
tests passing).

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made
> in session 6 by task reviewers and the controller **while the owner was
> absent** — do not present them as settled. **13-17** are genuine owner rulings
> from sessions 7, 8 and 9.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. ~~**Knee default 0.6 → 0.8**~~ (`3fd9ab8`). **Superseded by ruling 17.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). `linear_to_srgb`
   has an epsilon-free `debug_assert!` — unclamped panics under `cargo test`,
   behaves identically in release. **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`,
   `BEYOND_LIMIT_MIN_RATIO = 0.3`. **Verified still valid under ruling 16.**
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too** (`ba8859c`).
8. **Task 9b — the mask must not invent a stroke** (`297b10a`).
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it
    must be **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session 8,
    `23a1e39`).
16. **The compression direction is mid-grey anchored** (owner, session 8;
    implemented session 9, `8e30e24`). Chosen over cusp-anchored (40% vs 82% on
    yellow) and over the half-way hedge.
17. **The knee default is 0.99** (owner, session 9; implemented `868544c`).
    Re-measured on the ray geometry before landing. Supersedes ruling 4.

**Constants inherited from the plan and never challenged:**
`PERCENTILE = 0.99`, `MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`,
`C_SEARCH_HI = 0.5`, `T_HI = 6.0`, `T_STEPS = 24`, `ACHROMATIC_C = 1e-6`.
`max_compression = 2.5` can no longer touch sub-knee chroma.

Standing: **the branch is HELD** — no PR, no merge to `main`.

## Deferred minors

Session 9:

- `six_colour`'s blue vertex cannot reach the knee's design point (72% at knee
  0.8) because the constant-hue OKLch ray **bulges outside the linear-RGB hull**
  — the hull is convex in linear RGB, and a constant-hue line is straight in
  Oklab, not there. `t_max = 0.861`. Harmless: it is an idealised palette we do
  not ship, and `panel_measured` hits 1.000 on every ink. It has its own measured
  floor in `every_palette_ink_survives_at_the_knees_design_point`.
- `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` still hardcode the
  panel palette, now duplicating `test_support::panel_measured`. Integration
  tests cannot see the crate's `#[cfg(test)]` fixtures; exporting them is the fix
  if a fourth copy appears.
- `mapped_chroma` is now `#[cfg(test)]` — nothing in production reads chroma
  alone any more.

Session 8:

- The `Cmax` table's bilinear sample *overshoots* where the hull pinches (yellow:
  exact 0.073 vs sampled 0.093). **Now load-bearing knowledge** — it is why no
  `t_max` table was built. Do not "fix" it without re-reading that decision.

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

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal
   47%. **Re-measure in linear light first** — see the brightness section above.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise
   scale. **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. The benign
   half is established and asserted: a flat fill of a *measured ink* dithers to
   that single ink exactly, which is correct.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does
not work. `AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on
both axes — changing the default alters rendering for every device, so it is the
owner's call.
