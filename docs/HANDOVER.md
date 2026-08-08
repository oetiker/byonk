# Handover — Byonk

_Last updated: 2026-08-08 (session 2) — **Gamut mapping: Tasks 1–5 landed and reviewed clean.** Resume by executing Task 6. `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `0d1185b` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `cargo test -p eink-dither` green (183), workspace clippy clean, **`cargo fmt --check` clean**, tree clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` (approved) |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## Next action

**Resume the plan at Task 6** using the `superpowers:subagent-driven-development`
skill. Read the ledger first — it records four owner rulings that the plan text
alone does not explain, and it is the recovery map after a compaction.

Per-task loop that has worked twice now: `scripts/task-brief` → dispatch
implementer → **verify the build yourself** → `scripts/review-package` →
dispatch task reviewer → fix loop → ledger line → next task. All three scripts
live in the skill's directory.

## What landed

| Commit | What |
|---|---|
| `7bfe866` | **Task 1** — `Oklch` promoted to `color::Oklch` (`pub`) |
| `2916286` | **Task 2** — `gamut::hull`, incl. decline-to-map when the hull misses the grey axis |
| `8db9e45` | **Task 3** — `gamut::cmax`, `CmaxTable` (+ a rustfmt commit for Task 2's leftovers) |
| `284f645` | **Task 5** — `gamut::adapt`, `adaptation_factor` |
| `0d1185b` | **Task 4** — `gamut::knee`, `compress_chroma` |

Plan amendments: `aa2615f`, `b986caf`, `0d7053d`, `3fd9ab8` (all four explained below).

Public surface so far: `eink_dither::Oklch`; `gamut::hull::{Hull, HullShape}`;
`gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma`. Shared test fixtures are
`gamut::test_support::{six_colour, four_grey}` (defined inline in
`gamut/mod.rs` under `#[cfg(test)]`) — import them, never copy them.

## ⚠️ The lesson of this session — read before touching any constant

**The plan's numeric constants are not evidence.** Three of them were wrong,
and each was defended in the plan by prose that measurement contradicted. The
owner caught the pattern in one sentence: *"you are defending p=1 on the
grounds of 'the plan' but the values in 'the plan' seem not to be well
grounded."* That was correct.

Never justify a value with a threshold from the same unvalidated plan. Measure
the real domain first. A throwaway probe crate under `$CLAUDE_JOB_DIR/tmp`
with a path dependency on `eink-dither` takes minutes and settles these
questions outright — that is how the key number below was established.

**The number that decided everything:** sweeping every sRGB colour with
non-zero chroma against `CmaxTable` for the six-ink palette,
**`rho = C/Cmax` peaks at 5.02** (median 0.91, p90 1.30, p99.9 4.23). `Cmax`
shrinks toward black and white, but so does the chroma an sRGB colour can have
there, so the ratio stays bounded. At `k = 0.6` that is `t = 11.05`.

## The four owner rulings — carry these forward

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`). The
   implementer added a `* 0.95` margin inside `max_chroma` to pass a test. It
   was measured to leave a 7% residual overshoot (so it did not fix what it
   claimed), narrow the test's detection window to `0.874 ×`, and cost 5% of
   the panel's chroma at every hue and lightness — to suppress a 0.001-chroma
   artifact in one near-black row. Removed. The test now backs off by
   `(0.08 * c).max(0.0015)`. `max_chroma` keeps an `l <= 0.001` guard because
   the Oklab→linear cubic map degenerates at pure black, where `contains`
   admits a spurious non-zero chroma.

2. **Task 4 — the exponential shoulder was clipping real pixels**
   (`b986caf`, `0d7053d`). `1 - exp(-t)` saturates in `f32` at `t ≈ 10.2`,
   inside the reachable domain of `t ≈ 11.05`. It returned *exactly* `Cmax`
   for real content — the clipping this feature exists to prevent. Replaced by
   the **ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, at its default
   `SHOULDER_POWER = 1.2`**. Every exponent in 1..2 is monotone across the
   reachable domain with 2.4×–7.8× margin, so monotonicity does not choose
   between them; ACES's empirical backing does. The monotonicity test range was
   re-grounded from an arbitrary `0..10` to `0..3.0` (3× reachable).

3. **Task 5 — neither side of the plan was right** (`0d7053d`). The docs
   promised `select_nth_unstable_by`; the code did a full sort, justified as
   avoiding a panic. That justification is false:
   `partial_cmp().unwrap_or(Equal)` makes NaN equal to everything, so equality
   is not transitive, and the total-order contract is violated identically by
   sort and by select. Now `select_nth_unstable_by(idx, |a, b| a.total_cmp(b))`
   — O(n), a genuine total order, and NaN/∞ sort above all real values so they
   are discarded by the same percentile guard as any other outlier.

4. **Knee default 0.6 → 0.8** (`3fd9ab8`). The plan's rationale ("gamut small
   enough that almost everything is outside it; a high knee crushes the vivid
   range into a sliver") fails on both premises: about **half** the sRGB cube
   is outside the hull, and `map_frame` normalises by `R` (the 99th
   percentile), so the sliver only holds the top ~1% of a region. Measured at
   `p = 1.2`: knee 0.6 renders a frame's vivid end (`rho/R = 1`) at **82.4%**
   of achievable chroma vs **91.2%** at 0.8, buying back tail separation of
   ≤0.005 Oklab chroma against ~0.02 for one JND on a six-ink dithered panel.
   0.8 also sits in the ACES threshold band (0.815 / 0.803 / 0.880).
   **No code impact yet** — `knee` is a *parameter*; the default materialises
   in Task 6's `GamutOptions::default`, Task 11 and Task 12.

**Constants still inherited from the plan and never challenged:**
`max_compression = 2.5`, `PERCENTILE = 0.99`, `MIN_DISCARD = 32`,
`HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`.

## What remains

6. `GamutMapper` — assembles 2–5; idempotence, hue, monotonicity properties.
   **Must return early on an unmappable hull**, with a test asserting identity
   (consequence of Task 2's decline-to-map ruling). Also note it already
   returns early when `r <= 1.0 && !table.is_achromatic()`.
7. Validate the fast table against `best_reachable()`, the slow exact oracle
8. `src/rendering/tone_mask.rs` — the SVG rewriter (adds a `quick-xml` dep)
9. Rasterize the mask, sharing the frame's exact fit transform
10. Wire into `render_to_palette_png` between rasterization and dithering
11. `GamutTuningValues` + the Lua `gamut` table
12. Thread the knobs through eight structs on the display path
13. Regression metrics + the visual golden — **a real stop point**
14. Docs + `CHANGES.md`

**Task 13 needs the owner.** It renders the calibration screen with and without
mapping and asks whether the marker stays. Mean dE is *expected to worsen*;
present both images rather than deciding alone. Its sweep covers knee
0.4/0.6/0.8 and can overrule ruling 4 above; it is also the right place to
A/B `SHOULDER_POWER`.

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour;
every artifact that matters is at a boundary *between* colours. In the previous
initiative, every arm that improved patch dE made the rendered image worse.
Never conclude from patch dE alone. Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**IDE diagnostics lie in this tree.** They have three times now reported
unresolved imports and missing functions in a tree that built cleanly — stale
mid-edit LSP state, and during TDD the red step legitimately shows exactly
these errors. Verify with an actual `cargo` run. Equally, **do not take a
subagent's "all green" at face value**: every task so far was independently
re-verified by the controller before review, and two needed fix rounds.

## Deferred minors (for the final whole-branch review)

- Two pre-existing rustdoc warnings in `eink-dither` — a private intra-doc link
  to `apply_error`, and an unresolved link to `with_distance_metric`. Neither is
  from this work.
- `gamut/hull.rs` uses three different epsilon tolerances (`EPS` as a raw
  linear-RGB distance; `EPS` scaled by `norm(n)` in `classify`; a hard-coded
  `1e-4` for facet dedup). Each is locally defensible; the reviewer wants a
  comment saying they are deliberately different.
- `adapt.rs`: `max_compression < 1.0` collapses `R` to `1.0` regardless of
  content — defensible, but untested and unmentioned in the plan.
- `adapt.rs`: no test exercises literal `NaN` input (only `INFINITY`).

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Pass `timeout: 600000`** — it exceeds the Bash
  tool's 120 s default and gets auto-backgrounded otherwise.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. Never `git add -A`.
- `make docs` needs `mdbook-mermaid`.
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
