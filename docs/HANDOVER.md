# Handover — Byonk

_Last updated: 2026-08-09 (session 7) — **STOP. Do not finish this feature.**
Tasks 1-13 are implemented and green, but session 7 measured the output and found
that gamut mapping **desaturates colours the panel can already render exactly** —
the panel's own inks keep only 40% of their chroma. The spec contains two
incompatible descriptions of the adaptation step and the code implements the one
that breaks the spec's own headline promise. **The next session's job is to
investigate that, not to write Task 14's docs.**
`feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| Last code commit | `2f4a2a6` — the adaptation diagnostic. Anything after it is docs only. |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` green, tree clean, byonk lib **449** (+1 ignored), eink-dither lib **194** (+19 ignored) |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — **now known to be self-contradictory, see below** |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

---

# ⚠️ THE FINDING — read this before anything else

## What the owner saw

Shown the Task 13 renders, the owner said the mapped output looked "very subdued,
especially the red and the blue and yellow from the panel's actual colors seem to
be virtually non existent". That observation is correct and it is not a matter of
taste. Measurement followed, and it is worse than "subdued".

## The measurement

`cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture`

Applied to the panel's **own measured inks** — colours that sit exactly on the
hull, `rho = 1.0`, perfectly renderable, needing no compression whatsoever:

| ink | before | after | chroma kept |
|---|---|---|---|
| red | `#B50303` | `#874D45` | 40% |
| yellow | `#FFEE00` | `#F1ECAF` | 40% |
| blue | `#205497` | `#435670` | 40% |
| green | `#0D876B` | `#5A7C70` | 40% |

Red becomes muddy brown. Yellow becomes pale cream. Blue becomes grey.

## The mechanism

`mapper.rs::mapped_chroma` computes:

```rust
compress_chroma(c / r.max(1.0), c_max, opts.knee)
```

The `/ R` happens **before** the knee is consulted, so it applies to every pixel
unconditionally. `R` is the content-adaptation factor: the 99th percentile of
`rho` over the marked region, capped at `max_compression = 2.5`. On saturated
content `R` pins at the 2.5 cap (measured `rho` p99 = **15.9** on the test field),
so every chroma in the region — in-gamut or not — is divided by 2.5.

Two consequences the next session must keep in view:

- **It is contagious.** One `R` covers the whole marked region. A single vivid
  element drags down everything marked alongside it. Marking a photo containing
  one saturated sunset desaturates that photo's neutral areas too.
- **It falsifies a claim this handover previously made.** Earlier versions of this
  document asserted "gamut mapping is the identity on in-gamut targets." That
  holds **only when the entire region fits** (`R <= 1`, `map_frame`'s early
  return). For mixed content it is false. That sentence has been corrected below.

## The spec contradicts itself — this is the crux

`docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` contains **both**
readings, and they cannot both be satisfied:

| Where | Says | Consistent with the code? |
|---|---|---|
| ~line 201 | "**Normalising by the capped `R`** simply leaves it above `Cmax` going into the knee" | **Yes** — this is what the code does |
| ~line 217 | "compression only bites above `k*Cmax`, so **low-chroma content passes through untouched however large `R` becomes**. A mostly-grey photo with one vivid flower does not go flat." | **No** — impossible once you divide by `R` first |
| ~line 233 | The per-pixel formula: `C <= k*Cmax : C' = C`, and `C > k*Cmax : ...`. **No `R` appears in it anywhere.** | **No** |

So the answer to "was this built on sound principles?" is: the *curve* was —
ACES 1.3 RGC `powerP`, ruling 2, and the `Cmax` table was validated against an
independent oracle in Task 7. **The adaptation step is where it went wrong**, and
the divergence was invisible because every test measures relative properties
(monotonicity, hue order, "differences preserved") and **no test ever asserted
that an in-gamut colour survives unchanged.**

## A concrete hypothesis for the fix — verify, do not assume

A formulation satisfying *both* spec statements: keep the sub-knee region exactly
identity, and let `R` set only how hard the **tail** is squeezed.

```
C <= k*Cmax :  C' = C                                    // untouched, any R
C >  k*Cmax :  t  = (C - k*Cmax) / ((R - k)*Cmax)        // R scales the tail only
               C' = k*Cmax + (1-k)*Cmax * shoulder(t)
```

