# Handover — Byonk

_Last updated: 2026-08-09 (session 8). Session 7's adaptation defect is **fixed,
measured and committed**. Session 8 then went further at the owner's direction
and established, with measurements and pictures, that **the compression
*direction* is also wrong** — and the owner has ruled on the replacement.
**Nothing of that ruling is implemented yet. That is the next session's job.**
`feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| Last production commit | `a5c7df0` — the adaptation fix + its doc sweep |
| Prototype commits | `36a546a`, `6a5bdf2`, `73c0a17`, `9e5a8a0` — **test-only, nothing in the shipping path** |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` green at `a5c7df0`; everything after is `#[ignore]` test code, fmt+clippy clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` — **now behind reality; the anchor work is not in it** |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — adaptation section rewritten in session 8; **the fixed-lightness premise in it is now known to be wrong** |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

---

# ⚠️ START HERE — two owner rulings to act on

Neither is written into production code.

## Ruling 16 — the compression direction becomes mid-grey anchored

Today the mapper compresses **chroma at fixed lightness**: a colour keeps its
exact `L` and only gives up saturation. That was never an explicit decision; it
arrived as `mapper.rs`'s "Why chroma-only suffices" and went unchallenged.

It is wrong, and yellow was the proof. At yellow's own lightness (L = 0.933) the
reachable chroma is 0.073 against the ink's 0.197 — so saturated yellow washes
out to cream and **the panel cannot render its own ink**. But 0.028 of lightness
lower sits a point at 97% of that chroma. Fixed-lightness compression is
structurally forbidden from going there.

**The generalisation:** compress along a line converging on an anchor on the
neutral axis, instead of along a horizontal fixed-`L` line. Four anchors were
prototyped and measured (`crates/eink-dither/tests/gamut_cusp_prototype.rs`):

| anchor | `anchor_l` | panel yellow | photo out-of-gamut px | mean \|dL\| |
|---|---|---|---|---|
| fixed-L *(today)* | `src_l` | 34% | 72% / 60% | 0 |
| cusp-L | cusp lightness at that hue | 40% | 80% / 70% | 0.020 |
| **mid-grey ← RULED** | `0.5` | **82%** | **81% / 75%** | 0.012 / 0.009 |
| half-way | `0.5*(src_l+0.5)` | 82% | 78% / 70% | 0.006 / 0.005 |

**Anchoring at the cusp — the textbook answer, and the controller's confident
recommendation — barely helps.** The cusps sit within 0.012 of the inks' own
lightness, so the ray still climbs into the pinched region. It was measured, not
assumed, and the measurement killed it. Plain mid-grey wins and is the simplest.

The lightness-excursion fear that made half-way look attractive **did not
survive photographs**: the −0.26 excursion is synthetic sRGB green, a colour
photos do not contain. On real content mean |dL| stays ≤ 0.012 for every anchor.

## Ruling 17 (proposed, NOT yet ruled) — the knee should rise, probably to ~0.99

**The owner asked for the pictures and they have been rendered, but no ruling
was recorded before the session ended. This is the first thing to put to them.**

The panel's inks do not survive the mapper even when nothing needs compressing.
Cause is structural: the knee bends at `k*Cmax` and the shoulder above it is
asymptotic to `Cmax`, so **no input can ever be mapped onto the gamut boundary**
— and a panel ink *is* the boundary. At `k = 0.8` an ink comes back at 82%.

The knee's justification is that its headroom keeps out-of-gamut colours
distinguishable. Measured over 24 hues × 5 lightnesses (4932 out-of-gamut
samples), that is not happening:

| knee | inks keep | tail span | distinct outputs surviving |
|---|---|---|---|
| 0.80 *(today)* | 82% | 0.0171 | 74.5% |
| 0.90 | 91% | 0.0171 | 75.3% |
| 0.95 | 95% | 0.0172 | 75.9% |
| **0.99** | **99%** | 0.0172 | **77.2%** |

Raising the knee costs the tail nothing and *slightly improves* it. `R` is
already setting the tail width; the knee was paying in-gamut chroma for
separation it was not delivering.

**Confirmed visually too** (`knee-swatches-mapped.png`, `knee-portrait-mapped.png`,
`knee-background-mapped.png`; rows/grid = source, 0.80, 0.90, 0.95, 0.99): by
0.99 the panel inks are back to source while the far-out sRGB primaries barely
move, saturated regions that 0.80 visibly dulls (the portrait's magenta flowers,
the background's orange departure boards) are restored, and **no banding appears
in any gradient at any knee value.**

This **supersedes ruling 4** if accepted. Note it interacts with ruling 16 —
both act on the tail — so re-measure after the anchor lands rather than assuming
these numbers carry over.

## How to implement ruling 16

The prototype in `tests/gamut_cusp_prototype.rs` is the reference; `RayMapper`
is the shape production should take. Note the self-check it carries:
`Anchor::FixedL` reproduces `GamutMapper` to within 3/255, which is the
`CmaxTable` bilinear error against exact hull bisection. **Keep an equivalent
self-check** — it is what makes every other number in that file trustworthy.

What carries over unchanged: the hull, the knee curve, the adaptation factor,
the tone-mask rewriter and rasterizer, and all the config/Lua/CLI plumbing
(Tasks 8–12). What changes:

- `mapped_chroma` stops being about chroma. The operation becomes: build the ray
  from `(anchor_l, 0)` to `(src_l, src_c)` at fixed hue; bisect the hull for
  `t_max`; compress the ray parameter with `compress_chroma(1.0, t_max, knee, r)`
  — it is homogeneous, so it applies to a ray parameter exactly as to a chroma;
  read the mapped point back off the ray.
- `rho` becomes `1 / t_max` along that ray, not `C / Cmax`.
- **Per-pixel cost rises.** Today's `rho` is one table lookup; the prototype
  bisects the hull 24 times per pixel. `CmaxTable` may need a sibling
  "distance to boundary along the ray from mid-grey" table, or `t_max` needs
  caching. **This is unbenchmarked and is the main implementation risk.**
- Task 7's oracle and the `IN_LIMIT_MAX_RATIO` / `BEYOND_LIMIT_MIN_RATIO`
  constants were validated against fixed-`L` geometry and must be re-derived.

Tests that must be written RED first: an ink at `rho = 1` survives at ≥ the
knee's design point; hue is still preserved; output still lands in the hull;
the map is still monotonic along the ray.

---

## What session 8 already landed (production)

`23a1e39` and `a5c7df0`. **The adaptation factor no longer divides chroma.**

`mapper.rs::mapped_chroma` computed `compress_chroma(c / R, c_max, knee)` — the
division happened *before* the knee, so it hit every pixel unconditionally,
including colours already in gamut. `R` now enters **only the input span of the
tail**:

```
C <= k*Cmax :  C' = C                                  // identity, at every R
C >  k*Cmax :  C' = k*Cmax + (1-k)*Cmax * shoulder(t)
               t  = (C - k*Cmax) / ((R-k)*Cmax)
