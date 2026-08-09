# Handover — Byonk

_Last updated: 2026-08-09 (session 7) — **Gamut mapping: Tasks 1-13 landed, rulings 13 and 14 settled. Only Task 14 remains, plus one open owner decision: does the gamut calibration screen keep the continuous-tone marker?** `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| Last code commit | `e0d85b7` (Task 13 metrics + goldens). Verify with `git log --oneline -5`; anything after it is docs only. |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` fully green, tree clean, byonk lib **449** tests (+1 ignored), eink-dither lib **194** (+19 ignored) |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` (approved) |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## ⚠️ Next action: ONE OWNER DECISION, THEN TASK 14

**Task 13 has been executed and judged** (session 7, owner in session). The
renders exist, the metrics landed as `e0d85b7`, and the knee sweep **did not**
overturn ruling 4. **Do not re-run the sweep.** Exactly one thing is open:

> **Does `screens/builtin/calibration/gamut/screen.svg` keep the
> `data-byonk-tone="continuous"` marker?**
>
> The controller **recommends NO**, against the plan's assumption. `meta.yaml`
> says that screen "shows which hues the panel can actually mix and which
> collapse to a single flat colour" — marking it destroys exactly the
> diagnostic it exists to provide. It is the right test *subject* for
> validating the feature and a bad permanent home for the marker.
>
> `screen.svg` is **reverted, tree clean**. If the owner says keep, re-apply is
> a one-line `<g data-byonk-tone="continuous">` wrap around the patch-grid
> `{% for p in data.patches %}` loop (**not** the labels), committed on its own
> per the plan's Step 5.

**What the images showed** (all in `target/dither-compare/`, regenerate with the
commands in Build / verify):

- `gamut-unmapped.png` — at L=56, hues 45-165 are **nine identical flat yellow
  patches**; 0-30 and 285-345 identical flat red.
- `gamut-mapped.png` — those nine separate (60 yellow, 90-165 pale green to
  teal) and the red slabs break into distinguishable steps. **The cost is large
  and visible:** mid/light rows go pastel, L=56 h60-120 is near-white with
  sparse speckle, L=80 is washed out nearly end to end.
- Objective corroboration: mapped PNG 78860 B vs unmapped 59403 B — more dither
  noise, fewer flat areas, consistent with blobs turning into gradation.
- `gamut-mapping-field.png` — same story, more favourable on synthetic content.
- `gamut-knee-{0.4,0.6,0.8}.png` — 0.4 clearly flattest and most washed out,
  0.8 keeps the most chroma while still breaking up the blobs; 0.6 vs 0.8 is
  subtle. **Knee 0.8 stands on eye as well as on measurement.**

This was also the **first end-to-end proof** that the SVG marker → rewriter →
mask → mapper chain is live: the two renders differ, and only because of the
marker.

### Both decisions owed to the owner are SETTLED (session 7)

They are rulings **13** and **14** below.

After the marker decision: **Task 14** is docs + `CHANGES.md`, then the final
whole-branch review. Resume with the `superpowers:subagent-driven-development`
skill and **read the ledger first** — it records every ruling the plan text does
not.

## ⚠️⚠️ Read this before dispatching any subagent

**`make check` now exceeds 600 seconds in this tree.** The subagent stream
watchdog fires at 600 s of silence, so **an implementer that runs `make check`
in the foreground dies mid-run.** This cost session 6 two dead dispatches and
~20 minutes; the second stall made zero progress.

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets`
  then `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib`. Say so in the brief.
- **The controller runs the full gate**, in a **backgrounded** Bash call
  (`run_in_background: true`), and polls. The Bash tool's own 600 s cap
  auto-backgrounds it anyway.

When an implementer stalls, **do not resume it blindly a second time** — assess
the abandoned working tree first (`git status`, `cargo check`, read the diff).
Session 6's second resume produced nothing because the underlying cause was
environmental, not a model failure.

## What landed

| Commit | What |
|---|---|
| `7bfe866`…`57bb440` | **Tasks 1-9** — see the git log; `Oklch`, `gamut::{hull,cmax,adapt,knee}`, `GamutMapper`, oracle validation, the tone-mask rewriter, `rasterize_tone_mask` |
| `9b1d3e7`, `4a53c09`, `dcfcfba` | **Task 9b** — stroke-evidence stack + fixes |
| `82e7330` | **Task 10** — gamut mapping wired into `render_to_palette_png` |
| `e5d639e` | **Task 11** — `GamutTuningValues` + the Lua `gamut` table |
| `a3a3e7f` | **Task 12** — knobs threaded through the whole display path |
| `c415219` | **Task 12 fix** — regression test for the one compiler-invisible copy site |
| `5d14fd3` | **Ruling 14** — `amount` clamped to `[0,1]` in `mapped_chroma`, a Task 6 gap; two tests, both watched RED |
| `e0d85b7` | **Task 13** — hue-order + local-contrast metrics and the visual goldens. Marker deliberately excluded |

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`; `gamut::knee::compress_chroma`.
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey}` — import, never copy.

