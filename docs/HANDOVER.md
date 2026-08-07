# Handover — Byonk

_Last updated: 2026-08-07 — **Dependency security work is done and pushed. Two of three dithering defects are fixed and pushed. The third was re-measured and re-diagnosed this session: the earlier "Atkinson's 25% error loss" reading is wrong, and the algorithm swap it implied would cause a worse defect. It now needs an owner decision, not code.** `feat/screen-store-authoring-core` is still **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `999d35b` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` green, tree clean, pushed to origin |

## Next action

**Owner decision needed before fix 3 proceeds.** Fix 3 was re-measured this
session and re-diagnosed; the old diagnosis in this file was wrong. The
evidence is committed as three `#[ignore]` diagnostics at `999d35b`. See
"The remaining defect" below — read it before touching the ditherer.

Short version: do **not** swap the default algorithm, which is what the old
diagnosis implied. The measurement that looked like it justified a swap is
scored on a metric that is invalid for out-of-gamut targets, and the swap
would render saturated blues nearly black. The open question is whether the
gamut mapper is fix 3's prerequisite rather than something independent of it.

The gamut-mapping feature has an approved spec waiting:
`docs/superpowers/specs/2026-08-07-gamut-mapping-design.md`. Its
"Prerequisites" section lists these dithering fixes; two are done, and the
third may be reordered to depend on the mapper instead of preceding it.

## What happened this session

**Dependency security** (`633158d`, `857c96e`) — 27 of 28 Dependabot alerts were
one root cause, an outdated `gix`. Bumped 0.66 → 0.86, which needed the `sha1`
feature made explicit and a deprecated `peel_to_id_in_place`. Then `reqwest`
0.12 → 0.13 to collapse the duplicate copy gix pulled in.

The consequential part was TLS, and it was verified rather than assumed:
0.12's `rustls-tls` meant **ring + bundled Mozilla roots**; 0.13's `rustls`
means **aws-lc-rs + the host trust store**. `aws-lc-sys` builds C and needs
cmake, which ring did not — both cross musl images ship cmake and it compiles
clean for `aarch64-unknown-linux-musl`. On Windows/x86_64 it additionally needs
**NASM**, whose prebuilt fallback is opt-in only, and `windows-latest` has CMake
but no NASM — so `release.yml` now installs it. **PR CI is Ubuntu-only, so that
failure would only have appeared during the next release.**

The one remaining alert is `lru` (CVSS 0, an `IterMut` soundness lint). It
arrives via `usvg`, which pins `lru = "0.12"` in the `oetiker/resvg` fork. Not
fixable from this repo.

**`sierra-light`** (`f9b17f4` area, in `26b346d`..`904cf71`) — a misspelling of
`sierra-lite` that had spread into the admin API's advertised algorithm list and
both 6-colour panel presets. Nothing understood it: the renderer matches
canonical names and **silently falls back to Atkinson**, so a device configured
for it rendered with Atkinson *and* lost its per-algorithm panel tuning, with
nothing in the output to say so. Now an accepted alias, and
`resolve_render_params` canonicalises the effective algorithm once so the
renderer and the tuning lookup cannot disagree again.

**Fix 1 — exact-match pinning deleted** (`904cf71`~2). See below.
**Fix 2 — error_clamp semantics** (`904cf71`~1). See below.
**Calibrator regression fixed** (`904cf71`). See below.

## The dithering investigation — settled, do not re-derive

The user's report: a 6-colour panel renders most of the hue circle as flat
bands and "does not even try".

**The measurement that settled it.** A dithered patch's average is by
construction a convex combination of the palette's *actual* colours in linear
RGB, so the convex hull of those six colours is a hard bound on what **any**
error-diffusion algorithm can produce. `best_reachable()` in
`crates/eink-dither/src/domain_tests.rs` computes it by coordinate descent.

    bound (best any algorithm could do)   mean dE 0.097
    production at the time                mean dE 0.119

So **~82% of the error is the gamut, not the code**, and 77 of 144 targets were
already within 0.02 dE of the bound. Flat blue across 225°–270° genuinely is
optimal. That number is what stops anyone chasing the rest.

**The diagnostics are checked in** (all `#[ignore]`, run manually):

- `test_hue_gamut_sweep_patch_average` — measures what a patch *averages to*,
  not which entry won. The pre-existing `print_column_dominance` cannot tell
  "mixed red and yellow into orange" from "painted it all red".
- `test_dither_versus_gamut_bound` — the bound comparison above.
- `test_error_clamp_tradeoff` / `test_error_clamp_sweep_against_bound` — used to
  pick the new default.
