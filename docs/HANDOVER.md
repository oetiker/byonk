# Handover — Byonk

_Last updated: 2026-08-12 (session 14). **THE AMENDED PLAN IS COMPLETE — all 8 tasks done,
every one reviewed clean.** Task 8's measurements are in and they answer the three questions
the plan deliberately left open. Full `make check` green at `6a4e2b1` (1061 passed, 0 failed,
0 warnings). `feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to `main`.
**Next: the final whole-branch review — read the scoping warning below before dispatching
it — then `superpowers:finishing-a-development-branch`.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `6a4e2b1` — Task 8 fix round 1 |
| State | tree clean; **full `make check` PASSES: 1061 passed, 0 failed, 51 ignored, 0 warnings** |
| Pinning Task 1 | `c74312f`..`f82eedc`, reviewed clean |
| Pinning Task 2 | `89c2069`..`24ce479`, reviewed clean (2 fix rounds) |
| Amended Task 3 | `7c09875`..`574d8c5`, reviewed clean (1 fix round) |
| Amended Task 4 | `3711dea`..`0d73dda`, reviewed clean (1 fix round) |
| Amended Task 5 | `d9d7fe8`..`55130f3`, reviewed clean, 3 minors deferred |
| Amended Task 6 | `55130f3`..`e53e062`, reviewed clean, 2 minors deferred |
| Amended Task 7 | `e53e062`..`9e1493b`, reviewed clean, 5 minors deferred |
| **Amended Task 8** | **`c57e1e8`..`6a4e2b1`, reviewed clean, 0 open** |
| Out-of-plan (session 14) | `4eef93e` calibration patch fix, `c57e1e8` authoring docs |
| Active spec | `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` |
| Active plan | `docs/superpowers/plans/2026-08-11-panel-colour-pinning-amended.md` — **all tasks done** |
| Active ledger | `.superpowers/sdd/2026-08-11-panel-colour-pinning-amended/progress.md` (git-ignored) |
| Task 8's numbers | `.superpowers/sdd/2026-08-11-panel-colour-pinning-amended/task-8-report.md` (git-ignored) |
| Prior initiative | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — complete |

**Resume by:** reading the ledger's deferred/parked roll-up, then dispatching the **final
whole-branch review** — but read the scoping warning first, because the obvious command is
wrong here.

# ⚠️⚠️ START HERE — three things the next session must not get wrong

## 1. The "whole-branch review" cannot be the whole branch

`git merge-base main HEAD` is `67b3855`, and `67b3855..HEAD` is **261 commits, 178 files,
48,803 insertions** spanning three weeks and several unrelated initiatives (screen-store
authoring core, gamut mapping, pinning). The SDD skill's `review-package PLAN_FILE
MERGE_BASE HEAD` would produce a package nobody can review and a reviewer that finds nothing.

**Scope it to this plan's work.** The pinning initiative is `c74312f^..HEAD` — **21 files,
5,518 insertions**, a real reviewable package. Use that as MERGE_BASE, say so in the
dispatch, and state that earlier initiatives on the branch are out of scope and already
reviewed.

**Point the final reviewer at the ledger's `minor (deferred)` and `parked` lines** — there
are 12 of them across Tasks 3-8 plus older ones listed at the bottom of this file. That
roll-up is the reviewer's input and **nobody else has read it**. A roll-up nobody triages is
a silent discard.

## 2. The owner's three decisions are now backed by numbers

The plan's "After Task 8" deliberately does not make these. **Present them; do not decide
them.** Full detail in `task-8-report.md`.

**Decision 1 — λ's shipping value.** `pin_carry = 0.9` is supported but **indistinguishable
from 0.95 and 1.0**. More importantly: in the production configuration (structure over a
marked photo) **containment leaves no error for λ to attenuate, so λ is INERT there.** The
plan's chosen metric for this sweep ("line ink purity") turned out to be a theorem — see
below. 0.9 can ship unchanged; the honest framing is that the sweep found no reason to
prefer any value in [0.9, 1.0].

**Decision 2 — the unmarked-photograph cost. It is large, and it survived correction.**
The decision-carrying evidence is the **ink histograms**, not ΔE: **75.2% vs 56.2% black** on
`photo.png`, **69.1% vs 51.8%** on `background.jpg`, with 38-40% of pixels changing ink. An
unmarked photograph is not slightly worse, it is visibly darker. Block-averaged ΔE is 0.107
at k=16 and **barely decays**, i.e. the darkening is spatially persistent rather than
halftone noise, and it is **roughly 12x the gamut mapper's own effect** — the thing this
project already paid 218 ms/frame for.

**Decision 3 — is `calibration/tone`'s unmarked control column still legible enough** to
serve as a control now that it renders under the nominal model? **Still unmeasured** —
the brief scoped it out (no crate-level fixture) and Task 8 did not cover it. **This is the
one open question the measurement pass did not answer.** It needs an eye on a render, not a
diagnostic.

**Decision 4 (authoring docs) is DISCHARGED** — done this session in `c57e1e8`. Task 8's
report and its re-reviewer both list it as outstanding; **they predate the commit. Do not
re-do it.**

## 3. Two plan defects were verified real — the tally is now 13

**Step 1's metric could not vary.** "Line ink purity" is 100% at every λ **by construction**:
`builder.rs:252-262` sets `pinned[i]` by exact byte match against `palette.official(i)`, and
`dither/mod.rs:387-395` writes `output[idx] = ink` **unconditionally**, using `pin_carry` only
to scale the *forwarded* error. λ cannot reach the pinned pixel's own output. A theorem, not
a measurement.

**Step 3's seam did not reproduce.** Ruling 23 drops error at a model boundary and the plan
expected a visible discontinuity. There is none. The discontinuity appears in the
**no-boundary control**; containment removes it. The only boundary-attributable effect is a
3-4 px onset ramp on the unmarked side, **milder than the frame edge's own transient on the
same colour**.

Both were **reported as plan defects rather than worked around**, and both were then verified
independently against the code by the reviewer rather than accepted. That is the loop working.

# ⚠️⚠️ NEW METHOD RULES FROM SESSION 14 — apply these to every future measurement

## Block-average before comparing two dithered images

**This is a companion to the standing "measure in linear light" rule and it is just as
load-bearing.** `mean_de` compares per pixel, where every pixel is **one hard ink**. Two
halftones of the same image decorrelate their patterns and score a large per-pixel ΔE **even
when they are visually identical**.

| pair | k=1 (per-pixel) | k=4 | k=8 | k=16 |
|---|---|---|---|---|
| photo: unmarked vs marked_mapped | 0.1773 | 0.1224 | 0.1116 | **0.1066** |
| photo: marked_raw vs marked_mapped | 0.1144 | 0.0368 | 0.0160 | **0.0077** |
| background: unmarked vs marked_mapped | 0.1770 | 0.1235 | 0.1062 | **0.0975** |
| background: marked_raw vs marked_mapped | 0.1284 | 0.0418 | 0.0165 | **0.0088** |

The mapper pair **collapses by 15x**; the unmarked pair barely moves. ~0.11 of the mapper
pair's score was pure pattern noise. **A per-pixel mean over a halftone measures pattern, not
appearance.** `block_de` in `domain_tests.rs` does this correctly; k=1 reproduces `mean_de`
exactly, which is what validates it.

Note the conclusion **survived and strengthened** under correction — the honest statistic made
the case stronger, not weaker. That is the usual outcome and worth remembering when a
correction feels like it will cost you your result.

## A wall-clock benchmark in a test binary is only valid with `--test-threads=1`

Task 8's cost diagnostic used `Instant`, and the report's own run command ran it
**concurrently with four other 800×480 diagnostics**. Under that exact command the reviewer
measured the **baseline slower than the feature** (32.94 ms `None` vs 31.62 all-continuous) —
**the headline's sign flipped.** Isolated, the numbers reproduce within ~0.6 ms across three
runs. The recipe must state the condition; the numbers alone are not the result.

## The eighth time an owner looking at a picture beat the test suite

The owner looked at a fresh render and said the `calibration/color` patch row "should not
show a dither … these are the pure panel colors". **Confirmed by measurement**: patch
interiors were 57/43 red-black, 48/38/11 black-blue-green. Cause — `script.lua` filled the
patches from `device.colors_actual`, which ruling 22 turned into a non-matching value on the
unmapped path.

**Tasks 5, 6 and 7 all reviewed CLEAN and the full gate was green while a shipped calibration
screen was visibly broken.** No test asserted on that row. **Ruling 22's blast radius is
"every screen that paints a measured value", and nothing in the suite covers that class.**

The fix carries a trap worth not re-deriving: the patch label's contrast colour must stay
**measured** while the fill goes **nominal**, because the patch is filled nominal but
*renders* as the measured ink. Judging contrast on the fill puts black text (nominal
`#00FF00`, lum 182) onto the green ink (`#0D876B`, lum 107).