**The feature is end-to-end live.** A script returning
`{ gamut = { knee = 0.45 } }`, or a `gamut:` block on a device/panel, now reaches
the renderer — but **only where the SVG marks a region `data-byonk-tone="continuous"`**.
No shipping screen does yet, so nothing changes in practice until one does.

## ⚠️ The lesson, now proven six sessions running

**The plan's code and constants are not evidence.** Measure before believing the
plan, your own diagnosis, a reviewer's "harmless", *or* a reviewer's "correct".

Session 6 adds the sharpest version yet:

- **Pre-flight the brief, every time — it has never once been clean.** Session 6's
  three pre-flights found **4, 3 and 9 defects**. Task 12's plan text would have
  produced the exact silent-drop failure its own opening paragraph warns about,
  in four separate places.
- **A green suite proves nothing about a site the compiler cannot reach.**
  `CachedContent::with_tuning` copies tuning fields by hand. Deleting
  `self.gamut = tuning.gamut.clone();` left **all 448 tests passing**. The code
  was right; nothing protected it. Only a deliberate mutation test found that.
- **Grepping for a sibling field finds struct *fields and literals*, not
  hand-written *copies* between structs** — and copies are the real hazard,
  because they compile forever after. Audit with `grep -rn "\.error_clamp"` and
  demand an enumeration, not a summary.
- **Mutation-test every regression test before trusting it**, especially one
  written *because* the suite was silent. Both of session 6's were: the mutated
  build failed on that test and only that test.

Session 7 adds the case where the *test itself* was the defect:

- **A test can pass for the right reason and still guard nothing.** The plan's
  `test_gamut_mapping_preserves_local_contrast` passed — and went on passing
  with `compress_chroma` replaced by pure clipping, **the exact failure it
  claims to detect**. Its ramp normalised by `r = 2.0`, so `c/r` topped out at
  0.16 while `Cmax` there is 0.196: the knee's shoulder was never reached.
  Nothing about the green run hinted at it. **Mutation-test the assertions you
  inherit from the plan, not only the ones you write.**
- **Say "I verified" only after verifying.** That test was reported to the owner
  as "the one with teeth" on the strength of reading it. The mutation run
  disproved that in 30 seconds. Reading a test is not evidence about the test.
- **Pre-flight caught a plan defect that would have panicked**, again: the plan's
  own test code violated ruling 5 (`Srgb::from` an out-of-sRGB `Oklch`). Three
  defects found in Task 13's brief. Still never once clean.

## Fourteen standing rulings — carry these forward

> **Provenance matters here.** Rulings **1-9 are genuine owner rulings**, made
> with the owner in session. Rulings **10-12 were made in session 6 by the task
> reviewers and the controller while the owner was absent** — the only owner
> input all session was "go on" / "continue" / "keep going". Do not present them
> to the owner as already settled. **Ruling 10 in particular (silent `.ok()`
> coercion of a mistyped script value) is one the owner may well want to
> reopen.**

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. **Knee default 0.6 → 0.8** (`3fd9ab8`). Measured; Task 13's sweep may overrule.
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). `linear_to_srgb`
   has an epsilon-free `debug_assert!` — unclamped panics under `cargo test`,
   behaves identically in release. **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`, `BEYOND_LIMIT_MIN_RATIO = 0.3`.
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too** (`ba8859c`).
8. **Task 9b — the mask must not invent a stroke** (`297b10a`). Stroke-evidence stack.
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** A script writing
    `gamut = { knee = "loud" }` gets `None` with no diagnostic. That is exactly how
    `error_clamp`/`noise_scale`/`chroma_clamp`/`strength` already behave. If it is
    ever fixed it must be **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
    Ruling 14 is that decision carried out.
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
    There are no gamut query parameters; adding URL surface is new scope, not
    threading. A code comment in `dev.rs` says so.
13. **Amendment B confirmed — the CLI is gamut-aware** (owner, session 7).
    Task 12's plan text said `src/main.rs`'s `cli_tuning` "gets `gamut: None`",
    but those locals (`cli_error_clamp`, `cli_noise_scale`, …) are **not** CLI
    arguments — the registered branch of that tuple returns the *resolved*
    script > device > panel chain. Following the plan literally would have made
    gamut the one knob `byonk render` silently ignores, and the plan's own
    Step 4 audit rule contradicts it. The override stands; no revert. **It
    remains the only place plan text was overruled.**
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7).
    `mapped_chroma` now applies `opts.amount.clamp(0.0, 1.0)`, `map_frame`'s
    early return widened from `== 0.0` to `<= 0.0`, and the `GamutOptions::amount`
    doc states the clamp. Out of range the expression stopped being an
    interpolation: negative `amount` inverted the correction into a chroma
    *boost*, `amount > 1` desaturated past the target towards grey. Two tests,
    both watched RED first. This matches how `knee` (`knee.rs:61`) and
    `max_compression` (`adapt.rs:59-61`) are already clamped at point of use.

Standing: **the branch is HELD** — no PR, no merge to `main`.

**Constants still inherited from the plan and never challenged:**
`max_compression = 2.5`, `PERCENTILE = 0.99`, `MIN_DISCARD = 32`,
`HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`.

## Deferred minors — triage list for the final whole-branch review

Session 7 additions:

- **Task 13:** `test_gamut_mapping_preserves_hue_order` would also pass against
  an **identity** mapper — hue preservation is all it asserts. Weak guard, kept
  deliberately; consider strengthening it to assert that chroma actually moved.

Session 6 additions:

- **Task 10:** the unreachable mask-length-mismatch branch returns
  `RenderError::Dither`, a misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)`
  **cannot** detect a restated-constant violation — it passes identically whether
  `resolve()` calls `GamutOptions::default()` or hardcodes 0.8/1.0/2.5. That
  property is manual-review-only, not test-enforced.
