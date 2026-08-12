# Handover — Byonk

_Last updated: 2026-08-12 (session 15). **The pinning initiative is COMPLETE and REVIEWED.**
All 8 tasks done, the scoped final review is in, every blocking finding fixed, the
deferred-minors roll-up triaged. All three owner decisions are answered. The work lives on
`feat/screen-store-authoring-core`, which has an **open PR (#30) against `main`** — the branch
is no longer "held", it is awaiting merge through that PR. **Next: push the session-15 commits
to PR #30, then merge prep.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | see `git log -1`; session 15 added commits after `ac4e8a0` |
| State | tree clean; full `make check` green |
| Active spec | `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` |
| Active plan | `docs/superpowers/plans/2026-08-11-panel-colour-pinning-amended.md` — all tasks done |
| Ledger | `.superpowers/sdd/2026-08-11-panel-colour-pinning-amended/progress.md` (git-ignored) |
| Task 8's numbers | `.superpowers/sdd/.../task-8-report.md` (git-ignored) |
| Prior initiative | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — complete |

**Not yet pushed.** Session 15's commits are local. Pushing updates PR #30.

# ⚠️⚠️ START HERE — what session 15 changed about the story

## 1. λ is NOT inert. The previous handover said it was, and that was wrong.

The old text (lines 60/89 of the previous version) said `pin_carry` is inert "in the production
configuration". **That is scope-limited in a way that matters.** It holds only for scenario C —
structure over a *marked* photograph, where containment leaves no error for λ to attenuate.

**But a document with no tone markup gets `vec![false; …]`** (`svg_to_png.rs:170-175`): pinning
everywhere, no region boundaries at all, so containment drops nothing and **λ fully governs the
carried error**. Ten of the thirteen shipped screens are unmarked, so λ is live for most of the
collection.

**Decision 1 still resolves the same way** — ship `pin_carry = 0.9`; the sweep found no reason to
prefer any value in [0.9, 1.0]. But it ships on "no measured reason to change it", **not** on
"it doesn't matter anyway". Do not repeat the inert framing.

The metric finding is separate and still stands: "line ink purity" is 100% at every λ **by
construction** (`api/builder.rs` matches by exact byte against `palette.official(i)`;
`dither/mod.rs:387-395` writes `output[idx] = ink` unconditionally, using `pin_carry` only to
scale *forwarded* error). A theorem, not a measurement. Independently re-confirmed by the final
reviewer against the code.

## 2. Decision 2 — the unmarked-photograph cost — is now confirmed TWICE, by two paths

Task 8's crate diagnostic measured `photo.png` at **75.2% black unmarked vs 56.2% marked**.
Session 15 rendered `calibration/tone` through the **CLI renderer** on a different asset
(`photo.jpg`) and measured **74.3% vs 54.9%**.

Different harness, different image, same answer. This is the strongest-supported number in the
initiative. It is the accepted cost of ruling 22 and it is now in `CHANGES.md` as an upgrade note.

## 3. Decision 3 is answered and DISCHARGED — `calibration/tone` was relabelled

The question was whether the unmarked control column is "still legible enough to serve as a
control". The answer turned out to be sharper: **it is perfectly legible and is no longer a
control.**

The screen labelled its columns `UNMAPPED (control)` / `GAMUT MAPPED` — promising **one**
variable. Ruling 22 gave it **three**: one mask drives colour model, gamut mapping and pinning
together. The visible gap is dominated by the colour model, and Task 8 measured the mapper's own
contribution at roughly **1/14** of it (block-averaged ΔE 0.008 vs 0.107). The screen was
overstating the mapper by about an order of magnitude.

**Fixed by relabelling** to `UNMARKED - nominal, pinned` / `MARKED - measured, mapped`, with the
reasoning written into `script.lua`'s header so it cannot be re-broken by someone "tidying" the
labels. There was no option to vary one axis — ruling 22 binds all three to one mask, and the
screen's own comment forbids it becoming the reason a per-axis attribute gets built.

**The screen is now honest about what it shows: what byonk does to marked vs unmarked content —
which is exactly what an author must understand to mark their own screens.**

## 4. The final review happened, scoped correctly, and it was worth it

**Scope it to the initiative, never the branch.** `merge-base main HEAD` is `67b3855`, and
`67b3855..HEAD` is **261 commits / 178 files / 48,803 insertions** across three unrelated
initiatives. The reviewable package is `c74312f^..HEAD` = `b5a18bd..HEAD`, **21 files /
~5,563 insertions**. The review used that and returned findings; the whole-branch range would
have returned nothing usable.

What it caught that eight clean per-task reviews did not:

- **`CHANGES.md` shipped two entries that were FALSE at HEAD** — one said the calibrator patches
  are drawn in measured colours (`4eef93e` reverted exactly that), one said the pinning mechanism
  "is gone entirely… it turned out to buy nothing" (pinning is back, and `dither/mod.rs:380-384`
  refutes that rationale by name). Plus no entry described the feature at all.
- **No test covered the defect class that actually shipped broken.**
- **Three doc comments asserting properties their own code disproved** (instances 6-8).

It also **mutation-tested rather than read**: inverting `RegionMap::model` fails 3 tests, all on
the non-degenerate fixture. And it *rejected* one item I expected it to take — the
`std::env::temp_dir()` concern — correctly noting the no-`/tmp` scratch-image rule is a
**production** rule and that code is `#[cfg(test)]`.

# What session 15 did

| | |
|---|---|
| Regression test for the shipped-broken class | `the_colour_calibrator_patches_are_flat_single_inks` in `screen_store.rs` |
| `calibration/tone` relabel + rationale | `screens/builtin/calibration/tone/{script.lua,screen.svg}` |
| Containment's kernel property, now asserted | `no_kernel_propagates_error_upwards` + compile-time exhaustiveness guard, `dither/kernel.rs` |
| Sliver test's missing non-degeneracy control | `dither/mod.rs` |
| Wrong-length mask: `# Panics` doc + both-direction tests | `api/builder.rs` |
| Three stale doc claims corrected | `palette.rs` ×2, `api/builder.rs` ×1 |
| `CHANGES.md` | two false entries rewritten + the missing tone entry with upgrade note |
| Stale ~0.92 figure caveated in its failure message | `screen_store.rs` |
| Hardcoded panel in diagnostic, flagged in a comment | `svg_to_png.rs` |

**The regression test is mutation-verified.** Reverting `script.lua`'s `local ink = colors` to
`(device and device.colors_actual) or colors` fails it at **57.1% #FF0000**, reproducing the
figure in `4eef93e`. **Only that one patch value is confirmed** — the assertion aborts at the
first failure, so the other three in that commit message are not re-confirmed, and the test's doc
says so.

**The mutation is invisible on black and white**: both entries are identical in the two colour
models, so they pin either way. Red is the first entry that can discriminate. That is why the
test asserts ≥4 of 6 entries differ before measuring anything.

**`no_kernel_propagates_error_upwards` is mutation-verified too** — an upward tap planted in
`SIERRA_LITE` fails it by name.

# ⚠️⚠️ NEW METHOD RULES — session 14 and 15

## Block-average before comparing two dithered images

Companion to the standing "measure in linear light" rule, and just as load-bearing. `mean_de`
compares per pixel, where every pixel is **one hard ink**. Two halftones of the same image
decorrelate their patterns and score a large per-pixel ΔE **even when visually identical**.

| pair | k=1 | k=4 | k=8 | k=16 |
|---|---|---|---|---|
| photo: unmarked vs marked_mapped | 0.1773 | 0.1224 | 0.1116 | **0.1066** |
| photo: marked_raw vs marked_mapped | 0.1144 | 0.0368 | 0.0160 | **0.0077** |
| background: unmarked vs marked_mapped | 0.1770 | 0.1235 | 0.1062 | **0.0975** |
| background: marked_raw vs marked_mapped | 0.1284 | 0.0418 | 0.0165 | **0.0088** |

The mapper pair **collapses by 15x**; the unmarked pair barely moves. **A per-pixel mean over a
halftone measures pattern, not appearance.** `block_de` in `domain_tests.rs` does this correctly;
k=1 reproduces `mean_de` exactly, which is what validates it.

Note the conclusion **survived and strengthened** under correction. That is the usual outcome and
worth remembering when a correction feels like it will cost you your result.

## A wall-clock benchmark in a test binary is only valid with `--test-threads=1`

Task 8's cost diagnostic used `Instant` and its own run command ran it **concurrently with four
other 800×480 diagnostics**. Under that command the reviewer measured the **baseline slower than
the feature** — the headline's sign flipped. Isolated, it reproduces within ~0.6 ms. **The recipe
must state the condition; the numbers alone are not the result.**

## `--use-actual false` is a free second view of any render

`render --mac <MAC> --use-actual false --output <PATH>` draws the PNG in the **nominal** palette
— the actual wire format, indexed, exactly 6 colours. Byte-identical in *index* to the measured
render, so it costs nothing, and it is a **better lens for spotting stray pixels**: one wrong
index shows up as a screaming primary rather than a subtle speckle.

**It is also what makes ink histograms exact.** Session 15 measured `calibration/tone`'s columns
by counting exact RGB matches in the nominal render — no tolerance, no nearest-neighbour guess.
The same trick is what makes the new `calibration/color` regression test cheap and precise.

## Detect geometry from the rendered document, not from the layout code

The new patch-row test takes its rectangles from the **SVG that was actually rasterized**
(`include_svg: true`), selecting rects that carry both a literal hex fill and an explicit `x` —
which on that screen is exactly the patch row. Recomputing the Lua layout in the test would drift
silently the first time the screen is re-laid-out, and the test would keep passing while
measuring the wrong pixels.

# The design as built

**The unmapped path assumes the actual colours ARE the nominal colours.** One mask selects three
things at once:

| Region | Colour model | Gamut mapping | Pinning |
|---|---|---|---|
| **Unmarked** (structure) | **official/nominal**, substituted for actual | off | **on**, against official |
| **Marked** `continuous` | **actual/measured** | on | **off** |

`data-byonk-tone="continuous"` is the attribute. `RegionMap { continuous, pinned }` carries the
mask inside the crate; `EinkDitherer::dither_with_regions(pixels, w, h, continuous:
Option<&[bool]>)` is the public entry point.

- **`continuous` is the tone mask itself, NOT its inverse.** `true` = marked = measured model,
  mapped, not pinned. A flipped polarity is silent and produces a plausible image either way —
  **mutation-tested by the final reviewer: a flip fails 3 tests on the non-degenerate fixture.**
- **`None` means the feature is OFF** — measured everywhere, bit-for-bit today's output.
- **An all-`true` mask is bit-identical to `None`.** Cheapest polarity check available; keep it.
- **Pin eligibility is resolved by the CALLER** and is not re-checked in the dither loop.
- **`error_clamp` does not bound the pinned carry.** Inert at the live default — uniformly `1.0`
  for every algorithm (`dither/mod.rs:118`). Deliberate, verified.
- **A wrong-length mask silently disables the whole region map in RELEASE builds**, reverting the
  colour model for the entire frame. Debug builds panic. Now documented under `# Panics` with
  tests in both directions. Byonk's own call site hard-errors first.

## Containment (ruling 23), measured in both orientations

**No seam, either way.** The *harder* orientation is cleaner, structurally rather than by luck:
**every Atkinson/FS/JJN/Sierra entry has `dy >= 0`** — no upward taps at all. So nothing below a
**horizontal** boundary can influence anything above it. **This is now asserted**
(`no_kernel_propagates_error_upwards`) instead of resting on inspection; a new kernel with an
upward tap can no longer weaken containment silently. A compile-time exhaustiveness guard keeps
the algorithm list from going stale.

The vertical case has sideways exposure on one band via Atkinson's `(-1, 1)` tap. **Atkinson has
divisor 8, `max_dy` 2, entries including `(2,0)` and `(-1,1)`** — max reach |dx| = 2, and **error
does travel left.**

# ⚠️⚠️ THE FIXTURE TRAP — still the highest-value warning in this file

**Several palette helpers in `eink-dither` are built with `Palette::new(x, None)`, which sets
`actual = official` (`palette.rs`). Under any of them the two colour models are IDENTICAL, so a
test written against one passes against every mutant, silently — and a *measurement* reports zero
difference while looking like a clean result.**

`dither/mod.rs`'s own test module has two — `pin_test_palette()` and `panel_palette()` — and they
are what a fresh implementer reaches for by default. **That is the trap.**

The only fixture whose official and actual sets genuinely differ is
`crate::gamut::test_support::panel_measured()`. Probe indices 2-5 (red/yellow/blue/green);
**black and white are degenerate even there.**

**It is `#[cfg(test)] pub(crate)`, so integration tests in `tests/` CANNOT see it.** This nearly
sank Task 8. `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` are existing hardcoded
copies; **exporting `test_support` is the fix if a fourth appears.**

**The trap has a byonk-side twin.** The new `calibration/color` test defines its panel with
`colors_actual` differing from `colors` and **asserts ≥4 of 6 entries differ before measuring** —
because with the two sets equal the mutation it targets becomes a no-op and the test would pass
against it while looking healthy. Any byonk test about the colour models needs that guard.

The final review confirmed every colour-model test in the initiative uses a non-degenerate
fixture and asserts it. **Keep that record intact.**

One legitimate use of degenerate black: the pinned branch writes the ink index directly and never
calls `find_nearest` or `representative_linear`, so a pinned pixel consults **no** model and a
test about the *carry* may use black freely.

# ⚠️⚠️ Thirteen plan-authored tests measured unfounded; eight doc comments claimed what their code disproved

**Still the single most important thing in this file.** Every one was caught only because an
implementer or reviewer refused to accept the premise.

Tests: **4 (Task 1) + 5 (Task 2) + 1 (Task 3) + 1 (Task 4) + 2 (Task 8)** = 13.
Doc comments: 5 through session 14, **+3 found by the final review** = 8.

**The rules in force:**

- **A test claiming "X rescues this case" must assert, in the same test, that the case needed
  rescuing.**
- **A comparison test must assert its comparison is non-degenerate.**
- **A doc comment asserting a mutation or invariant property is an unverified claim.** Eight
  instances — including once in code written to fix the first instance, and three found in the
  final review after eight clean task reviews.
- **A test must be able to attribute its result to the mechanism it names.** Ask what *else*
  could produce the same pass.
- **A mutation-table row can describe an IMPOSSIBLE mutation.** Saying so is correct; inventing a
  code path to satisfy the table is not.
- **A plan can name a metric that cannot vary with the parameter it is meant to tune.** Check the
  metric against the mechanism before running the sweep.
- **NEW (session 15): a doc can claim coverage the code makes impossible.** `builder.rs` claimed
  a test caught an all-true-mask mutant; an all-continuous mask is bit-identical to `None` by
  construction, so that mutant is undetectable. **When a doc names a mutant, check the mutant is
  reachable.**
- **Write one mutant per site.**

**Corollary for any future brief:** put the plan's test bodies in as *hypotheses to measure*, and
say so explicitly. Tell the implementer that a mutant surviving its named test is a plan defect to
report, not a value to tune. **That single instruction has caught all thirteen.**

# ⚠️ Pre-flight has never once been clean — 14 sessions running

Session 14's Task 8 pre-flight found 2 Important + 2 Minor, and **both Important ones would have
invalidated the entire task**: the `tests/` location could not see the only non-degenerate
fixture; and `RegionMap` is `pub(crate)`, so the plan's wording was not expressible — the failure
mode being an implementer **widening production visibility to make a diagnostic compile.** Forbid
that explicitly. (The final review confirmed no visibility creep happened.)

- **A plan step that says "pass `None` here so it compiles" can silently disable a feature an
  earlier task's tests assert.** Grep for what asserts the behaviour being placeholdered.
- **Check whether a mutation-table row is reachable at all** before dispatching.
- **A stale baseline in a brief is not a baseline.** Re-derive the before-value in the same
  harness that produces the after-value.

# ⚠️⚠️ Read this before dispatching any subagent

**`make check` takes ~10 minutes in this tree.** The subagent stream watchdog fires at 600 s of
silence, so **an implementer that runs `make check` in the foreground dies mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` then
  `CARGO_BUILD_JOBS=2 cargo test -p <crate> --lib`. Say so in the brief.
- **The controller runs the full gate** in a **backgrounded** Bash call (`run_in_background:
  true`). Foreground `sleep` is blocked — use an `until` loop in a backgrounded call to wait.
- **Redirect the gate log to a file; never pipe it through `tail`** — you lose the counts.
- **`cargo test` takes only ONE filter argument.** Passing several fails with "unexpected
  argument"; run the whole crate suite instead.

**Subagent task briefs scoped to `--lib` cannot see integration tests.** The controller's full
gate is not a formality.

**Tell the implementer the pre-existing failures.** `cargo test -p eink-dither --lib -- --ignored`
reports **3 pre-existing failures unrelated to any current work**:
`preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
test_resize_full_pipeline_with_photo_preset}`, which panic at `resize_lanczos` **by design**.

**Give the reviewer the cross-task risk to check, by name.** The final review was given six named
risks and returned a verdict on each, having **independently reproduced the claims against the
code**. **A named risk earns its cost; "check all uses" does not.**

**Hand deviations to a reviewer as "judge these on their merits", never as "these were
approved."** Task 8 declared four; the final review upheld all four *with mechanism*.

**Never pre-judge a finding for a reviewer.** If the prompt contains "do not flag" or "at most
Minor", stop — you are sparing yourself a review loop at the cost of the review.

# ⚠️ This build cannot resample (eink-dither only)

`resize_lanczos` **panics on any real dimension change** — no `image` backend is compiled into
`eink-dither` proper. Same root cause as the three pre-existing failures.

**But `image` IS a dev-dependency**, so test code may resize with `image::open(...).resize(...,
FilterType::Lanczos3)`, as `tests/visual_compare.rs` does. Never route test images through
`Preprocessor`. `image` is also a real dependency of the **byonk** crate.

# ⚠️⚠️ Read this before trusting any dithering picture

**Every visual dither comparison in this tree reads about 30% too dark, and it is the viewer's
fault, not the ditherer's.**

| | mean LINEAR luminance | mean GAMMA-SPACE byte |
|---|---|---|
| portrait | **+10.2%** vs source | −32.4% vs source |
| background | **+4.4%** vs source | −29.3% vs source |

Error diffusion preserves brightness in linear light. A viewer downscaling a PNG without
linearising averages sRGB bytes directly, under-weighting bright pixels in a black/ink speckle.

- **Relative** comparisons between variants remain valid. Use `-mapped` (undithered) renders for
  absolute judgements.
- **Open defect 1 below was diagnosed this way and should be re-measured in linear light before
  anyone chases it.**

# ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading** — a flat patch is a single colour; every artifact that
matters is at a boundary *between* colours.

**Whole-image means are equally misleading.** On the portrait all four gamut anchors scored
0.0545–0.0550 mean chroma and looked identical, because only 7% of pixels are out of gamut and the
untouched 93% swamped them. Restricted to the pixels the mapper acts on, the spread was 68% to
90%. **Measure the pixels the change touches.**

**And block-average before comparing halftones** — see above.

**Look to find what to measure; measure to decide.** When comparing an old behaviour to a new one,
**render both from the same input in the same image**.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run. Never take a subagent's
"all green" at face value.

# The screen collection is marked — done

Ruling 19 applied across all 13 shipped screens; **10 needed nothing**. Marked:
`builtin/default` (background photograph), `builtin/calibration/color` (each gradient bar, hue
sweep, photo), `examples/gphoto` (full-screen photo). `builtin/calibration/tone` was already
marked; its grid rect left the marked group in Task 7, and its column labels were corrected in
session 15.

**The argument for leaving an achromatic gradient unmarked is the one to remember.** Grey is
always in gamut, so mapping it is a *no-op* — while marking it switches exact-match pinning *off*
across a deliberate dithering test pattern. **Any "should this be marked?" question should start
by asking whether the content can even be out of gamut.**

**Marking goes on the element that IS continuous-tone, never on a group around a band.**

### The mask rasterizes in document order, and that does real work

At 800×480: `calibration/color` marks **262,672 / 384,000** px; `default` marks **309,125 /
384,000 (80.5%)** — even though its photo is *full-screen*. Unmarked elements drawn **after** a
marked one paint black back over it, so `default`'s hero text, swatches and white info bar punch
themselves out of the photo's marked area. Those pixels are opaquely covered, so excluding them is
correct. **For authors: text over a photo needs no special handling as long as it comes later in
document order.**

### Untracked duplicate — do not sweep it in

`/Users/oetiker/checkouts/byonk/examples/` is an **untracked** near-copy of `screens/examples/`,
drifted by one file (`gphoto/screen.svg`). Exactly the kind of local file that makes
`git add -A` dangerous here. **Never `git add -A`; add by explicit path.**

# ⚠️ Ruling 22's blast radius — the class no test could see

**"Every screen that paints a measured value."** In session 14 a shipped calibration screen was
**visibly broken** while Tasks 5, 6 and 7 all reviewed CLEAN and the full gate was green.
`script.lua` filled the patches from `device.colors_actual`, which ruling 22 turned into a
non-matching value on the unmapped path. The owner found it in one sentence by looking at a
render.

**Session 15 closed it two ways:**

1. **Source:** `grep -rn colors_actual screens/` returns exactly one hit — a *label contrast
   decision*, not a fill. No screen paints a measured value into unmarked content.
2. **Test:** `the_colour_calibrator_patches_are_flat_single_inks` renders the screen and asserts
   each patch interior is ≥99% one ink. Mutation-verified.

**The trap in the fix, worth not re-deriving:** the patch label's contrast colour must stay
**measured** while the fill goes **nominal**, because the patch is filled nominal but *renders* as
the measured ink. Judging contrast on the fill puts black text (nominal `#00FF00`, lum 182) onto
the green ink (`#0D876B`, lum 107).