## `--use-actual false` is a free second view of any render

`render --mac <MAC> --use-actual false --output <PATH>` draws the PNG in the **nominal**
palette — the actual wire format, indexed, exactly 6 colours. Byte-identical in *index* to the
measured render, so it costs nothing, and it is a **better lens for spotting stray pixels**: a
single wrong index shows up as a screaming primary rather than a subtle speckle.

Renders from this session: `~/byonk-screens-9e1493b/` (measured, plus `color-fixed.png`) and
`~/byonk-screens-9e1493b/as-sent/` (nominal).

# The design as built (all of it is now live)

**The unmapped path assumes the actual colours ARE the nominal colours.** One mask selects
three things at once:

| Region | Colour model | Gamut mapping | Pinning |
|---|---|---|---|
| **Unmarked** (structure) | **official/nominal**, substituted for actual | off | **on**, against official |
| **Marked** `continuous` | **actual/measured** | on | **off** |

`data-byonk-tone="continuous"` is the attribute. `RegionMap { continuous, pinned }` carries
the mask inside the crate; `EinkDitherer::dither_with_regions(pixels, w, h, continuous:
Option<&[bool]>)` is the public entry point.

- **`continuous` is the tone mask itself, NOT its inverse.** `true` = marked = measured
  model, mapped, not pinned. A flipped polarity is silent and produces a plausible image
  either way.