Under this, a palette ink at `C = Cmax` with `k = 0.8` moves only within the top
20% of its range instead of losing 60% of its chroma, and a grey pixel is
genuinely untouched however vivid its neighbours are — which is the spec's stated
promise. **This is an unverified sketch. Measure it; do not trust it because it
appears here.** In particular check what it does to the Task 7 oracle bounds and
to the monotonicity property, which is the one thing that must not break.

## Why the knee sweep looked inert — same root cause

Session 7's knee sweep (0.4 / 0.6 / 0.8) barely moved the image, and the owner
spotted it before the controller did:

| | mean saturation (max−min channel) |
|---|---|
| unmapped | 132.3 |
| knee 0.4 | 85.6 |
| knee 0.6 | 86.5 |
| knee 0.8 | 87.1 |

**The knee spans 1.7%; the mapping itself costs 35%.** 40% of pixels differ
between knee variants, but that is dithering noise rearranging, not a visible
change. Because `R` has already pushed 73–99% of pixels into the compressive tail,
the curve approaches `Cmax` regardless of where the bend sits.

`max_compression` is the knob that controls the appearance; `knee` is not:

| max_compression | R | mean Oklab chroma |
|---|---|---|
| source | — | 0.1752 |
| **2.5 (current)** | 2.500 | 0.0461 |
| 2.0 | 2.000 | 0.0506 |
| 1.5 | 1.500 | 0.0554 |
| 1.2 | 1.200 | 0.0583 |

This is uncomfortable: **ruling 4 was a whole debate about knee 0.6 vs 0.8 — a
1.7% effect — while `max_compression = 2.5` sat in the "never challenged" list.**
Treat ruling 4 as settled-but-irrelevant until the adaptation is resolved.

## Three options, none of them chosen

1. **Lower `max_compression` toward ~1.2.** Cheap, no redesign, partial — the
   in-gamut colours still get divided, just by less.
2. **Redesign the adaptation** so normalisation applies only above the knee (the
   sketch above). The real fix. Likely invalidates parts of the Task 4-7 work and
   every constant tuned against the current curve.
3. **Accept and document** that marked regions must not mix vivid and in-gamut
   content. Cheapest, but it makes the feature much narrower than advertised, and
   the documentation in Task 14 would have to say so plainly.

**The owner has not chosen. Do not pick one unattended.**

---

## How to resume

1. Read this file, then `git log --oneline -8` for anything after the diagnostic.
2. Run the diagnostic — it prints every number quoted above:
   `cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture`
3. Read the spec's "Content adaptation" and "Per pixel" sections (~lines 185-245)
   **in full** and judge the contradiction yourself before accepting the framing
   above.
4. Then talk to the owner. This is a design decision, not an implementation task.

**Do not start Task 14** (docs + `CHANGES.md`). Documenting a feature whose
central behaviour is under question would mean writing user-facing text that may
be wrong. The plan's task list is not the authority here; the finding is.

`crates/eink-dither/tests/gamut_adaptation_diag.rs` asserts the headline finding
(red ink keeps <50% chroma). **If a redesign lands, that assertion should start
failing** — that is deliberate, and the test's own message says so.

## ⚠️⚠️ Read this before dispatching any subagent

