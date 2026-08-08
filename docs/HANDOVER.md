# Handover — Byonk

_Last updated: 2026-08-08 (session 3) — **Gamut mapping: Tasks 1–7 landed and reviewed clean.** Resume by executing Task 8. `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `7cd2395` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` fully green, 192 lib tests, tree clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` (approved) |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## Next action

**Resume the plan at Task 8** using the `superpowers:subagent-driven-development`
skill. Read the ledger first — it records six owner rulings that the plan text
alone does not explain, and it is the recovery map after a compaction.

Per-task loop, now proven five times: `scripts/task-brief` → dispatch
implementer → **verify the build yourself** → `scripts/review-package` →
dispatch task reviewer → resolve the ⚠️ items yourself → ledger line → next
task. All three scripts live in the skill's directory.

**Task 8 is the first byonk-side task** (leaves `crates/eink-dither`) and is a
step up in complexity. Its binding requirements — CSS paint stripping, the
accepted over-marking, `<defs>` handling — live in the plan's **"Deviations
from the spec"** section, *not* in Global Constraints. The constraints block
handed to Task 8's reviewer must include them, or the review will miss them.

## What landed

| Commit | What |
|---|---|
| `7bfe866` | **Task 1** — `Oklch` promoted to `color::Oklch` (`pub`) |
| `2916286` | **Task 2** — `gamut::hull`, incl. decline-to-map when the hull misses the grey axis |
| `8db9e45` | **Task 3** — `gamut::cmax`, `CmaxTable` |
| `284f645` | **Task 5** — `gamut::adapt`, `adaptation_factor` |
| `0d1185b` | **Task 4** — `gamut::knee`, `compress_chroma` |
| `b5c8b36` | **Task 6** — `GamutMapper`, `GamutOptions` |
| `7cd2395` | **Task 7** — `best_reachable` repair + `CmaxTable` oracle validation |

Plan amendments: `aa2615f`, `b986caf`, `0d7053d`, `3fd9ab8`, `03eb802`, `f6f263d`.

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma`. Shared test fixtures are
`gamut::test_support::{six_colour, four_grey}` (inline in `gamut/mod.rs` under
`#[cfg(test)]`) — import them, never copy them.

## ⚠️ The lesson, now proven twice over — read before touching any constant

**The plan's numeric constants are not evidence.** Five have now been wrong.
Never justify a value with a threshold from the same unvalidated plan. Measure
the real domain first: a throwaway probe crate under the scratchpad with a path
dependency on `eink-dither` settles these questions in minutes, and did so
three times this session.

**Session 3 added a second, sharper lesson: measure before believing your own
diagnosis, too.** Task 7's failure was diagnosed, re-diagnosed and finally
inverted. Three plausible hypotheses were raised and each refuted by
measurement before the true cause emerged. Had any one been acted on, correct
code would have been "fixed".

## The six owner rulings — carry these forward

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`). A
   `* 0.95` margin inside `max_chroma` was removed; the test backs off by
   `(0.08 * c).max(0.0015)` instead. `max_chroma` keeps its `l <= 0.001` guard
   because the Oklab→linear cubic map degenerates at pure black.

2. **Task 4 — the exponential shoulder was clipping real pixels**
   (`b986caf`, `0d7053d`). Replaced by the **ACES 1.3 RGC `powerP` curve,
   `t/(1+t^p)^(1/p)`, at `SHOULDER_POWER = 1.2`**.

3. **Task 5 — neither side of the plan was right** (`0d7053d`). Now
   `select_nth_unstable_by(idx, |a, b| a.total_cmp(b))` — O(n), a genuine total
   order, NaN/∞ discarded by the same percentile guard as any outlier.

4. **Knee default 0.6 → 0.8** (`3fd9ab8`). Measured: knee 0.6 renders a frame's
   vivid end at 82.4% of achievable chroma vs 91.2% at 0.8. 0.8 also sits in the
   ACES threshold band. Task 13's sweep can still overrule this.

5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`, session 3).
   The `Oklch → Oklab → LinearRgb` round trip can land just outside the cube,
   and `color::lut::linear_to_srgb` carries an **epsilon-free
   `debug_assert!(0.0..=1.0)`** before clamping for release — so an unclamped
   conversion behaves identically in release but **panics under `cargo test`**.
   Measured over 421k colours at `R = 2.5`: the only excursion is pure white at
   `1.0000001`, one ULP. Excursions are larger and genuine (worst `-4.7e-4` at
   `r = 1.0`) where chroma compression targets a hue outside sRGB — there,
   clamping is the correct answer, not a workaround. **Any later task
   converting `Oklch` back to `Srgb` must do this.** Now a Global Constraint.

6. **Task 7 — the oracle was broken, not the table** (`f6f263d`, session 3).
   See below; this one has a long tail.

## Task 7 in full — the session's main finding

The plan said: *"If it fails, the table is wrong: fix `cmax.rs`, not the
thresholds."* **Both halves were wrong**, and the investigation reversed the
controller's own initial conclusion. What is true:

- **`best_reachable` had a real defect** — pre-existing shared test
  infrastructure that had been quietly under-reporting reachability at the dark
  end for every diagnostic using it. It is coordinate descent; from a pure-black
  start, growing the diluting weight is a **zero-gradient move** (the cost
  normalises by the weight sum) while the smallest ink step overshoots
  `L = 0.0625` — so descent halted at the vertex and reported the target's own
  chroma as the distance. Witness: at `(L=0.06250, C=0.01219, h=1.571)` the
  stock oracle returned `0.01219` ("nothing reachable"); repaired, it returns
  `~0.00014`, landing at ~90% of the table's `0.01354` — **vindicating the
  table**. Fixed in place per owner ruling: dilute near-black starts (derived
  from the darkest palette entry, not hard-coded) plus ladder steps
  `0.001, 0.0005, 0.0001`.