- **`None` means the feature is OFF** — measured everywhere, bit-for-bit today's output.
  Guarded by `regions_none_reproduces_the_measured_unpinned_output_exactly`.
- **An all-`true` mask is bit-identical to `None`** — asserted by
  `an_all_continuous_mask_is_bit_identical_to_no_mask`, with a non-vacuity guard. This is the
  cheapest polarity check available; keep it.
- **Pin eligibility is resolved by the CALLER** and is not re-checked in the dither loop.
- **`error_clamp` does not bound the pinned carry.** Inert at the live default — it is
  uniformly `1.0` for every algorithm (`dither/mod.rs:118`). Deliberate, not an omission.

## What shipped, measured

- **The tone grid**: 2 px black lines between saturated patches went **73.58% → 100.00%**
  black (9,734/9,734 px). This was the original defect.
- **`builtin/default`'s swatches**: dominant ink flipped from **67% yellow → 67% teal-green**
  for `#00FF00` and from **65% black → 68% blue** for `#0000FF`.
- **⚠️ But Step 4 found the headline is the COLOUR MODEL, not the pin.** An unpinnable
  "one byte off" control also reaches 100%, so on a large flat patch nominal matching alone
  delivers the win. Pinning still owns the original defect — black lines *inside* saturated
  content — which is where diffused error, not matching, is the mechanism.
- **Also from Step 4:** today's *shipping* arm (marked + mapped) was already 94.7%/96.5%.
  The 99.9%-yellow catastrophe in the old evidence belongs to the **unmapped measured model**,
  not to what shipped.

## Containment (ruling 23), as measured in both orientations

**No seam, either way.** And the *harder* orientation is cleaner, structurally rather than by
luck: `kernel.rs` confirms **every Atkinson/FS/JJN/Sierra entry has `dy >= 0`** — there are no
upward taps at all. So nothing below a **horizontal** boundary can influence anything above
it; rows above are bit-identical to the no-boundary control. The vertical case has sideways
exposure on one band via Atkinson's `(-1, 1)` tap; the horizontal case has **zero** exposure
to the marked side by kernel geometry alone.

**Atkinson has divisor 8, `max_dy` 2, entries including `(2,0)` and `(-1,1)`** — max horizontal
reach |dx| = 2, and **error does travel left**. A session-14 report claimed otherwise and was
corrected.

# ⚠️⚠️ THE FIXTURE TRAP — still the highest-value warning in this file

**Several palette helpers in `eink-dither` are built with `Palette::new(x, None)`, which sets
`actual = official` (`palette.rs:167`). Under any of them the two colour models are
IDENTICAL, so a test written against one passes against every mutant, silently — and a
*measurement* reports zero difference while looking like a clean result.**

`dither/mod.rs`'s own test module has two — `pin_test_palette()` and `panel_palette()` — and
they are what a fresh implementer reaches for by default. **That is the trap.**

The only fixture whose official and actual sets genuinely differ is
`crate::gamut::test_support::panel_measured()` (`gamut/mod.rs:40`). Probe indices 2-5
(red/yellow/blue/green); **black and white are degenerate even there.**

**It is `#[cfg(test)] pub(crate)`, so integration tests in `tests/` CANNOT see it.** This
nearly sank Task 8 — the plan put the diagnostic in `tests/`, where every measurement would
have read zero difference between the colour models. Resolution: the whole diagnostic went in
`src/domain_tests.rs`. `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` are existing
hardcoded copies; **exporting `test_support` is the fix if a fourth appears.**

Tasks 3, 4 and 8 all had their fixtures audited measurement-by-measurement, which is why
their numbers are trustworthy. **Put this in every remaining dispatch verbatim.**

One legitimate use of degenerate black: the pinned branch writes the ink index directly and
never calls `find_nearest` or `representative_linear`, so a pinned pixel consults **no** model
and a test about the *carry* may use black freely.

# ⚠️⚠️ Thirteen plan-authored tests have measured unfounded