**`make check` exceeds 600 seconds in this tree.** The subagent stream watchdog
fires at 600 s of silence, so **an implementer that runs `make check` in the
foreground dies mid-run.** This cost session 6 two dead dispatches and ~20 minutes.

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets`
  then `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib`. Say so in the brief.
- **The controller runs the full gate**, in a **backgrounded** Bash call
  (`run_in_background: true`), and polls.

When an implementer stalls, **do not resume it blindly a second time** — assess
the abandoned working tree first. Session 6's second resume produced nothing
because the cause was environmental, not a model failure.

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
| `2f4a2a6` | **The adaptation diagnostic** — evidence for the finding above |

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`; `gamut::knee::compress_chroma`.
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey}` — import, never copy.

**The feature is end-to-end live** but reaches nothing: it applies only where an
SVG marks a region `data-byonk-tone="continuous"`, and **no shipping screen does**.
That is the one piece of luck here — the finding above affects no rendered output
today, so there is no urgency and no user impact. Keep it that way until the
design question is settled.

## Open owner decisions

**1. The adaptation finding above.** The big one.

**2. The gamut calibration screen's marker — the owner has already given the
answer, it is not yet built.** Asked whether
`screens/builtin/calibration/gamut/screen.svg` should keep
`data-byonk-tone="continuous"`, the owner's answer was better than either option
offered: *"for a test screen I would suggest that it contains the same content
twice, once with marker and once without to show the difference."*

Two shapes were put to the owner and **no reply came before the session ended**:

- **Split cells** — each patch halved, left unmapped / right marked. Best
  comparison (no eye travel), labels unchanged; but the tone-mask boundary then
  runs through all 144 patches, so error diffusion bleeds across each one.
- **Stacked grids** — six raw rows above six marked rows. One boundary instead of
  144, cleaner diffusion; but halves patch height and separates the comparison.

`screen.svg` is currently **unmarked and clean** — session 7's renders were made
by applying the wrap, rendering, and reverting.

## ⚠️ The lesson, now proven seven sessions running

**The plan's code and constants are not evidence.** Measure before believing the
plan, your own diagnosis, a reviewer's "harmless", a reviewer's "correct" — **or
the spec.**

Session 7 is the sharpest case yet, because the process worked and still missed it:

- **Every task passed review. The feature was still wrong.** Thirteen tasks, each
  reviewed, each re-verified by the controller, `make check` green throughout —
  and the headline behaviour was broken the whole time. **What saved it was the
  owner looking at a picture and saying "the colours look wrong".** No amount of
  additional test discipline would have caught this, because every test measured
  *relative* properties and none asserted an *absolute* one.
- **Ask what the tests do not assert.** Monotonicity, hue order and "differences
  preserved" were all tested. "An in-gamut colour comes out unchanged" was not,
  and that is precisely the property that failed. When a suite is all-relative,
  find the absolute claim nobody wrote down.
- **A self-contradictory spec reads as authoritative.** Nobody noticed lines 201
  and 217 disagreeing across six sessions of implementation. When a spec states a
  property in prose *and* gives a formula, check that the formula has the same
  variables in it.
- **A test can pass for the right reason and still guard nothing.**
  `test_gamut_mapping_preserves_local_contrast` went on passing with
  `compress_chroma` replaced by pure clipping — the exact failure it claims to
  detect — because its ramp normalised by `r = 2.0` and never reached the
  shoulder. Fixed in `e0d85b7`. **Mutation-test the assertions you inherit.**
- **Say "I verified" only after verifying.** That test was reported to the owner
  as "the one with teeth" on the strength of *reading* it. Same session, the
  controller called knee 0.4 "clearly flattest"; measurement put the whole sweep
  at 1.7%. Both were retracted. Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.** Sessions 6
  and 7 found 4, 3, 9 and 3 defects. Task 13's included a CLI invocation that does
  not exist and test code that violated ruling 5 and panicked.

Session 6's still-true additions: a green suite proves nothing about a site the
compiler cannot reach (`CachedContent::with_tuning` copies fields by hand;
deleting one left all 448 tests passing); grepping for a sibling field finds
*fields and literals*, not hand-written *copies* between structs, and copies are
the real hazard.

## Fourteen standing rulings — carry these forward

> **Provenance matters here.** Rulings **1-9** are genuine owner rulings.
> **10-12** were made in session 6 by task reviewers and the controller **while
> the owner was absent** — do not present them as settled. **13-14** are genuine
> owner rulings from session 7.
>
> **Ruling 4 is now in question** — not because it was decided wrongly, but
> because the finding above shows the knob it concerns barely matters.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. **Knee default 0.6 → 0.8** (`3fd9ab8`). **Measured at a 1.7% effect in session 7 — see the finding. Revisit only after the adaptation question is settled.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). `linear_to_srgb`
   has an epsilon-free `debug_assert!` — unclamped panics under `cargo test`,
   behaves identically in release. **Global Constraint.** Session 7 tripped it
   twice, both times in *test* code.
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`, `BEYOND_LIMIT_MIN_RATIO = 0.3`.
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too** (`ba8859c`).
8. **Task 9b — the mask must not invent a stroke** (`297b10a`). Stroke-evidence stack.
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** A script writing
    `gamut = { knee = "loud" }` gets `None` with no diagnostic, exactly as
    `error_clamp`/`noise_scale`/`chroma_clamp`/`strength` already behave. If ever
    fixed it must be **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
    Ruling 14 is that decision carried out.
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
    No gamut query parameters; adding URL surface is new scope.