# Authoring documentation — done

- **`docs/src/tutorial/svg-templates.md`** — "Marking continuous-tone content": what the attribute
  does, the three things one mark drives, the two easy mistakes, document-order behaviour.
- **`docs/src/api/lua-api.md`** — its `colors_actual` example **taught the measured-fill
  anti-pattern**. Corrected to "DECIDE with measured, PAINT with official".
- **`docs/src/guide/dev-mode.md`** — "only it is gamut-mapped" corrected to the three things.
- **`CHANGES.md`** (session 15) — the tone-marking entry carries the same rules, so changelog and
  guide teach the same thing.

Verified with `make docs`.

## What is left

1. **Push session 15's commits to PR #30.** Not done — the commits are local.
2. **Merge prep.** Ruling 21's combined gamut+pinning changelog entry is **written** (session 15).
   Re-read `CHANGES.md`'s Unreleased section as a whole before release; it is long and was written
   across many sessions.
3. **Remaining fix-later minors** from the review's triage, none blocking:
   - two test names that overstate what they assert (`dither/mod.rs`) — cleanup only;
   - `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
     `kchroma = 10` tuning are **not on the crate's dithering path at all**. Documented, not
     changed — **this is a live design question for the owner**, not a bug.
4. **Open dithering defects** below — independent of this work.

# The prior initiative: gamut mapping (complete)

Rulings 16 and 17 are implemented, measured and committed. The mapper compresses along a ray from
mid-grey; knee default 0.99. All four measured panel inks come back at `t_max = 1.000`.

**Ruling 16 and ruling 17 are only safe together.** The ray's liability is near-white tints: a
high-`L`, low-chroma colour's ray exits the hull at the *white point*, so it reads as
boundary-saturated even though its chroma was never out of gamut. At knee 0.8 this darkens `grey
250 tint 4` by **−0.084**; at 0.99 by −0.0035. **Do not lower the knee without re-measuring** —
`gamut::mapper::tests::ray_geometry_diagnostic` prints the table,
`a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee` guards it. Whole-image mean `|dL|`
hides this completely.

**The port is proven equivalent to the prototype, pointwise** — `cusp_anchored_vs_fixed_lightness`,
swept over 5832 colours across the sRGB cube, worst channel diff 0. **Keep this check working.**

Per-pixel cost: **218 ms for a worst-case 800×480 frame**. **No `t_max` lookup table was built,
deliberately** — a sibling to `CmaxTable` would have inherited its bilinear *overshoot at the
pinch* (yellow: exact 0.073 vs sampled 0.093), and an overshot `t_max` maps pixels outside the hull
in exactly the region the change exists to fix.

**The region model's own cost is ~1% of that** — Task 8 Step 5, measured with `--test-threads=1`.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` + `cargo test
  --workspace`. **~10 min — background it.**