**Still the single most important thing in this file.** Every one was caught only because an
implementer or reviewer refused to accept the plan's premise.

Tally: **4 (Task 1) + 5 (Task 2) + 1 (Task 3) + 1 (Task 4) + 2 (Task 8)** = 13.

**The rules now in force:**

- **A test claiming "X rescues this case" must assert, in the same test, that the case needed
  rescuing.**
- **A comparison test must assert its comparison is non-degenerate.**
- **A doc comment asserting a mutation or invariant property is an unverified claim.** Five
  instances so far — including once in code written to fix the first instance.
- **A test must be able to attribute its result to the mechanism it names.** Non-degeneracy is
  not enough; ask what *else* could produce the same pass.
- **A mutation-table row can describe an IMPOSSIBLE mutation.** Saying so is the correct
  response; inventing a code path to satisfy the table is not.
- **NEW (Task 8): a plan can name a metric that cannot vary with the parameter it is meant to
  tune.** Check the metric against the mechanism before running the sweep.
- **Write one mutant per site.**

**Corollary for any future brief:** put the plan's test bodies in as *hypotheses to measure*,
and say so explicitly in the dispatch. Tell the implementer that a mutant surviving its named
test is a plan defect to report, not a value to tune. **That single instruction has caught all
thirteen.**

# ⚠️ Pre-flight has never once been clean — 14 sessions running

Session 14's Task 8 pre-flight found 2 Important + 2 Minor, and **both Important ones would
have invalidated the entire task**:

- the `tests/` location could not see the only non-degenerate fixture (above);
- `RegionMap` is `pub(crate)`, so the plan's Step 5 wording was not expressible — and the
  failure mode is an implementer **widening production visibility to make a diagnostic
  compile.** Forbid that explicitly.

Generalising from earlier sessions:

- **A plan step that says "pass `None` here so it compiles" can silently disable a feature an
  earlier task's tests assert.** Grep for what asserts the behaviour being placeholdered.
- **Check whether a mutation-table row is reachable at all** before dispatching.
- **A stale baseline in a brief is not a baseline.** Task 8's Step 4 quoted "51% black" for the
  green swatch; re-measuring gave 67.3% yellow. Different pixel sets, neither wrong — but
  re-derive the before-value in the same harness that produces the after-value.

# ⚠️⚠️ Read this before dispatching any subagent