- `test_in_gamut_targets_still_mix` — proves the diffusion maths is sound: a
  50/50 red+blue mix dithers to 33/31 with **dE 0.004**.
- `byonk-builtin/calibration/gamut` — the visual counterpart. Isolated flat
  patches over hue × lightness. Speckled = mixed, solid = gave up.

## Fix 1 — exact-match pinning deleted

Any pixel whose value exactly equalled an official palette colour was forced to
that entry and its error discarded. It keys off **pixel value**, which cannot
tell "the author filled this shape with palette red" from "this gradient passes
through palette red on its way from orange to magenta" — so ramps got a hard
seam. It also pinned pure `#00FF00` to the panel's dark green (L 0.56) when a
bright yellow-green mixture (L 0.87) was available and far closer.

**Removed, not repaired**, because it was buying nothing: a pixel already equal
to a palette colour has zero quantisation error, so error diffusion reproduces
it exactly with no special case. Measured: the new default output is
**pixel-identical** to the old `preserve_exact=false` render; against the old
default only 1.26% of pixels in a text block change, all antialiased glyph
edges, indistinguishable at 4× zoom.

Guarded by `test_ramp_through_palette_primary_has_no_seam`.

**Breaking:** the Lua `preserve_exact` key is gone, with
`preserve_exact_matches` / `exact_absorb_error` throughout, and `Preprocessor`
lost its palette parameter and lifetime.

⚠️ **Watch out:** the seam test must use a palette with *measured* colours. With
`actual == official`, pinning pure green is correct and the test passes
vacuously. It cost a false pass here.

## Fix 2 — error_clamp bounds the error, not the value

It used to clamp `channel + accumulated_error` into `[-clamp, 1+clamp]`.
Bounding the **value** makes headroom depend on where the channel already sits:
saturated colours are at an extreme by definition, so they were starved exactly
where mixing was needed, while neutral mid-tones got the most room. Now
`apply_error()` bounds the error itself.

Both metrics improve together — **there is no trade-off**, which was not the
expectation going in:

    clamp   muted max dE   gamut mean dE
     0.05         0.3098          0.1514
     0.2          0.1167          0.1240
     1.0          0.0555          0.1127
     2.0          0.0555          0.1103

**1.0 is the knee** and reads naturally: accumulated error may not exceed full
scale in a channel.

Per-algorithm `error_clamp` defaults (0.03–0.12) are replaced by that single
value — their variation was tuned under the old semantics and corresponds to
nothing now. `noise_scale` stays per-algorithm.

**Three places carried stale constants**, all fixed: `EinkDitherer::new`
hardcoded `error_clamp(0.08)` so `new()` and `.algorithm(Atkinson)` disagreed;
the greyscale override raising it to 0.6 compensated for the old semantics and
would now only *lower* it; and both 6-colour panel presets pinned 0.11, which
with the alias fix would finally have applied and hurt.

**Config migration:** `error_clamp` in `config.yaml` or a script now means
something different. Old values (~0.1) render flat. Docs updated.

## Calibrator regression — found and fixed

Fix 1 regressed the colour calibrator: its `#00FF00` patch came out **solid
yellow**, indistinguishable from the `#FFFF00` patch beside it. Correct
reproduction of *bright green* — the panel's green is a much darker `#0D876B` —
but useless for a screen whose job is showing each ink.

Fixed in the screen, no engine change: draw the **measured** colour, which is by
definition an exact palette entry and so quantises with zero error and comes out
as that ink alone. Label still shows the official value, since that is what an
author writes.

⚠️ **This is the general workaround** for "I want ink N exactly": draw the
measured colour, not the official one. There is otherwise no longer any way to
demand a specific ink. The gamut spec's SVG marker was to be the real answer.

## The remaining defect — fix 3, re-measured and re-diagnosed

Re-measured at `999d35b`. The defect is real and still open, but **the previous
diagnosis in this file was wrong and the fix it implied is a trap.** Do not act
on the old "Atkinson discards 25%, so switch algorithm" reading.

**The symptom, restated from the ink histogram** (`test_ink_histogram_versus_optimal_recipe`).
It is not under-mixing with black. It is a *spurious ink*: at 45°/L0.32, where
the bound is 0.000 and the optimal recipe is blk 47% / red 30% / yel 23%,
Atkinson produces **grn 41%** and only **blk 1%**. Green is not in the recipe
at all. At 30°/L0.20 it is grn 18%. Whatever is wrong selects green, and a dE
alone could never have said so.