- **`make check` runs `cargo fmt`, not `cargo fmt --check`.** It rewrites files in place.
- **The clippy gate is `-D warnings`**, including test modules.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine.
- **`make check` does not run the `#[ignore]` tests.** Most evidence is ignored:
  - `cargo test -p eink-dither --lib gamut::mapper::tests::ray_geometry_diagnostic -- --ignored --nocapture`
  - `cargo test --release -p eink-dither --lib map_frame_cost -- --ignored --nocapture`
  - **Task 8's five:** `cargo test --release -p eink-dither --lib region_model -- --ignored --nocapture --test-threads=1`
    — **the `--test-threads=1` is mandatory.**
  - `cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture`
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- Output PNGs land in `target/dither-compare/`.
- **Production applies no preprocessing before dithering** — `src/rendering/svg_to_png.rs` goes
  `rgba → Srgb → (gamut map) → dither`. `map_frame` runs **only** where `mask[i] == true`; an
  unmarked document gets `vec![false; …]` and no mapping at all.
- **Rendering a builtin screen needs a device.** `render --mac <MAC> --output <PATH>`, resolved
  through config. Do **not** edit the tracked `config.yaml` — copy it, point `CONFIG_FILE` at the
  copy, add a throwaway device:
  ```yaml
    "AA:BB:CC:DD:EE:01":
      panel: reterminal_e1002          # WITHOUT this you get a GREYSCALE render
      screen: byonk-builtin/calibration/tone
  ```
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run --release -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
  **The missing-panel trap is silent** — it renders happily, in greyscale. Add `--use-actual false`
  for the as-sent view. `devices:` is the last top-level block in `config.yaml`, so a device can be
  appended.