**`make check` takes ~10 minutes in this tree.** The subagent stream watchdog fires at 600 s of
silence, so **an implementer that runs `make check` in the foreground dies mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` then
  `CARGO_BUILD_JOBS=2 cargo test -p <crate> --lib`. Say so in the brief.
- **The controller runs the full gate** in a **backgrounded** Bash call
  (`run_in_background: true`). Foreground `sleep` is blocked.
- **Redirect the gate log to a file; never pipe it through `tail`** — you lose the counts and
  have to re-run.

**Subagent task briefs scoped to `--lib` cannot see integration tests.** The controller's full
gate is not a formality.

**Tell the implementer the pre-existing failures.** `cargo test -p eink-dither --lib --
--ignored` reports **3 pre-existing failures unrelated to any current work**:
`preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
test_resize_full_pipeline_with_photo_preset}`, which panic at `resize_lanczos` **by design**.

**Give the reviewer the cross-task risk to check, by name.** Task 8's reviewer was given five
named risks and **independently reproduced every number in the report** rather than accepting
it — catching that the headline used the wrong statistic. **A named risk earns its cost;
"check all uses" does not.**

**Hand deviations to a reviewer as "judge these on their merits", never as "these were
approved."** Task 8 declared four; all four were upheld *with mechanism*.

**Never pre-judge a finding for a reviewer.** If the prompt contains "do not flag" or "at most
Minor", stop — you are sparing yourself a review loop at the cost of the review.

# ⚠️ This build cannot resample (eink-dither only)

`resize_lanczos` **panics on any real dimension change** — no `image` backend is compiled into
`eink-dither` proper. Same root cause as the three pre-existing failures.

**But `image` IS a dev-dependency**, so test code may resize with
`image::open(...).resize(..., FilterType::Lanczos3)`, as `tests/visual_compare.rs` does at
lines 400-404 and 716-717. Never route test images through `Preprocessor`. `image` is also a
real dependency of the **byonk** crate (`Cargo.toml:17`).

# ⚠️⚠️ Read this before trusting any dithering picture

**Every visual dither comparison in this tree reads about 30% too dark, and it is the viewer's
fault, not the ditherer's.**

| | mean LINEAR luminance | mean GAMMA-SPACE byte |
|---|---|---|
| portrait | **+10.2%** vs source | −32.4% vs source |
| background | **+4.4%** vs source | −29.3% vs source |

Error diffusion preserves brightness in linear light. A viewer downscaling a PNG without
linearising averages sRGB bytes directly, under-weighting bright pixels in a black/ink speckle.
On the panel the eye averages optically, in linear light.

- **Relative** comparisons between variants remain valid. Use `-mapped` (undithered) renders
  for absolute judgements.
- **Open defect 1 below was diagnosed this way and should be re-measured in linear light
  before anyone chases it.**

# ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading** — a flat patch is a single colour; every artifact that
matters is at a boundary *between* colours.

**Whole-image means are equally misleading.** On the portrait all four gamut anchors scored
0.0545–0.0550 mean chroma and looked identical, because only 7% of pixels are out of gamut and
the untouched 93% swamped them. Restricted to the pixels the mapper acts on, the spread was 68%
to 90%. **Measure the pixels the change touches.**

**And block-average before comparing halftones** — see the session-14 rule above.

**Look to find what to measure; measure to decide.** When comparing an old behaviour to a new
one, **render both from the same input in the same image**.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run. Never take a
subagent's "all green" at face value.

# The screen collection is marked (`fe66ee6`) — done, out of plan

Applies ruling 19 across all 13 shipped screens; **10 needed nothing**. Marked:
`builtin/default` (background photograph), `builtin/calibration/color` (each gradient bar, hue
sweep, photo), `examples/gphoto` (full-screen photo). `builtin/calibration/tone` was already
marked. `builtin/calibration/tone`'s grid rect left the marked group in Task 7.

**The argument for leaving an achromatic gradient unmarked is the one to remember.** Grey is
always in gamut, so mapping it is a *no-op* — while marking it switches exact-match pinning
*off* across a deliberate dithering test pattern. **Any "should this be marked?" question
should start by asking whether the content can even be out of gamut.**

**Marking goes on the element that IS continuous-tone, never on a group around a band.** On
`calibration/color` the gradient bar and its label come from the same loop body, so a `<g>`
wrapper would have swallowed the label.

### The mask rasterizes in document order, and that does real work

At 800×480: `calibration/color` marks **262,672 / 384,000** px; `default` marks **309,125 /
384,000 (80.5%)** — even though its photo is *full-screen*. Unmarked elements drawn **after** a
marked one paint black back over it, so `default`'s hero text, swatches and white info bar
punch themselves out of the photo's marked area. Those pixels are opaquely covered, so
excluding them is correct. **For authors: text over a photo needs no special handling as long
as it comes later in document order.** This is now documented — see below.

### Untracked duplicate — do not sweep it in

`/Users/oetiker/checkouts/byonk/examples/` is an **untracked** near-copy of `screens/examples/`,
drifted by one file (`gphoto/screen.svg`). Exactly the kind of local file that makes
`git add -A` dangerous here.

# Authoring documentation — DISCHARGED in `c57e1e8`

The standing obligation ("marking goes from optimisation to requirement under ruling 22") is
done:

- **`docs/src/tutorial/svg-templates.md`** gains "Marking continuous-tone content" — what the
  attribute does, the three things one mark drives, the two easy mistakes (mark the element not
  a group; leave achromatic gradients unmarked), and document-order behaviour.
- **`docs/src/api/lua-api.md`** — its `colors_actual` example **taught the measured-fill
  anti-pattern**. Corrected to "DECIDE with measured, PAINT with official", and the "dithering
  targeted the measured ones" paragraph now says that is true only inside a `continuous` region.
- **`docs/src/guide/dev-mode.md`** — "only it is gamut-mapped" corrected to the three things.

Verified with `make docs`: the anchor exists and both inbound cross-links resolve.

## Open owner decisions

1. **The three Task-8 decisions** — see the top of this file. λ, the photograph cost, and
   `calibration/tone`'s control column (the last is **unmeasured**).
2. **The branch.** Still HELD. **Fourteen sessions, 261 commits, 48,803 insertions unmerged.**
   This is now the largest outstanding risk in the initiative — not because anything is wrong
   with the work, but because the volume keeps growing and none of it has shipped.
3. **CHANGES.md** — ruling 21 defers one entry covering gamut *and* pinning together to merge
   prep. It has not been written.

# The prior initiative: gamut mapping (complete)

Rulings 16 and 17 are implemented, measured and committed. The mapper compresses along a ray
from mid-grey; knee default 0.99. All four measured panel inks come back at `t_max = 1.000`.

**Ruling 16 and ruling 17 are only safe together.** The ray's liability is near-white tints: a
high-`L`, low-chroma colour's ray exits the hull at the *white point*, so it reads as
boundary-saturated even though its chroma was never out of gamut. At knee 0.8 this darkens
`grey 250 tint 4` by **−0.084**; at 0.99 by −0.0035. **Do not lower the knee without
re-measuring** — `gamut::mapper::tests::ray_geometry_diagnostic` prints the table,
`a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee` guards it. Whole-image mean
`|dL|` hides this completely.

**The port is proven equivalent to the prototype, pointwise** — `cusp_anchored_vs_fixed_lightness`,
swept over 5832 colours across the sRGB cube, worst channel diff 0. **Keep this check working.**

Per-pixel cost: **218 ms for a worst-case 800×480 frame**. **No `t_max` lookup table was built,
deliberately** — a sibling to `CmaxTable` would have inherited its bilinear *overshoot at the
pinch* (yellow: exact 0.073 vs sampled 0.093), and an overshot `t_max` maps pixels outside the
hull in exactly the region the change exists to fix.

**The region model's own cost is ~1% of that** — Task 8 Step 5, measured with `--test-threads=1`.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` + `cargo test
  --workspace`. **~10 min — background it.** **Green at `6a4e2b1`: 1061 passed, 0 failed,
  0 warnings.**
