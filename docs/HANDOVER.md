# Handover — Byonk

_Last updated: 2026-08-07 — **Dependency security work is done and pushed. Two of three dithering defects are fixed and pushed. The third (Atkinson's 25% error loss) is diagnosed but not started.** `feat/screen-store-authoring-core` is still **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `904cf71` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` green, tree clean, pushed to origin |

## Next action

**Fix 3 of 3: the achromatic under-mixing defect.** Diagnosed, not started. Details below under "The remaining defect".

After that, the gamut-mapping feature has an approved spec waiting:
`docs/superpowers/specs/2026-08-07-gamut-mapping-design.md`. Its
"Prerequisites" section lists these dithering fixes; two are now done.

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

## The remaining defect — fix 3, not started

**Dark warm colours are under-mixed with black.** At 30°–60°, L 0.20–0.32, the
computed bound is **0.000** — exactly reproducible from black plus red/yellow —
yet production missed by 0.05–0.09. No gamut excuse.

**It is not fixed by either change above**, and the gamut mapper will not touch
it (those targets are in gamut, so the mapper is identity there by construction).

**Strong evidence for the cause:** Floyd-Steinberg largely fixes it where the
clamp did not — 45°/L0.32 went 0.064 → **0.016**, 30°/L0.32 → 0.018. That is
the signature of **Atkinson deliberately discarding 25% of its error**
(6 entries of weight 1, divisor 8).

Re-measure before acting: those numbers predate both fixes. Run
`cargo test -p eink-dither gamut_bound -- --nocapture --ignored`.

**Careful:** the 25% loss *is* Atkinson — changing it makes it not-Atkinson.
The likelier move is reconsidering the default algorithm, not editing the
kernel. That is a taste decision for the owner, not a silent fix.

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