- **Before adding a builtin screen, grep for what enumerates the inventory.** Two tests hardcode
  the shipped count as an exact `assert_eq!` on purpose: `tests/builtin_package.rs` and
  `tests/screen_schemas_test.rs`. Update them, never loosen them.
- **Before changing any VISIBLE TEXT on a builtin screen, grep for the string.** Session 15's
  `calibration/tone` relabel broke `the_tone_screen_renders_both_columns`, which asserts the
  column labels literally — and it is a *different* test from the one that checks the marking, in
  the same file, so finding one does not find the other. **`grep -rn "<the old label>" src/ tests/`
  before editing a screen's text.** The label assertions now carry a message explaining why the
  wording is what it is, so the next person hits an explanation rather than a bare `assert!`.
- **`Dockerfile` is broken independently** — it never copies `crates/`. Releases unaffected
  (`Dockerfile.release`, CI-built binaries).
- `make docs` needs `mdbook-mermaid`.

## Useful test assets

`screens/builtin/calibration/color/photo.png` (portrait, 1024×1024, 7% out of gamut) and
`screens/builtin/default/background.jpg` (station concourse, 2505×1404, 12%) are byonk's own
shipping assets. Synthetic fields at full saturation are unrepresentative (`ρ` p50 = 2.87 against
a photo's 1.2–1.5).

## Public surface

`eink_dither::{Oklch, GamutMapper, GamutOptions}`; `gamut::hull::{Hull, HullShape}`;
`gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`; `gamut::knee::compress_chroma`;
`DitherOptions::pin_carry`; `EinkDitherer::{dither_with_pinning, dither_with_regions, pin_carry}`;
`palette::ColourModel`, `Palette::{find_nearest(_, model), representative_linear(idx, model)}`;
`dither::RegionMap` — **`pub(crate)`, and deliberately so.**
Byonk: `models::GamutTuningValues`, `DitherTuningValues::gamut`, `DeviceConfig::gamut`,
`RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey, panel_measured}` — **import,
never copy**, and **not visible from `tests/`**. `six_colour`'s idealised primaries do not
reproduce the hull's pinch.

## The lesson, now proven fifteen sessions running

**The plan's code and constants are not evidence.** Measure before believing the plan, your own
diagnosis, a reviewer's "harmless", the spec — or your own eyes on a downscaled PNG.

Sessions 10–15 extend this to **the tests, the comments, and the statistics the plan specifies**:
thirteen plan-authored tests measured unfounded, eight doc comments claimed properties their own
code disproved, and in session 14 **the headline number was the wrong statistic entirely**.

**Session 15's own addition: a clean review of every task does not mean the work is right, and it
does not mean the DOCUMENTATION is right.** Eight tasks reviewed clean while `CHANGES.md` carried
two entries that were the opposite of what shipped. **Changelog and docs are part of the diff —
review them against the code, not against the plan.**

**Session 14's, still true: a clean review of every task does not mean the feature is right.**
Tasks 5, 6 and 7 each passed review, the gate was green, and a shipped calibration screen was
visibly broken the whole time — because no test asserted on it. **The owner found it in one
sentence by looking at a picture. Render something and show it to them, early and often.** It has
now happened in sessions 8, 11, 12, 14 and 15. **Budget for it.**

**Session 12's, still true:** the owner's framing of their own ruling can be the thing that is
wrong. Ruling 23 was carried a full session as "a screen border", and a reviewer found a real gap
against that wording. The re-framing to *containment* dissolved the gap without a line of logic
changing. **When a finding says the code violates a ruling, check what the ruling is actually
protecting before changing the code.**

Session 9's, still true:

- **Adding a builtin screen has a fan-out nobody mapped.**
- **A reviewer that fact-checks docs against code earns its cost.** (Session 15: it earned it
  again, on the changelog.)
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
- **Fixing the code is not fixing the cause.**

Session 7's, still true:

- **Every task passed review. The feature was still wrong.** **Ask what the tests do not assert.**
- **Say "I verified" only after verifying.** Reading is not measuring. (Session 15 caught itself
  doing this: four patch percentages were written into a doc comment from a commit message before
  any of them had been re-measured. Only one was actually confirmed; the doc now says which.)
- **Pre-flight the brief, every time — it has never once been clean.**

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448 tests passing).

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made in session 6 by
> task reviewers and the controller **while the owner was absent** — do not present them as
> settled. **13-23** are genuine owner rulings from sessions 7-12.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`**.
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`**.
4. ~~**Knee default 0.6 → 0.8**~~. **Superseded by ruling 17.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`**. **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table.** `IN_LIMIT_MAX_RATIO = 0.05`,
   `BEYOND_LIMIT_MIN_RATIO = 0.3`. **Verified still valid under ruling 16.**
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too**.
8. **Task 9b — the mask must not invent a stroke**.
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`**. Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it must be **one
    pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session 8).