**Why not to swap the default.** Widening the ranking to all nine kernels and
sweeping saturation (`test_algorithm_ranking_in_and_out_of_gamut`) first looks
like it settles it — with a representative 201 reachable / 231 gamut-limited
sample, Atkinson is **last on both axes**, and its supposed out-of-gamut
advantage was an artifact of scoring only fully saturated targets:

    algorithm             in mean  in worst | out mean  out worst
    floyd-steinberg         0.010    0.057  |   0.012     0.076
    burkes                  0.013    0.066  |   0.011     0.059
    jarvis-judice-ninke     0.017    0.066  |   0.011     0.057
    atkinson-hybrid         0.025    0.074  |   0.015     0.050
    atkinson                0.038    0.099  |   0.013     0.072

**The histogram vetoes that table.** On saturated blue (240°/L0.44, optimal
`blu:100%`), Atkinson renders **98% blue** and *every* 100%-propagation kernel
renders **70–87% black** — Floyd 87%, Burkes 83%, Jarvis 80%. A saturated blue
region comes out nearly black. Mean dE barely registers it, because against a
target that far outside the gamut solid blue and mostly-black score alike, so
**the metric ranks the kernels that destroy blues highest**. Swapping the
default on that table would trade a muted-orange defect for a much worse one.

⚠️ **Generalise this:** dE against an out-of-gamut target is a bad objective.
Rank on it only after the target is reachable.

**The likely shared root cause, not yet confirmed.** For an unreachable target
the residual error is permanent and one-signed, so it accumulates until it
trips a wildly wrong ink; Atkinson's 25% discard is a bleed valve that happens
to prevent the runaway. That predicts the two defects are one bug, and that
**the gamut mapper is the unblock** — mapping targets into gamut first removes
the permanent residual, which is what would make a wide kernel safe to adopt,
which in turn is what fixes the dark-warm case. Contrary to the old note here,
the mapper is not irrelevant to fix 3: it is plausibly its prerequisite.

Confirm that chain before building on it — it is a hypothesis with good
evidence, not a measurement. Unexplained: why *green* specifically wins at
45°/L0.32, a target that is fully in gamut and so has no permanent residual.
That one does not fit the runaway story and should be chased first.

Reproduce all of the above with:

    cargo test -p eink-dither ink_histogram -- --nocapture --ignored
    cargo test -p eink-dither algorithm_ranking -- --nocapture --ignored
    cargo test -p eink-dither gamut_bound -- --nocapture --ignored

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. **Pass `timeout: 600000`**
  — it exceeds the Bash tool's 120 s default and gets auto-backgrounded otherwise.
- `make docs` needs `mdbook-mermaid`.
- `cargo clippy -- -D warnings` **skips test targets**.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. Never `git add -A`.
- **Local `podman build` needs `.dockerignore` to exclude `tools/`** — fixed this
  session; `tools/ha-vm/work/` holds a ~22 GB VM image that killed the upload.
- **`Dockerfile` is broken independently** — it copies `Cargo.toml`, `Cargo.lock`
  and `src/` but never `crates/`, so the workspace cannot resolve `eink-dither`.
  Releases are unaffected (they use `Dockerfile.release` with CI-built binaries).
  Untouched as out of scope.

## Rendering a screen for visual checks

Fastest loop found this session — no server needed:

    CONFIG_FILE=<scratch>.yaml SCREENS_DIR=screens \
      ./target/debug/byonk render --mac "AA:BB:CC:DD:EE:01" --output out.png

with a scratch config defining a panel (incl. `colors_actual`) and a device
pointing at e.g. `byonk-builtin/calibration/gamut`. Screens are referenced as
`<handle>/<path>`. Then `Read` the PNG.

## Settled — do not re-derive

- The diffusion maths is correct. In-gamut targets land at dE 0.004.
- ~82% of the 6-colour panel's hue error is the gamut. Flat blue at 225°–270°
  is optimal, not a bug.
- Matching and error both use *actual* colours consistently (`find_nearest` over
  `actual_oklab`, error against `actual_linear`). No mismatch there.
- Everything from the previous handover about the measured-colour precedence
  chain, `/api/display`'s `use_actual=false`, and candidate prepending still
  holds.

## The thing that actually finds bugs here

**Measure it; do not reason about it.** Every real finding this session came
from a number or a rendered image, and three confident hypotheses died on
contact:

1. `error_clamp` starvation "refuted" — because the test hue was pinned by
   exact-match, masking the effect entirely.
2. Muted colours would trade off against saturated ones — they do not; both
   improve together.
3. The seam test passed on first write — the palette had no measured colours,
   so there was no seam to find.

Also: render the actual screen. The calibrator regression was invisible to the
whole test suite and obvious in one PNG.