- **`cmax.rs` is correct and untouched. Task 3 stands.**
- **The absolute thresholds were structurally wrong**, independently of the
  oracle: both statistics scale with `Cmax`, which → 0 at *both* lightness
  extremes (1.05× margin even discounting the dark row). Now ratios, with
  `IN_LIMIT_MAX_RATIO = 0.05` (3.9× margin) and `BEYOND_LIMIT_MIN_RATIO = 0.3`
  (1.53× margin), measured from the shipped implementation.
- **`d_out` is a 3-D distance whose nearest hull point need not be radial**, so
  `d_out < 1.5*Cmax` never demonstrated under-reporting — the check only
  establishes "not on the hull". Keep it generous.
- A trapped bin biases `d_out` **upward**, so the trap could never have produced
  the binding *minimum* — which is why the repair left `0.4582` unchanged.

**Constants still inherited from the plan and never challenged:**
`max_compression = 2.5`, `PERCENTILE = 0.99`, `MIN_DISCARD = 32`,
`HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`.

## What remains

8. `src/rendering/tone_mask.rs` — the SVG rewriter (adds a `quick-xml` dep).
   **First byonk-side task**; see the Task 8 note under "Next action".
9. Rasterize the mask, sharing the frame's exact fit transform
10. Wire into `render_to_palette_png` between rasterization and dithering
11. `GamutTuningValues` + the Lua `gamut` table
12. Thread the knobs through eight structs on the display path
13. Regression metrics + the visual golden — **a real stop point**
14. Docs + `CHANGES.md`

**Task 13 needs the owner.** It renders the calibration screen with and without
mapping and asks whether the marker stays. Mean dE is *expected to worsen*;
present both images rather than deciding alone. Its sweep covers knee
0.4/0.6/0.8 and can overrule ruling 4; it is also the right place to A/B
`SHOULDER_POWER`.

## Deferred minors — triage list for the final whole-branch review

- **Task 6:** `map_color`'s `[0,1]` clamp deviates from the plan's literal code.
  Reviewer's verdict: justified, disclosed, correct place for the boundary; no
  action needed. Now codified as ruling 5.
- **Task 7:** the investigation's winning dilute start was `eps = 0.005`, which
  falls between the shipped ladder's `0.003` and `0.01` rungs. Adding `0.005`
  would tighten the witness from `0.00014` toward `0.00007` and the worst
  in-limit ratio from `0.0128` toward `0.0083`. Purely optional — constants were
  correctly measured from the shipped implementation and clear their targets.
- Two pre-existing rustdoc warnings in `eink-dither` (private intra-doc link to
  `apply_error`; unresolved link to `with_distance_metric`). Not from this work.
- `gamut/hull.rs` uses three different epsilon tolerances. Each is locally
  defensible; the reviewer wants a comment saying they are deliberately different.
- `adapt.rs`: `max_compression < 1.0` collapses `R` to `1.0` regardless of
  content — defensible, but untested and unmentioned in the plan.
- `adapt.rs`: no test exercises literal `NaN` input (only `INFINITY`).

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour; every
artifact that matters is at a boundary *between* colours. In the previous
initiative, every arm that improved patch dE made the rendered image worse.
Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**IDE diagnostics lie in this tree.** They have repeatedly reported unresolved
imports and missing functions in a tree that built cleanly. Verify with an
actual `cargo` run. Equally, **do not take a subagent's "all green" at face
value**: every task so far was independently re-verified by the controller, and
that re-verification has caught real issues more than once.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Pass `timeout: 600000`** — it exceeds the Bash
  tool's 120 s default and gets auto-backgrounded otherwise.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. Never `git add -A`.
- `make docs` needs `mdbook-mermaid`.
- **`cargo test -p eink-dither --lib -- --ignored` takes ~5 minutes** and
  currently reports **3 pre-existing failures unrelated to this work**:
  `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`
  panic at `preprocess/resize.rs:26`. `resize_lanczos()` panics **by design**
  ("resize not available (image crate removed)"); `resize.rs` was last touched
  in `8b52e62`, long before this initiative. Do not mistake these for a
  regression — but they are dead tests guarding a dead code path and deserve
  their own cleanup someday.
- **`Dockerfile` is broken independently** — it copies `Cargo.toml`,
  `Cargo.lock` and `src/` but never `crates/`, so the workspace cannot resolve
  `eink-dither`. Releases are unaffected (`Dockerfile.release`, CI-built
  binaries). Out of scope, untouched.

## Open dithering defects — independent of this work

Gamut mapping is the identity on in-gamut targets, so it neither fixes nor
worsens these:

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an
   optimal 47%. In gamut, bound 0.000, so no gamut excuse.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise
   scale. **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red.

The selector work that tried to fix these is **three-for-three refuted** and
should not be resumed without a new idea; `crates/eink-dither/tests/spike_simplex.rs`
is the deliberate record of what does not work. `AtkinsonHybrid` remains an
unlanded candidate that beats Atkinson on both axes — changing the default
alters rendering for every device, so it is the owner's call.