16. **The compression direction is mid-grey anchored** (owner, session 8; implemented session 9).
    Chosen over cusp-anchored (40% vs 82% on yellow).
17. **The knee default is 0.99** (owner, session 9). Supersedes ruling 4.
18. **Pinning is eligible everywhere outside a `continuous` region, in every document** (owner,
    session 10). Its cost was measured in Task 8.
19. **The mask marks content that is continuous-tone, not regions of the layout** (owner,
    session 10). **Applied across the whole shipped collection.**
20. **Task 5's `#[ignore]` diagnostics stay non-asserting** (owner, session 10). Task 8's inherit
    this. The **one** asserting test there (all-`true` == `None`) was cleared as an *invariant*.
21. **CHANGES.md is not touched by the pinning plan** (owner, session 10). One entry gets written
    at merge prep, covering gamut and pinning together. **DISCHARGED in session 15.**
22. **The unmapped path assumes actual == nominal** (owner, session 11). Unmarked content is
    matched against **official** colours and pinned against them; marked `continuous` content
    keeps **actual/measured** colours and is not pinned. One mask, three consumers. Accepted cost:
    an unmarked photograph looks bad — **measured twice, and the cost is large; it is in
    `CHANGES.md` as an upgrade note.**
23. **Error is CONTAINED within a colour model** (owner, session 11; **re-framed by the owner in
    session 12**): _"errors from a mapped region do not go into an unmapped region and the other
    way round."_ Error is never **deposited** across a model boundary, in either direction; the
    pinned λ-carry obeys the same stop. **Dropped, not renormalised.** Since a tap deposits only at
    its endpoint, dropping taps whose endpoints straddle the boundary is **exactly sufficient**, at
    any region width. **Measured in both orientations: no seam either way. The kernel property it
    rests on is now asserted.**