- **`make check` runs `cargo fmt`, not `cargo fmt --check`.** It rewrites files in place. Put
  `cargo fmt` in the implementer's command list.
- **The clippy gate is `-D warnings`**, including test modules.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **`make check` does not run the `#[ignore]` tests.** Most evidence is ignored:
  - `cargo test -p eink-dither --lib gamut::mapper::tests::ray_geometry_diagnostic -- --ignored --nocapture`
  - `cargo test --release -p eink-dither --lib map_frame_cost -- --ignored --nocapture`
  - **Task 8's five:** `cargo test --release -p eink-dither --lib region_model -- --ignored --nocapture --test-threads=1`
    — **the `--test-threads=1` is mandatory**, see the method rule above.
  - `cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture`
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- Output PNGs land in `target/dither-compare/`.
- **Production applies no preprocessing before dithering** — `src/rendering/svg_to_png.rs` goes
  `rgba → Srgb → (gamut map) → dither`. `map_frame` runs **only** where `mask[i] == true`
  (`svg_to_png.rs:154-168`); an unmarked document gets `vec![false; …]` and no mapping at all.
- **Rendering a builtin screen needs a device.** It is `render --mac <MAC> --output <PATH>`,
  resolved through config. Do **not** edit the tracked `config.yaml` — copy it, point
  `CONFIG_FILE` at the copy, add a throwaway device:
  ```yaml
    "AA:BB:CC:DD:EE:01":
      panel: reterminal_e1002          # WITHOUT this you get a GREYSCALE render
      screen: byonk-builtin/calibration/tone
  ```
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
  **The missing-panel trap is silent** — it renders happily, in greyscale.
  Add `--use-actual false` for the as-sent view (see above).
- **Before adding a builtin screen, grep for what enumerates the inventory.** Two tests hardcode
  the shipped count as an exact `assert_eq!` on purpose: `tests/builtin_package.rs:44` and
  `tests/screen_schemas_test.rs:128`. Update them, never loosen them.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the workspace cannot
  resolve `eink-dither`. Releases unaffected (`Dockerfile.release`, CI-built binaries).
- `make docs` needs `mdbook-mermaid`.

## Useful test assets

