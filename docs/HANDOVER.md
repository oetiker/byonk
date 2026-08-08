# Handover — Byonk

_Last updated: 2026-08-08 — **Gamut mapping is under way.** A 14-task plan is written and Tasks 1–2 are landed and reviewed clean. Resume by executing Task 3. `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `2916286` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `cargo test -p eink-dither` green, workspace clippy clean, tree clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` (approved) |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## Next action

**Resume the plan at Task 3** using the `superpowers:subagent-driven-development`
skill. Read the ledger first — it is the recovery map, and it records the owner
ruling and two mid-flight plan amendments that the plan text alone does not
explain.

The loop that worked, per task: `scripts/task-brief` → dispatch implementer →
verify the build yourself → `scripts/review-package` → dispatch task reviewer →
fix loop if needed → ledger line → next task. All three scripts live in the
skill's directory.

## What landed

| Commit | What |
|---|---|
| `dcbad30` | Plan + two pre-flight fixes (shared test fixtures; Task 11 must keep the tree compiling) |
| `7bfe866` | **Task 1** — `Oklch` promoted from `preprocess` (`pub(crate)`) to `color::Oklch` (`pub`) |
| `d06a7d3` | **Task 2** — `gamut::hull`: convex hull of the palette in linear RGB |
| `c45eeca` | Plan fix: Task 2's test module was missing `use crate::Srgb;` |
| `aab2f4a` | Plan amendment for the owner ruling below — **Tasks 3 and 6 requirements changed** |
| `2916286` | **Task 2 fix** — decline to map when the hull misses the grey axis |

New public surface so far: `eink_dither::Oklch`, and `gamut::hull::{Hull, HullShape}`
with `from_palette`, `shape`, `contains`, `lightness_range`, `is_mappable`.
Shared test fixtures live in `gamut::test_support::{six_colour, four_grey}` —
Tasks 3 and 6 import them; do not add local copies.

## The owner ruling — carry this forward

The plan originally had `compute_lightness_range` return `(0.0, 1.0)` when no
reachable neutral was found — asserting full black-to-white reachability at the
moment it proved the opposite. The review caught it; because the defect was in
the plan text, it went to the owner, who ruled: **decline to map.**

`Hull::is_mappable()` now gates it. The consequences reach two unbuilt tasks,
and `aab2f4a` already amended the plan for both:

- **Task 3** — `CmaxTable` must separate two degenerate cases that call for
  opposite behaviour: a **greyscale** palette (`HullShape::Line`) desaturates
  marked content to grey, which the spec explicitly wants; an **unmappable**
  hull (coplanar, or a volume whose grey axis lies outside it) must leave
  content untouched. Hence both `is_achromatic()` and `is_unmappable()`.
- **Task 6** — `GamutMapper::map_frame` returns early on an unmappable hull,
  with a test asserting the identity.

## What remains

Tasks 3–14, unchanged from the plan except as noted above:

3. `CmaxTable` — precomputed `Cmax(hue, lightness)`, bilinear sampling
4. The knee curve — continuous, strictly increasing, asymptotic to `Cmax`
5. Content adaptation — `R` from a percentile with an absolute discard floor
6. `GamutMapper` — assembles 2–5; idempotence, hue, monotonicity properties
7. Validate the fast table against `best_reachable()`, the slow exact oracle
8. `src/rendering/tone_mask.rs` — the SVG rewriter (adds a `quick-xml` dep)
9. Rasterize the mask, sharing the frame's exact fit transform
10. Wire into `render_to_palette_png` between rasterization and dithering
11. `GamutTuningValues` + the Lua `gamut` table
12. Thread the knobs through eight structs on the display path
13. Regression metrics + the visual golden — **includes a real stop point**
14. Docs + `CHANGES.md`

**Task 13 needs the owner.** It renders the calibration screen with and without
mapping and asks whether the marker stays. Mean dE is *expected to worsen*;
present both images rather than deciding alone. Everything before it is
mechanical enough for subagents.

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour;
every artifact that matters is at a boundary *between* colours. In the previous
initiative, every arm that improved patch dE made the rendered image worse —
one scored the best numbers ever measured here and looks abysmal. Never
conclude from patch dE alone. Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**IDE diagnostics lied twice during this session**, reporting unresolved
imports and missing modules in a tree that built cleanly. They were stale
mid-edit LSP state. Verify with an actual `cargo` run before acting on them —
and equally, do not take a subagent's "all green" at face value; both Task 1
and Task 2 were independently re-verified before review.

## Two deviations from the spec, already settled

1. **Exact-match pinning and the `preserve_exact` API change are obsolete.**
   Pinning was removed from the crate entirely (`preprocess/preprocessor.rs`
   doc comment: "That is gone"). No `eink-dither` API change is needed for it.
2. **Both of the spec's prerequisites are satisfied** — pinning is gone and
   `error_clamp` now defaults to `1.0`.

Also settled during planning: **CSS is a real hazard.** Screen templates set
`fill` from `<style>` blocks (`screens/examples/hello/screen.svg` has
`.date { fill: #555555; }`) and a CSS rule beats a presentation attribute, so
Task 8 strips paint declarations from stylesheets rather than relying on
precedence.

## Deferred minors (for the final whole-branch review)

- Two pre-existing rustdoc warnings in `eink-dither` — a private intra-doc link
  to `apply_error`, and an unresolved link to `with_distance_metric`. Neither is
  from this work.
- `gamut/hull.rs` uses three different epsilon tolerances (`EPS` as a raw
  linear-RGB distance; `EPS` scaled by `norm(n)` in `classify`; a hard-coded
  `1e-4` for facet dedup). Each is locally defensible; the reviewer wants a
  comment saying they are deliberately different.

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