**Constants inherited from the plan and never challenged:** `PERCENTILE = 0.99`,
`MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`, `T_HI = 6.0`,
`T_STEPS = 24`, `ACHROMATIC_C = 1e-6`. **`pin_carry = 0.9` — the sweep found no reason to prefer
any value in [0.9, 1.0]. It is inert in scenario C only; for an unmarked document it is LIVE.**

## Deferred minors — triaged by the final review

The ledger holds the full text. The review triaged all ~20: **6 were already fixed** by later
tasks, and session 15 cleared the fix-before-merge set and most fix-later items. What the review
explicitly **dropped**, with reasons worth keeping:

- `dither_with_regions`' doc bullet 3 duplicated at the builder level — covered elsewhere.
- `mask.len() != pixels.len()` coverage — unreachable from byonk; now documented and tested at the
  crate level anyway.
- "every rightward and downward tap lands inside the gap" — exactly true for cell-edge pixels only;
  the conclusion it supports holds regardless.
- The diagnostic's `std::env::temp_dir()` — **the no-`/tmp` scratch-image rule is a PRODUCTION
  rule**; that code is `#[cfg(test)]`.
- `f32::clamp` not trapping NaN in `pin_carry` — no user-reachable path.
- Task 4's mutation-table row 5 — documented as unreachable rather than faked. Correct handling.