13. **Amendment B confirmed — the CLI is gamut-aware** (owner, session 7).
    Task 12's plan text said `main.rs`'s `cli_tuning` "gets `gamut: None`", but
    those locals are **not** CLI arguments — they carry the *resolved*
    script > device > panel chain. Following the plan literally would have made
    gamut the one knob `byonk render` silently ignores. **The only place plan
    text was overruled.**
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
    Negative `amount` inverted the correction into a chroma *boost*; `> 1`
    desaturated past the target. Two tests, both watched RED first.

Standing: **the branch is HELD** — no PR, no merge to `main`.

**Constants inherited from the plan and never challenged:**
`PERCENTILE = 0.99`, `MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`,
`C_SEARCH_HI = 0.5`. — `max_compression = 2.5` **has now been challenged**; it is
the dominant knob and the subject of the finding above.

## Deferred minors — triage list for the final whole-branch review

Session 7:

- `test_gamut_mapping_preserves_hue_order` would also pass against an **identity**
  mapper — hue preservation is all it asserts. Weak guard, kept deliberately.
- The `gamut-renders.html` comparison artifact built for the owner lives only in
  the session scratchpad; the PNGs it embeds are in `target/dither-compare/` and
  are regenerable. Nothing durable was lost.

Session 6:

- **Task 10:** the unreachable mask-length-mismatch branch returns
  `RenderError::Dither`, a misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)`
  **cannot** detect a restated-constant violation — manual-review-only.
- **Task 11:** `PanelDitherConfig` accepts a `gamut:` key in panel YAML; verify it
  is live now that Task 12 has landed.
- **Task 12 (inherited):** `resolve_effective_tuning` replaces the **whole** struct
  when any override field is set, so an active dev-UI query override resets the
  previewed gamut to default and diverges from production. Symmetric with the
  other four knobs.

Earlier sessions:

- **Task 7:** the winning dilute start was `eps = 0.005`, between the shipped
  ladder's `0.003` and `0.01`. Optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`;
  traced — failure mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()`
  while `rewrite_start` propagates them. Style wart.
- **Task 8:** element names matched as raw bytes, so `<svg:image>` would be
  mis-handled and `<symbol>` gets no `<defs>`-style stripping. Dormant. **Same
  case-and-name matching family as the `viewBox` near-miss — worth one pass.**
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`.
- **Task 9b:** `resolve_stroke` cannot see stylesheet-only strokes; that element
  under-marks. Deliberate, documented.
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three
  epsilons want a comment; `adapt.rs`'s `max_compression < 1.0` collapse is
  untested; no test exercises literal `NaN`.

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour; every
artifact that matters is at a boundary *between* colours. In the previous
initiative, every arm that improved patch dE made the rendered image worse.
Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**And now the converse, from session 7: looking is not sufficient either.** The
controller looked at the renders, described them accurately, and still drew the
wrong conclusion about the knee — because "which of these three is more saturated"
is not a judgement the eye makes reliably at a 1.7% difference. **Look to find
what to measure; measure to decide.**

**IDE diagnostics lie in this tree.** Session 6 saw them report `missing field
gamut` in a file that compiled cleanly. Verify with an actual `cargo` run.
Equally, **never take a subagent's "all green" at face value**.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Exceeds 600 s — background it.**
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **byonk lib suite is 449 tests** (+1 ignored); eink-dither lib **194** (+19
  ignored). Re-measure, don't inherit.
- `make docs` needs `mdbook-mermaid`.
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
  which looks like a gamut result and is not one. Session 7 hit it.
- Regenerate the Task 13 imagery with
  `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`;
  the metrics with `cargo test -p eink-dither test_gamut_mapping -- --ignored --nocapture`;
  **the finding** with `cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture`.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the
  workspace cannot resolve `eink-dither`. Releases unaffected
  (`Dockerfile.release`, CI-built binaries). Out of scope, untouched.

## Open dithering defects — independent of this work

Gamut mapping is the identity only when a marked region's content **entirely**
fits the gamut (`R <= 1`), so it does not reliably fix or worsen these:

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale. **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. An unmapped
   100×100 `#ff00aa` frame dithers to a PLTE of a single colour.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does
not work. `AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on
both axes — changing the default alters rendering for every device, so it is the
owner's call.