- **Task 11:** `PanelDitherConfig` now *accepts* a `gamut:` key in panel YAML that
  parses fine but was inert until Task 12; an admin guessing the shape got a
  silent no-op. Re-check now that Task 12 has landed.
- **Task 12 (inherited, not new):** `resolve_effective_tuning` replaces the
  **whole** struct when any override field is set, so an active dev-UI query
  override (e.g. `error_clamp` via URL) resets the previewed gamut to default and
  diverges from what production renders. Symmetric with the other four knobs.

Carried from earlier sessions:

- **Task 6:** `map_color`'s `[0,1]` clamp deviates from the plan's literal code — now ruling 5.
- **Task 7:** the winning dilute start was `eps = 0.005`, between the shipped ladder's `0.003` and `0.01`. Optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`; traced — failure mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()` while `rewrite_start` propagates them. Style wart.
- **Task 8:** element names matched as raw bytes, so `<svg:image>` would be mis-handled and `<symbol>` gets no `<defs>`-style stripping. Dormant. **Same case-and-name matching family as the `viewBox` near-miss — worth one deliberate pass.**
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`.
- **Task 9b:** `resolve_stroke` cannot see stylesheet-only strokes; that element under-marks. Deliberate, documented.
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three different epsilons want a comment; `adapt.rs`'s `max_compression < 1.0` collapse is untested; no test exercises literal `NaN`.

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour; every
artifact that matters is at a boundary *between* colours. In the previous
initiative, every arm that improved patch dE made the rendered image worse.
Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**IDE diagnostics lie in this tree.** Session 6 saw them report `missing field
gamut` in a file that had already been fixed and compiled cleanly. Verify with an
actual `cargo` run. Equally, **never take a subagent's "all green" at face
value** — every task so far was independently re-verified by the controller, and
that re-verification keeps finding real things.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Exceeds 600 s — background it.** See the warning above.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **byonk lib suite is 449 tests** (+1 ignored). Session-6 progression: 437 → 439
  (Task 10) → 445 (Task 11) → 448 (Task 12) → 449 (Task 12 fix). Re-measure, don't inherit.
- `make docs` needs `mdbook-mermaid`.
- **`cargo test -p eink-dither --lib -- --ignored` takes ~5 minutes** and reports
  **3 pre-existing failures unrelated to this work**:
  `preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
  test_resize_full_pipeline_with_photo_preset}` panic at `preprocess/resize.rs:26`.
  `resize_lanczos()` panics **by design**. Not a regression — but they are dead
  tests guarding a dead code path and deserve their own cleanup someday.
- **Rendering a builtin screen needs a device, and the plan's CLI is wrong.**
  There is no `render --screen X --out Y`; it is `render --mac <MAC> --output
  <PATH>`, resolved through config. Do **not** edit the tracked `config.yaml` —
  copy it and point `CONFIG_FILE` at the copy, adding a throwaway device:
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
- Regenerate the Task 13 imagery with
  `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
  and the metrics with
  `cargo test -p eink-dither test_gamut_mapping -- --ignored --nocapture`.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the
  workspace cannot resolve `eink-dither`. Releases unaffected
  (`Dockerfile.release`, CI-built binaries). Out of scope, untouched.

## Open dithering defects — independent of this work

Gamut mapping is the identity on in-gamut targets, so it neither fixes nor
worsens these:

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale. **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. Session 6 saw
   this incidentally: an unmapped 100×100 `#ff00aa` frame dithers to a PLTE of a
   single colour.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does
not work. `AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on
both axes — changing the default alters rendering for every device, so it is the
owner's call.
