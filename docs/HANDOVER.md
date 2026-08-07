# Handover — Byonk

_Last updated: 2026-08-07 — **One real fix landed (blue-noise defaults, plus a build-system hole that was hiding it). The selector work is three-for-three refuted and should not be resumed without a new idea.** `feat/screen-store-authoring-core` is still **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `37efbe7` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` green, tree clean, pushed to origin |

## Next action

**Nothing is half-finished. Pick a direction deliberately.**

The dithering defects that remain are real but every attempt to fix them has
made the rendered output worse. Do not resume the selector work by trying
another variant of the same idea — see "The refuted family" below, which
records what has already died so it is not re-derived.

Candidates, in the order I would weigh them:

1. **Stop here and consolidate.** The noise-defaults fix is a genuine,
   measured, visible improvement. The remaining defects have survived every
   attempt; they may not be worth further regression risk.
2. **Gamut mapping.** Its spec is approved and untouched
   (`docs/superpowers/specs/2026-08-07-gamut-mapping-design.md`), it is
   independent of the selector, and no gamut code has landed yet.
3. **Reconsider the default algorithm** — `AtkinsonHybrid` is measured better
   than Atkinson on both axes (see below). Small, and still unlanded.

## ⚠️ Read this before trusting any dithering measurement

**`make check` did not run this crate's tests until `de9f605`.** `cargo test`
and `cargo clippy` without `--workspace` cover only the root package, so
nothing under `crates/` was ever checked. A failing `eink-dither` test still
printed "All checks passed!". Coverage went 484 → 951 tests when fixed. Any
"green" claim about dithering in a handover older than this is worthless.

**Flat-patch dE is actively misleading, and it misled this whole session.**
A flat patch is a single colour; every artifact that matters is at a boundary
*between* colours. Every arm that improved patch dE made the rendered image
worse — one of them scored the best numbers ever measured here and looks
abysmal. Never conclude from patch dE alone. Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes original/dithered pairs, magnified crops and before/after triptychs
  to `target/dither-compare/`. Use it for every rendering change.

## What landed

**`6a16ac8` — blue-noise jitter defaults, tuned per kernel.** Error diffusion
on smooth content locks into a limit cycle instead of staying stochastic,
producing a herringbone weave over flat areas and solid lines drawn clean
across gradients. Atkinson shipped with the jitter **off** (0.0).

The optimum tracks **kernel width**, and it is not "turn it up everywhere" —
the jitter is clamped to the right/below weights, so on a narrow kernel a large
scale saturates that clamp and degenerates into a deterministic toggle:

    sierra-lite (3 neighbours)   optimum 2.0, degrades to 0.0120 by 24 — kept at 2.5
    floyd-steinberg (4)          optimum 8.0, degrades after            — 4.0 → 8.0
    atkinson (6)                 0.0384 → 0.0363                        — 0.0 → 8.0
    burkes, sierra-2row (7)      0.0132 → 0.0125                        — → 16.0
    stucki, jjn, sierra (10–12)  still improving at 24                  — → 16.0

Wide kernels stop at 16, not 24: the remaining gain is ~0.0002 dE and 16 is the
largest value checked by eye for damage to thin strokes and text-scale detail
(there is none — a 1px stroke, a 1px checkerboard and text bars render as
crisply at 16 as at 0).

Both 6-colour panel presets pinned `sierra-lite: noise_scale: 5`, past its
optimum; removed so they follow the default. Same class of stale pin as the
`error_clamp: 0.11` removed earlier.

**`de9f605` — `make check` now covers the workspace.** See the warning above.

## The refuted family — do not re-derive

The diagnosis was sound and is unchanged: **greedy nearest-ink selection
answers the wrong question.** At 45° L0.32 the optimal mixture is
blk 47% / red 30% / yel 23%, and black is the *farthest* of the six inks
(0.617, dead last) while green — weight zero — is the nearest (0.165). The ink
the mixture needs most is the one greedy matching reaches last, so Atkinson's
25% error discard starves it and green fills in.

Three attempts to act on that, all refuted **on rendered output**, all with
excellent patch numbers:

| attempt | patch result | render result |
|---|---|---|
| restrict candidates to the mixture's support | worse (dE 0.0496 → 0.0616) — black still unreachable, red absorbs the slack | arcs unchanged |
| restriction **+ full propagation** | excellent — blk 1% → 42% (optimal 47%), blue held exactly at the bound | horizontal contours through gradients |
| soft bias (distance reduced by mixture weight) | best ever measured — dE 0.0050 at 45° | flat regions; "does not even try" reintroduced |

Mechanisms established, so they need not be retested:

- **Green was standing in for the missing black.** It is dark, so it was
  covering the luminance. Removing the symptom without curing the cause makes
  the patch worse.
- **Restriction is what makes full propagation safe** — black is not a
  candidate for a blue target, so it cannot be run to. Floyd alone collapses
  saturated blue to ~80% black; with restriction it sits at the bound.
- **Hard gating bands gradients intrinsically.** Support membership is binary,
  so an ink appears across a locus; in a vertical gradient those are horizontal
  lines. Refining the lookup 64 → 255 levels (8k → 120k entries) changes
  nothing.
- **Soft biasing has no usable λ.** ≤ 0.10 and blue still collapses; ≥ 0.20 and
  the dominant ink wins outright over whole regions and the output goes flat.
  Quantisation excluded here too.
- **Mixture weights are genuinely continuous** in the target (largest step
  0.090 down a constant-hue column). The premise was right; it was not enough.

Spike code is `crates/eink-dither/tests/spike_simplex.rs`, kept deliberately —
it is the record of what does not work.

## Open defects

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an
   optimal 47%. In gamut, bound 0.000, so no gamut excuse.
2. **Blue collapse under full propagation.** Not a live defect at the current
   defaults (Atkinson holds blue at 98–100%); it is the constraint that blocks
   adopting any 100%-propagation kernel.
3. **Scalloped arcs at ink-set boundaries.** Survives every kernel, every noise
   scale, and candidate restriction. **No working hypothesis.**
4. **Flat fills collapse to one ink** — a `#C06020` swatch renders solid red
   rather than mixing. Visible in the sharp-structure scene.