```

At `R = 1` this is *exactly* the previous curve, which is what made it safe.
Measured with `R` pinned at the 2.5 cap: red 40% → 80%, blue 40% → 80%, green
40% → 81%; on a mixed field the in-gamut third rose 0.0297 → 0.0411 (+38%).

Both new mapper tests were watched RED first, and the RED run independently
reproduced session 7's 40% figure.

The spec's self-contradiction is repaired: "Content adaptation" and "Per pixel"
now agree with each other and with the code, and record why the old form was
wrong. `adapt.rs`, `knee.rs` and both `GamutOptions` knob docs were swept for
the same stale description.

## Tests guarding the production fix

- `gamut::mapper::tests::sub_knee_chroma_is_untouched_however_large_r_is` — the
  absolute property nobody had written down. Sweeps `R` over `[1, 2.5]`.
- `gamut::mapper::tests::a_colour_on_the_gamut_boundary_keeps_most_of_its_chroma`.
- `gamut::knee::tests::*` — every pre-existing property now asserted across
  every reachable `R`, plus `a_larger_adaptation_factor_squeezes_the_tail_harder`
  and `an_adaptation_factor_below_one_is_treated_as_one` (0, negative, `NaN`).
- `tests/gamut_adaptation_diag.rs` — flipped from session 7's evidence (red kept
  **<50%**) into the standing guard (red, blue, green keep **>70%** with `R` at
  the cap). **Yellow is excluded on purpose**, with the geometry in the module
  docs; ruling 16 is what will fix yellow.

---

## ⚠️⚠️ Read this before trusting any dithering picture

**Every visual dither comparison in this initiative reads about 30% too dark,
and it is the viewer's fault, not the ditherer's.** Measured on the same
buffers:

| | mean LINEAR luminance | mean GAMMA-SPACE byte |
|---|---|---|
| portrait | **+10.2%** vs source | −32.4% vs source |
| background | **+4.4%** vs source | −29.3% vs source |

Error diffusion preserves brightness in linear light, which is where averaging
is physical. An image viewer downscaling a PNG without linearising averages sRGB
bytes directly, which under-weights the bright pixels in a black/ink speckle. On
the panel the eye averages the dots optically, in linear light.

- **Relative** comparisons between dither variants remain valid — the artefact
  hits them equally.
- **Absolute** judgements of brightness from those PNGs are worthless. The
  controller called the photo renders "far too dark" and had to retract it.
- **Open defect 1 below ("dark warm under-mixing") was diagnosed this way and
  should be re-measured in linear light before anyone chases it.**

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading** — a flat patch is a single colour;
every artifact that matters is at a boundary *between* colours.

**Whole-image mean chroma is equally misleading for gamut work.** On the
portrait all four anchors scored 0.0545–0.0550 and looked identical, because
only 7% of pixels are out of gamut and the untouched 93% swamped them.
Restricted to the pixels the mapper acts on, the spread was 72% to 81%.
**Measure the pixels the change touches, not the frame.**

**Look to find what to measure; measure to decide.** And when comparing an old
behaviour to a new one, **render both from the same input in the same image** —
a single "after" picture is not judgeable.

**IDE diagnostics lie in this tree.** Session 8 saw `non-exhaustive patterns`
reported for a file that compiled and ran. Verify with an actual `cargo` run.
Never take a subagent's "all green" at face value.

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

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Exceeds 600 s — background it.**
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **byonk lib 449 tests** (+1 ignored); **eink-dither lib 199** (+19 ignored) as
  of `a5c7df0`. Re-measure, don't inherit.
- **`make check` does not run the `#[ignore]` tests**, and all the gamut
  evidence is ignored. Run explicitly:
  - `cargo test -p eink-dither test_gamut_mapping -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture`
    (four tests: the anchor comparison, the knee/ink question, the photographs,
    and `knee_sweep_on_the_chosen_anchor`)
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- Output PNGs land in `target/dither-compare/`.
- **`cargo test -p eink-dither --lib -- --ignored` takes ~5 minutes** and reports
  **3 pre-existing failures unrelated to this work**:
  `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`
  panic at `preprocess/resize.rs:26`. `resize_lanczos()` panics **by design**.