Still open, non-blocking: two overstated test names; `six_colour`'s blue vertex cannot reach the
knee's design point (`t_max = 0.861`, harmless); `gamut_cusp_prototype.rs` and
`gamut_adaptation_diag.rs` still hardcode the panel palette — **exporting `test_support` is the fix
if a fourth copy appears**; `test_gamut_mapping_preserves_hue_order` would also pass against an
identity mapper (weak guard, kept deliberately); `PanelDitherConfig` accepts a `gamut:` key in
panel YAML — verify it is live; `resolve_effective_tuning` replaces the **whole** struct when any
override field is set, so an active dev-UI query override resets the previewed gamut to default.

Earlier, still open: the `Event::CData` stylesheet branch is live but untested;
`strip_paint_declarations`/`_inline` split naively on `;` and `:` (failure mode is always "left
untouched", safe); `resolve_tone` drops attribute-iteration errors via `.flatten()`; element names
matched as raw bytes, so `<svg:image>` would be mis-handled and `<symbol>` gets no `<defs>`-style
stripping (dormant); `image_to_rect` never inspects a `style` on the source `<image>`;
`resolve_stroke` cannot see stylesheet-only strokes (deliberate); two pre-existing rustdoc warnings
in `eink-dither`; `gamut/hull.rs`'s three epsilons want a comment; `adapt.rs`'s
`max_compression < 1.0` collapse is untested; no test exercises literal `NaN`.

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
   **Re-measure in linear light first.**
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