## The unlanded candidate

`AtkinsonHybrid` (100% achromatic / 75% chromatic propagation) beats Atkinson
on both axes: in-gamut mean 0.025 vs 0.038, out-of-gamut worst case 0.050 vs
0.072 (best of all nine). On the calibration photo Atkinson is **9.4% too
light** and the hybrid lands within 1.4%; the owner judged it "the best nuance
of all the photo samples". It does not fully fix defect 1 (black reaches 20%,
not 47%).

Changing the default alters rendering for every device — owner's call, not
taken.

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
- Local `podman build` needs `.dockerignore` to exclude `tools/` (~22 GB VM
  image). Already fixed.

## Diagnostics available

All `#[ignore]`, run with `-- --ignored --nocapture`:

- `test_dither_versus_gamut_bound` — the physical bound. ~82% of the 6-colour
  panel's hue error is the gamut, not the code.
- `test_algorithm_ranking_in_and_out_of_gamut` — all nine kernels, split by
  reachability, swept over saturation.
- `test_ink_histogram_versus_optimal_recipe` — which ink landed vs the recipe.
  Says *which way* a patch is wrong, which a dE cannot.
- `test_noise_scale_against_bound` — the sweep the defaults came from.
- `test_error_trajectory_decision_regions` — separates matcher faults from
  diffusion dynamics.
- `visual_compare` (integration test) — all the imagery.

## Settled — do not re-derive

- The diffusion maths is correct; in-gamut targets land at dE 0.004.
- ~82% of the hue error is the gamut. Flat blue at 225°–270° is optimal.
- Green really is the nearest single ink to dark olive (0.165 vs red 0.203).
  That is not a matcher defect.
- The gamut mapper is **not** a prerequisite for the dark-warm defect — those
  targets are in gamut, so the mapper is identity there.
- No gamut code has landed. The only gamut-aware code is `best_reachable()`, a
  test helper.