- **Production applies no preprocessing before dithering** — `src/rendering/svg_to_png.rs`
  goes `rgba → Srgb → (gamut map) → dither`. Verified in session 8; a prototype
  that dithers raw pixels is faithful to production in that respect.
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
  **The missing-panel trap is silent** — it renders happily, just in greyscale.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the
  workspace cannot resolve `eink-dither`. Releases unaffected
  (`Dockerfile.release`, CI-built binaries). Out of scope, untouched.
- `make docs` needs `mdbook-mermaid`.

## Useful test assets

`screens/builtin/calibration/color/photo.png` (portrait, 7% out of gamut) and
`screens/builtin/default/background.jpg` (station concourse, 12%) are byonk's
own shipping assets and are what the panel actually renders. Use them; synthetic
fields at full saturation are unrepresentative (`ρ` p50 = 2.87 against a photo's
1.2–1.5) and exaggerate every difference.

## What landed

| Commit | What |
|---|---|
| `7bfe866`…`57bb440` | **Tasks 1-9** — `Oklch`, `gamut::{hull,cmax,adapt,knee}`, `GamutMapper`, oracle validation, tone-mask rewriter, `rasterize_tone_mask` |
| `9b1d3e7`, `4a53c09`, `dcfcfba` | **Task 9b** — stroke-evidence stack + fixes |
| `82e7330` | **Task 10** — gamut mapping wired into `render_to_palette_png` |
| `e5d639e` | **Task 11** — `GamutTuningValues` + the Lua `gamut` table |
| `a3a3e7f` | **Task 12** — knobs threaded through the whole display path |
| `c415219` | **Task 12 fix** — regression test for the one compiler-invisible copy site |
| `5d14fd3` | **Ruling 14** — `amount` clamped to `[0,1]` in `mapped_chroma` |
| `e0d85b7` | **Task 13** — hue-order + local-contrast metrics and the visual goldens |
| `2f4a2a6` | Session 7's adaptation diagnostic |
| `23a1e39` | **Session 8 — the adaptation redesign**, `R` moved into the tail's input span |
| `a5c7df0` | `adapt.rs` doc sweep for the same stale description |
| `36a546a` | Prototype: four compression directions, measured |
| `6a5bdf2` | Prototype: photographs + the knee/ink question |
| `73c0a17` | Prototype: the dither-brightness retraction |
| `9e5a8a0` | Prototype: the knee sweep rendered for visual judgement |

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma` — **signature changed in session 8, now takes `r`**.
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey}` — import, never copy.

**The feature is end-to-end live** but reaches nothing: it applies only where an
SVG marks a region `data-byonk-tone="continuous"`, and **no shipping screen
does**. So none of this changes rendered output today — there is no urgency and
no user impact. Keep it that way until rulings 16 and 17 land.