`screens/builtin/calibration/color/photo.png` (portrait, 1024×1024, 7% out of gamut) and
`screens/builtin/default/background.jpg` (station concourse, 2505×1404, 12%) are byonk's own
shipping assets and are what the panel actually renders. Synthetic fields at full saturation are
unrepresentative (`ρ` p50 = 2.87 against a photo's 1.2–1.5).

## Public surface

`eink_dither::{Oklch, GamutMapper, GamutOptions}`; `gamut::hull::{Hull, HullShape}`;
`gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma` (takes `r`); `DitherOptions::pin_carry`;
`EinkDitherer::{dither_with_pinning, dither_with_regions, pin_carry}`;
`palette::ColourModel`, `Palette::{find_nearest(_, model), representative_linear(idx, model)}`;
`dither::RegionMap` — **`pub(crate)`, and deliberately so.**
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`,
`DeviceContext::dither_gamut_{knee,amount,max_compression}`, `lua_runtime::ScriptResult::gamut`,
`svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey, panel_measured}` —
**import, never copy**, and **not visible from `tests/`**. `six_colour`'s idealised primaries do
not reproduce the hull's pinch, so they cannot guard it.

## The lesson, now proven fourteen sessions running

**The plan's code and constants are not evidence.** Measure before believing the plan, your own
diagnosis, a reviewer's "harmless", the spec — or your own eyes on a downscaled PNG.

Sessions 10–14 extend this to **the tests, the comments, and the statistics the plan specifies**:
thirteen plan-authored tests measured unfounded, five doc comments claimed properties their own
code disproved, and in session 14 **the headline number was the wrong statistic entirely**. What
caught them: implementers that reported a failure instead of adjusting the test, and reviewers
told to check a named cross-task risk.

**Session 14's own addition: a clean review of every task does not mean the feature is right.**
Tasks 5, 6 and 7 each passed review, the gate was green, and a shipped calibration screen was
visibly broken the whole time — because no test asserted on it and the defect class (screens
painting measured values) was invisible to the suite. **The owner found it in one sentence by
looking at a picture. Render something and show it to them, early and often.**

**Session 12's, still true:** the owner's framing of their own ruling can be the thing that is
wrong. Ruling 23 was carried a full session as "a screen border", and a reviewer found a real
gap against that wording. The re-framing to *containment* dissolved the gap without a line of
logic changing. **When a finding says the code violates a ruling, check what the ruling is
actually protecting before changing the code.**

Session 9's, still true:

- **Adding a builtin screen has a fan-out nobody mapped** — see the inventory guards above.
- **A reviewer that fact-checks docs against code earns its cost.**
- **Measure the claim you are actually making.**
- **A ruling can carry a latent defect that only a second ruling masks.**
- **A rewritten test is an unverified test.** Mutate every guard you touch, in both directions.
- **Bound synthetic sweeps to inputs that can occur.**
- **The risk the handover flags loudest may be the cheapest to retire.**

Session 8's, still true:

- **The confident recommendation was wrong.** Cusp anchoring was the principled, cited fix and
  measured 40% against mid-grey's 82%. **Prototype before recommending; a citation is not a
  measurement.**
- **A surprising number is a lead, not noise.**
- **The owner's question was better than the controller's plan.** It has now happened in
  sessions 8, 11, 12 and 14. **Budget for it, and render something to look at early.**
- **Fixing the code is not fixing the cause.**

Session 7's, still true:

- **Every task passed review. The feature was still wrong.** **Ask what the tests do not assert.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.**

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448 tests passing).

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made in session 6 by
> task reviewers and the controller **while the owner was absent** — do not present them as
> settled. **13-23** are genuine owner rulings from sessions 7-12.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. ~~**Knee default 0.6 → 0.8**~~ (`3fd9ab8`). **Superseded by ruling 17.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`,
   `BEYOND_LIMIT_MIN_RATIO = 0.3`. **Verified still valid under ruling 16.**
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too** (`ba8859c`).
8. **Task 9b — the mask must not invent a stroke** (`297b10a`).
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it must be **one
    pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session 8, `23a1e39`).
16. **The compression direction is mid-grey anchored** (owner, session 8; implemented session 9,
    `8e30e24`). Chosen over cusp-anchored (40% vs 82% on yellow).
17. **The knee default is 0.99** (owner, session 9; implemented `868544c`). Supersedes ruling 4.
18. **Pinning is eligible everywhere outside a `continuous` region, in every document** (owner,
    session 10). Its cost was measured in Task 8.
19. **The mask marks content that is continuous-tone, not regions of the layout** (owner,
    session 10). **Applied across the whole shipped collection in `fe66ee6`.**
20. **Task 5's `#[ignore]` diagnostics stay non-asserting** (owner, session 10). Task 8's
    diagnostics inherit this. The **one** asserting test there (all-`true` == `None`) was cleared
    as an *invariant*, not a threshold.
21. **CHANGES.md is not touched by the pinning plan** (owner, session 10). One entry gets written
    at merge prep, covering gamut and pinning together. **Still outstanding.**
22. **The unmapped path assumes actual == nominal** (owner, session 11). Unmarked content is
    matched against **official** colours and pinned against them; marked `continuous` content
    keeps **actual/measured** colours and is not pinned. One mask, three consumers. Accepted cost:
    an unmarked photograph looks bad — **now measured, and the cost is large; see decision 2.**
23. **Error is CONTAINED within a colour model** (owner, session 11; **re-framed by the owner in
    session 12**): _"errors from a mapped region do not go into an unmapped region and the other
    way round … two dither systems both active and supposed not to step on each other's feet."_
    Error is never **deposited** across a model boundary, in either direction; the pinned λ-carry
    obeys the same stop. **Dropped, not renormalised.** Since a tap deposits only at its endpoint,
    dropping taps whose endpoints straddle the boundary is **exactly sufficient**, at any region
    width. The resulting behaviour is width-dependent in **connectivity**, not containment.
    **Task 8 measured it in both orientations: no seam either way.**

**Constants inherited from the plan and never challenged:** `PERCENTILE = 0.99`,
`MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`, `T_HI = 6.0`,
`T_STEPS = 24`, `ACHROMATIC_C = 1e-6`. **`pin_carry = 0.9` — the sweep found no reason to prefer
any value in [0.9, 1.0], and it is inert in the production configuration. Owner decision 1.**