## Open owner decisions

**1. Ruling 17 — the knee.** Pictures rendered, numbers taken, no answer yet.

**2. The gamut calibration screen's marker.** The owner's answer: *"for a test
screen I would suggest that it contains the same content twice, once with marker
and once without to show the difference."* Two shapes were offered and **no reply
came**:

- **Split cells** — each patch halved, left unmapped / right marked. Best
  comparison; but the tone-mask boundary then runs through all 144 patches, so
  error diffusion bleeds across each one.
- **Stacked grids** — six raw rows above six marked rows. One boundary instead of
  144; but halves patch height and separates the comparison.

`screen.svg` is currently **unmarked and clean**.

## The lesson, now proven eight sessions running

**The plan's code and constants are not evidence.** Measure before believing the
plan, your own diagnosis, a reviewer's "harmless", a reviewer's "correct", the
spec — or your own eyes on a downscaled PNG.

Session 8's additions:

- **The confident recommendation was wrong.** The controller told the owner that
  cusp anchoring was the principled fix, citing the literature. Measurement put
  it at 40% against mid-grey's 82%. **Prototype before recommending; a citation
  is not a measurement.**
- **A surprising number is a lead, not noise.** Red, blue and green all landed at
  80–81%; yellow at 42%. Averaging that away as "mostly fixed" would have hidden
  a design limitation the headline claim was papering over.
- **Prove the geometry before blaming the code.** The yellow investigation ran
  through four hypotheses — bin resolution, broken bisection, lossy OKLCh
  round-trip, hull re-entry — and only the last survived a scan of
  `Hull::contains`. Each was cheap; each would have been wrong to assert.
- **The owner's question was better than the controller's plan.** "Why do panel
  colours not dither to themselves?" produced the knee finding, which may be
  worth more than the anchor work and is a one-constant change.
- **Fixing the code is not fixing the cause.** The spec said two incompatible
  things for six sessions. It was rewritten, not just the code.

Session 7's still-true lessons:

- **Every task passed review. The feature was still wrong.** What saved it was
  the owner looking at a picture. **Ask what the tests do not assert** — when a
  suite is all-relative, find the absolute claim nobody wrote down.
- **A test can pass for the right reason and still guard nothing.**
  `test_gamut_mapping_preserves_local_contrast` went on passing with
  `compress_chroma` replaced by pure clipping. **Mutation-test inherited
  assertions.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.**

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448
tests passing); grepping for a sibling field finds *fields and literals*, not
hand-written *copies* between structs.

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made
> in session 6 by task reviewers and the controller **while the owner was
> absent** — do not present them as settled. **13-16** are genuine owner rulings
> from sessions 7 and 8.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. **Knee default 0.6 → 0.8** (`3fd9ab8`). **Very likely superseded by ruling 17 — see above.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). `linear_to_srgb`
   has an epsilon-free `debug_assert!` — unclamped panics under `cargo test`,
   behaves identically in release. **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`,
   `BEYOND_LIMIT_MIN_RATIO = 0.3`. **Must be re-derived under ruling 16.**
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
    `23a1e39`). Chosen over lowering `max_compression` and over accepting the limit.
16. **The compression direction becomes mid-grey anchored** (owner, session 8).
    **Not yet implemented.** Chosen over cusp-anchored (measured at 40% vs 82% on
    yellow) and over the half-way hedge, because the lightness-excursion risk did
    not survive photographic measurement.

**Constants inherited from the plan and never challenged:**
`PERCENTILE = 0.99`, `MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`,
`C_SEARCH_HI = 0.5`. `max_compression = 2.5` was challenged in session 7; under
the session-8 curve it can no longer touch sub-knee chroma.

Standing: **the branch is HELD** — no PR, no merge to `main`.

## Deferred minors

Session 8:

- `mapper.rs`'s "Why chroma-only suffices" module section is *true* but weaker
  than it reads: a non-empty `[0, Cmax]` at every `(L, h)` does not imply the
  palette's own inks survive. **Ruling 16 makes this section obsolete — rewrite
  it, don't patch it.**
- The `Cmax` table's bilinear sample *overshoots* where the hull pinches (yellow:
  exact 0.073 vs sampled 0.093). Harmless today; noted so it is not re-derived.
- `gamut_cusp_prototype.rs` hardcodes the panel palette, as `visual_compare.rs`
  and `gamut_adaptation_diag.rs` already do. Three copies now — worth a shared
  fixture if a fourth appears.

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
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. Note session
   8 established the benign half of this: a flat fill of a *measured ink* dithers
   to that single ink exactly, which is correct and now asserted.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does
not work. `AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on
both axes — changing the default alters rendering for every device, so it is the
owner's call.