Standing: **the branch is HELD** — no PR, no merge to `main`.

## Deferred minors — the final review's triage list

**The ledger holds the full text of all of these** (12 `minor (deferred)` / `parked` lines across
Tasks 3-8). Point the final whole-branch review at it.

Session 14 (Task 8): none open — all six review findings were addressed in fix round 1.

Session 13 (Tasks 5-7), in the ledger:

- Task 5: a stale doc comment on `plain_dither_is_unchanged_by_this_feature`; two tests whose
  names overstate what they assert; `dither_with_regions`' doc bullet 3 (error containment) is
  not covered by a test in that file.
- Task 6: `structural_…`'s doc says the colour model "cannot be the cause" — imprecise; no
  coverage of the `mask.len() != pixels.len()` error path.
- Task 7: the SVG comment says "paint order is unchanged" (document order DID change relative to
  two siblings; harmless because the bands are geometrically disjoint); a stale ~0.92
  discrimination figure survives in `screen_store.rs`'s failure message; the diagnostic hardcodes
  the `reterminal_e1002` palette and 800×480 instead of reading config; "every rightward and
  downward tap lands inside the gap" is exactly true only for cell-edge pixels; the diagnostic
  uses `std::env::temp_dir()` and cleans up only on the success path.

Session 12 (Task 4): mutation-table row 5 remains unreachable by construction; documented, not
faked. (`builder.rs`'s placeholder uniform mask was replaced by Task 5, as required.)

Session 11 (Task 3):

- **TWO stale rustdoc references to the deleted `find_second_nearest`** at `palette.rs:38` and
  `palette.rs:355-358`. Both PREDATE that task and exist as historical rationale for the
  kchroma=10 tuning decision, but they name a method that does not exist, in user-facing rustdoc.
  Two-line fix.

Session 11 (Task 2):

- Hostile-field fixture duplicated verbatim 5× and the ink-share closure 2× in `builder.rs` tests.
  Partially addressed by the helper extraction; verify.
- ~65 lines of per-test comments are archaeology of the *brief* rather than of the code.
- A wrong-length `pin_eligible` silently disables pinning in release builds (`debug_assert!` only)
  and has no test in either direction.
- Stale/duplicated step-numbering comments in `dither_with_pinning`.

Session 10 (Task 1): `f32::clamp` does not trap NaN in `pin_carry`; the field is `pub` anyway.

Session 9:

- `six_colour`'s blue vertex cannot reach the knee's design point because the constant-hue OKLch
  ray **bulges outside the linear-RGB hull**. `t_max = 0.861`. Harmless.
- `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` still hardcode the panel palette,
  duplicating `test_support::panel_measured`. **Exporting `test_support` is the fix if a fourth
  copy appears** — Task 8 came within one decision of being that fourth copy.
- `mapped_chroma` is now `#[cfg(test)]`.

Session 8: the `Cmax` table's bilinear sample *overshoots* where the hull pinches (yellow: exact
0.073 vs sampled 0.093). **Now load-bearing knowledge** — it is why no `t_max` table was built.

Session 7: `test_gamut_mapping_preserves_hue_order` would also pass against an **identity**
mapper. Weak guard, kept deliberately.

Session 6:

- **Task 10:** the unreachable mask-length-mismatch branch returns `RenderError::Dither`, a
  misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)` **cannot** detect a
  restated-constant violation — manual-review-only.
- **Task 11:** `PanelDitherConfig` accepts a `gamut:` key in panel YAML; verify it is live.
- **Task 12 (inherited):** `resolve_effective_tuning` replaces the **whole** struct when any
  override field is set, so an active dev-UI query override resets the previewed gamut to default
  and diverges from production.

Earlier sessions:

- **Task 7:** the winning dilute start was `eps = 0.005`. Optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`; traced — failure
  mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()` while
  `rewrite_start` propagates them. Style wart.
- **Task 8:** element names matched as raw bytes, so `<svg:image>` would be mis-handled and
  `<symbol>` gets no `<defs>`-style stripping. Dormant.
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`.
- **Task 9b:** `resolve_stroke` cannot see stylesheet-only strokes. Deliberate.
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three epsilons want a
  comment; `adapt.rs`'s `max_compression < 1.0` collapse is untested; no test exercises literal
  `NaN`.

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
   **Re-measure in linear light first** — see the brightness section above.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale. **No working
   hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. The benign half is established
   and asserted: a flat fill of a *measured ink* dithers to that single ink exactly, which is
   correct — **but only in isolation.** Set next to saturated content, 27% of those same pure-ink
   pixels were taken over by diffused error. **This is the defect the completed initiative
   addresses**, and the tone grid now measures 100.00% black where it was 73.58%.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does not work.
`AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on both axes — changing the
default alters rendering for every device, so it is the owner's call.
