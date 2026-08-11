# Handover — Byonk

_Last updated: 2026-08-11 (session 11). The **panel-colour pinning** initiative was
**redirected mid-flight by owner rulings 22 and 23**, re-specified and re-planned. Old
Tasks 1–2 and new Task 3 are done; **Tasks 4–8 remain**. Full `make check` green at
`574d8c5` (1051 passed, 0 failed, 0 warnings). The prior **gamut** initiative is complete.
`feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `574d8c5` — Task 3 fix round 1 |
| Pinning Task 1 | `c74312f`..`f82eedc`, reviewed clean |
| Pinning Task 2 | `89c2069`..`24ce479`, reviewed clean (2 fix rounds) |
| Amended Task 3 | `7c09875`..`574d8c5`, reviewed clean (1 fix round) |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | tree clean; **full `make check` PASSES: 1051 passed, 0 failed, 0 warnings** |
| Active spec | `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` — **read Amendment 1 + ruling 23 at the end** |
| Active plan | `docs/superpowers/plans/2026-08-11-panel-colour-pinning-amended.md` (Tasks 3–8) |
| Active ledger | `.superpowers/sdd/2026-08-11-panel-colour-pinning-amended/progress.md` (git-ignored) |
| Superseded | the 2026-08-10 plan (its Tasks 1–2 are done and valid; its `task-3-brief.md` is DEAD) and `.superpowers/sdd/2026-08-10-panel-colour-pinning/progress.md`, kept for history |
| Prior initiative | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — complete |

**Resume by:** reading the active ledger, then `git log 574d8c5..HEAD`, then dispatching
**Task 4** via `superpowers:subagent-driven-development` from the **amended** plan. The
ledger is the recovery map; trust it and `git log` over memory.

**⚠️ Task 4 is the biggest task in the plan** — `RegionMap`, per-pixel model selection, the
representative-colour rule and the boundary hard stop, all in the dither loop's inner code,
with four tests that each need real measurement. **Pre-flight its brief before dispatching**
(that has never once been clean) and put the fixture trap in the dispatch, not just the
brief.

# ⚠️⚠️ START HERE — owner ruling 22 supersedes Task 3 as planned (2026-08-10, session 11)

**The unmapped path assumes the actual colours ARE the nominal colours.** One mask
selects the colour model, and it selects three things at once:

| Region | Colour model | Gamut mapping | Pinning |
|---|---|---|---|
| **Unmarked** (structure) | **official/nominal**, substituted for actual | off | **on**, against official |
| **Marked** `continuous` | **actual/measured** | on | **off** |

This is not what the code does today, and it is not what the plan's Task 3 builds.
`palette/palette.rs:442` — `find_nearest` scans `self.actual_oklab` **unconditionally**,
mapped or not. That single site is why today's unmapped structure is matched against
measured inks.

### The evidence that produced the ruling

`builtin/default` paints its palette swatches in **nominal** colours (`layout.colors`);
`builtin/calibration/color` paints its patches in **measured** ones
(`screens/builtin/calibration/color/script.lua:16`, `device.colors_actual`). Measured off
the current renders at 800×480:

| Swatch fill (nominal) | Nearest measured ink | Rendered result |
|---|---|---|
| `#FF0000` | `#B50303` | 85% red (≈100% on non-label rows) |
| `#FFFF00` | `#FFEE00` | 89.5% yellow (≈100% on non-label rows) |
| `#00FF00` | `#0D876B` | **51% black, 27% red, 17% teal-green** |
| `#0000FF` | `#205497` | **81% black, 13% white, 5% blue** |

`calibration/color`'s patches, painted in measured values, are **>99% pure**. Red and
yellow survive only because they happen to sit near their measured inks. Pure green and
pure blue are chased toward a dark teal and a mid navy they cannot reach, and speckle.
**Under ruling 22 that is a bug, not the ditherer correctly approximating an unreachable
colour** — on the unmapped path `#00FF00` *is* green.

### What makes this cheap, already verified

- **`Palette::new(&official, None)` yields a nominal palette** — `actual` defaults to
  `official` (`palette/palette.rs:167`). No new constructor needed.
- **`build_eink_palette` dedups on official bytes** (`svg_to_png.rs:414`, `kept_indices`
  drives the output palette), so nominal and measured palettes built from the same
  official list have **identical index spaces**. Output PLTE stays valid whichever
  palette matched a pixel.

### What it costs

The dither loop needs **per-pixel palette selection driven by the mask**;
`dither_with_kernel_noise` currently takes a single `&Palette`, and
`EinkDitherer::dither_with_pinning` currently takes only `pin_eligible`. So this
**extends Task 2's API and rewrites Task 3** — it is not a byonk-side wiring change.

**Pinning is still required under ruling 22.** Nominal matching makes an exact official
colour match itself at distance zero, but error diffused *into* it can still take it
over. That is the original defect and it is unaffected by which palette is matched.

### The accepted tradeoff (owner, session 11)

**A photograph left unmarked will look pretty scary**, because nominal matching aims a
continuous-tone image at primaries the panel cannot produce. **In exchange, graphical
elements become simple to work with**: panel colours render as themselves, and even
simple transitions between them behave predictably. That is the trade the owner has
taken — it is not a defect to be fixed later.

**⚠️ This makes ruling 19's marking discipline load-bearing in a way it was not before.**
Until now, forgetting to mark continuous-tone content cost you gamut mapping — mild, and
invisible on most content. Under ruling 22 it costs you the *colour model*, on exactly
the content least able to survive it. The failure mode of a missed mark goes from mild to
severe.

Consequences to work through when re-planning:

- **`calibration/tone`'s left column is unmarked by design** as the raw-behaviour control,
  and it contains a photograph. It will now look markedly worse. That is the control doing
  its job, but confirm it is still legible enough to serve as a comparison.
- **The shipped collection is already covered** (`fe66ee6` marked `default`,
  `calibration/color`, `gphoto`; `tone` was already marked; the other 10 have no
  continuous tone). **User-authored screens with photographs are not**, and their
  rendering changes. This is a documentation and possibly a release-notes obligation —
  see ruling 21, which defers the CHANGES.md entry to merge prep.
- **Task 5 must measure the unmarked-photograph case**, not just the pinning sweep. It is
  now the feature's main downside and nobody has looked at it.
- **Screen-authoring docs (`docs/src/`) need to state the rule plainly**: mark photographs
  and gradients-through-hue, or they will render badly. Previously this was an
  optimisation; now it is a requirement.

**The spec amendment is WRITTEN and the re-plan is DONE.**
`docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` carries "Amendment 1"
plus ruling 23, with a banner at the top pointing to them.
`docs/superpowers/plans/2026-08-11-panel-colour-pinning-amended.md` carries Tasks 3–8.
**Task 3 is done.** Tasks 4–8 remain.

### Where Task 3 got to, and what it means for Task 4

Task 3 made the colour model a *parameter* and nothing more: `ColourModel::{Nominal,
Measured}`, an `official_chroma` cache built once in the constructor, and
`representative_linear(idx, model)`. **All 26 pre-existing call sites pass `Measured`, so
behaviour is unchanged** — the reviewer checked every one individually. Task 4 is what
actually turns the feature on.

`find_second_nearest` was **dropped by owner ruling**. My plan's Interfaces block had
mandated it from stale knowledge of a `dither/blue_noise.rs` that no longer exists; it had
no caller, no test, and no mutant that actually probed it. Plan and spec are corrected. **Do
not re-add it without a consumer.**

# The active initiative

## Panel-colour pinning: 2 of 5 tasks done

**The defect.** In the tone screen's **unmapped** control column, 2 px pure-black grid
lines between saturated patches come back only **73.2% black** — the rest is
red/blue/green error diffused *into* them from their neighbours. Gamut mapping costs
under 2 points, so this is a dithering effect. Scope is not the calibration screen: any
black text or logo abutting saturated content is speckled the same way.

**The design** (spec is current and owner-approved). A pixel that is **eligible** and
**is exactly a nominal palette ink** outputs that ink, ignores the error diffused into
it, and emits `λ · accumulated` — its own quantisation error being zero. λ (`pin_carry`)
decays the carry geometrically per pinned pixel. Thin structure passes error through
nearly intact (no seam); a wide flat region absorbs it within a few pixels of its edge
(no far-edge dump). λ=0 is absorb, λ=1 is pure pass-through.

### Task 1 — DONE (`c74312f`, `f82eedc`)

`DitherOptions::pin_carry` (default 0.9, clamped `[0,1]`, `#[inline]` builder) plus the
pinning branch in `dither_with_kernel_noise`, which gained a 7th parameter
`pinned: Option<&[Option<u8>]>`.

**`pinned: None` means pinning is OFF, never "eligible everywhere."** Guarded by
`no_pin_map_reproduces_the_unpinned_output_exactly`. The inverse silently changes every
existing caller's output.

Do not weaken `a_fully_absorbing_pin_isolates_what_lies_beyond_it`: it uses a pinned bar
**wider than the kernel's horizontal reach** (Atkinson max `dx` = 2, `BAR` = 4, full
height, serpentine off), so at λ=0 the field beyond is bit-identical regardless of the
field before it. No threshold, no direction. The `BAR` const exists so the guard cannot
silently stop guarding — all five consumers derive from it.

### Task 2 — DONE (`89c2069`, `51db984`, `24ce479`)

`EinkDitherer::dither_with_pinning(pixels, w, h, pin_eligible: Option<&[bool]>)`, with
`dither()` reduced to a delegation passing `None`. Plus an `#[inline] pin_carry`
passthrough on the builder for Task 5's sweep, a `debug_assert!` on pin-map length, the
corrected `preprocess/preprocessor.rs` comment, and the stale `api/builder.rs:56`
`error_clamp=0.08` doc line fixed.

**The reviewer verified the cross-task contract and found it sound _by construction_,
which is worth knowing before Task 3 touches it:** `dither/mod.rs:336` does `p[idx]`
unguarded, so a short pin map would panic rather than degrade — but the map is only
built when neither target dimension is set, and `Preprocessor::process` leaves
`result.width/height` equal to the input in exactly that case. Lengths cannot disagree
on the path where `Some` is passed. It also noted the resize guard is **stricter** than
the hazard (trips if *either* dimension is set; `process` only resamples when *both*
are) — safe direction.

### ⚠️⚠️ THE FIXTURE TRAP — the single highest-value warning for Task 4

**Several palette helpers in `eink-dither` are built with `Palette::new(x, None)`, which
sets `actual = official` (`palette.rs:167`). Under any of them the two colour models are
IDENTICAL, so a test written against one passes against every mutant, silently.**

`dither/mod.rs`'s own test module has two such helpers — `pin_test_palette()` (:680) and
`panel_palette()` (:692) — and Task 4's tests live in that very file. A fresh implementer
reaches for the module's own helper by default. **That is the trap, and it would produce a
task that passes review while testing nothing.**

The only fixture whose official and actual sets genuinely differ is
`crate::gamut::test_support::panel_measured()` (`gamut/mod.rs:40`). Probe indices 2–5
(red/yellow/blue/green); **black and white are degenerate even there**, because
`build_eink_palette` forces measured B/W to match official.

Task 3's reviewer verified this fixture-by-fixture and it is why that task's tests are
trustworthy. Put it in Task 4's dispatch verbatim.

### ⚠️⚠️ Ten plan-authored tests have now measured unfounded — four in Task 1, five in Task 2, one in Task 3

**This is the single most important thing in this file.** Every one was caught only
because an implementer or reviewer refused to accept the plan's premise.

Task 2's five, all controller-authored:

| Test as planned | What measuring it showed |
|---|---|
| `pinning_is_refused_when_resizing` | uniformly black image — all-black dithers to all-index-0 either way, so the mutant passes. **Also panics** (see the resize fact below) |
| `a_near_miss_is_not_pinned` | uniform near-black row diffuses almost no error, so pinning it changes nothing even when it fires |
| `eligibility_decides_where_pinning_applies` | flat/short content diffused too little error to distinguish pinned from unpinned |
| `the_exact_match_is_against_the_nominal_entry` | **could not distinguish `official` from `actual` at all** — nearest-match lands on ink 2 either way. The reviewer, not the implementer, caught this one |
| `plain_dither_is_unchanged_by_this_feature` | same degenerate-comparison class |

All five were replaced with a **2 px-line-in-a-hostile-field** geometry (a pure-ink line
between saturated `#C06020` against the 6-ink measured palette), each asserting its own
"this case needed rescuing" precondition before asserting the rescue. A shared
`line_ink_share` helper over a shared `HOSTILE_LINE_COLS` const means the measured region
cannot drift from the constructed one.

**The three rules now in force. Apply them to Tasks 3–5 and anything later.**

- **A test claiming "X rescues this case" must assert, in the same test, that the case
  needed rescuing.**
- **A comparison test must assert its comparison is non-degenerate.** Two runs that agree
  prove nothing if a mutant collapses both.
- **NEW (Task 2): a doc comment asserting a mutation property is an unverified claim.**
  Check it against the executed mutation table like any other. This bit three times in
  one task — the brief's framing, then `pinning_is_refused_when_resizing`'s "fixed on
  both counts", then two comments introduced *by the fix for the first two*. Each time
  the disproving measurement was already sitting in the implementer's own report.

Task 3's addition: `the_chroma_coupling_term_follows_the_model` as I wrote it was a
*ranking* probe through `find_second_nearest`, and it is structurally unfounded on this
fixture **at every grey level** — kchroma=10 keeps every chromatic entry's distance above
black/white's regardless of which chroma cache is read, so the ranking can never flip. The
implementer replaced it with a direct `distance()` call holding pixel, palette colour,
pixel chroma and index fixed and varying only `model`. **The plan now carries the corrected
probe with a note not to "simplify" it back into a ranking.**

**Corollary for writing Task 4–8 briefs:** put the plan's test bodies in as *hypotheses to
measure*, and say so explicitly in the dispatch. Tell the implementer that a mutant
surviving its named test is a plan defect to report, not a value to tune. **That single
instruction has caught all ten.**

**And a mutation-table lesson from Task 3:** a row that mutates several sites at once
proves nothing about any one of them. Row 4 flipped all three `match model` arms together,
so its failures were attributable to the other two methods and `find_second_nearest`'s
selection was never actually probed. **Write one mutant per site.**

### ⚠️ NEW environment fact: this build cannot resample

`resize_lanczos` **panics on any real dimension change** — there is no `image` crate
backend compiled into `eink-dither`. This is the same root cause as the three
"pre-existing failures" flagged below, and it is why Task 2's resize test uses a
**no-op-dimension** resize (`.resize(32,32)` on a 32×32 input): the guard is on
`PreprocessOptions` being *set*, not on dimensions actually changing, so the
configuration path is still exercised. The index-misalignment half of that guard's
rationale is **unverifiable in this build**, and the test's doc comment now says so
plainly. Do not read that green test as covering it.

Note `image` *is* a real dependency of the **byonk** crate (`Cargo.toml:17`, with `png`)
— the limitation is eink-dither's build only. Task 3's test may decode PNGs freely.

## ⚠️ Task 3 is pre-flighted — read this before dispatching

Brief: `.superpowers/sdd/2026-08-10-panel-colour-pinning/task-3-brief.md`.
Verified correct: `has_tone_markup`, `rasterize_tone_mask`, `RenderError::Dither`, and
the `render_to_palette_png` 7-arg shape all match the brief.

**The restructure the brief describes is the right one.** Today `rasterize_tone_mask` sits
*inside* the `amount != 0.0` gate (`svg_to_png.rs:133-159`), so pinning would never get a
mask when gamut is off. The mask must be rasterized whenever the document carries markup,
with only the *mapping* skipped at amount zero. Note the consequence: a marked document
now pays for a second rasterization even with gamut disabled. Intended.

Two findings to hand the implementer:

1. **`DisplaySpec` is `{width, height, max_size_bytes}` with no visible `Default` derive**
   (`src/models/display_spec.rs:5`), so the brief's `..Default::default()` may not
   compile. Construct as neighbouring tests do.
2. **The brief's test has the same defect as Task 2's five.** It asserts only the *after*
   state (black share > 0.99) and relegates the "before" to a manual Step 2, so nothing
   in the body asserts the case needed rescuing. **A good fix is available:**
   `svg_to_png.rs:864` already has a marked-vs-unmarked comparison test to copy from, so
   the same geometry with the black bar wrapped in `data-byonk-tone="continuous"` gives
   an in-test control — it should stay eroded, because marking makes it *ineligible*.
   That asserts non-degeneracy **and** covers the mask-inversion mutant the brief
   otherwise punts to Task 4. Offer it as a hypothesis to measure, not a mandate.

### Tasks 4–8 of the AMENDED plan — NOT STARTED

- **Task 4** — `RegionMap` in `dither/mod.rs`: per-pixel model, the representative-colour
  rule, and the boundary hard stop. **The big one.** Replaces Task 1's `pinned` parameter.
- **Task 5** — `dither_with_regions` on the builder, replacing `dither_with_pinning`.
  **Polarity flips here**: the crate now takes the tone mask itself, not its inverse. That
  slip is silent and produces a plausible image either way, so it has its own guard.
- **Task 6** — byonk passes the mask through unchanged. **This changes the rendering of
  every unmarked screen**, since unmarked content now matches nominal inks.
- **Task 7** — the tone screen's backing rect leaves the marked group. Pure black is in
  gamut so the mapped patches cannot move, but those pixels leave the adaptation group and
  **`R` is a 99th percentile over that set — measure it before and after.**
- **Task 8** — the measurement pass: λ sweep, the unmarked-photograph cost, the boundary
  artefact, the swatch win, per-frame cost.

### Carried forward as a deliberate decision, not an omission

`error_clamp` does **not** bound the pinned path's carry. A normal pixel's contribution is
bounded via `apply_error`; a pinned pixel forwards raw `accumulated * pin_carry`. **Inert
at the live default** — `error_clamp` is uniformly `1.0` for every algorithm
(`dither/mod.rs:118`).

# The screen collection is marked (`fe66ee6`) — done, out of plan

Applies ruling 19 across all 13 shipped screens; **10 needed nothing**. Marked:
`builtin/default` (background photograph), `builtin/calibration/color` (each gradient
bar, hue sweep, photo), `examples/gphoto` (full-screen photo). `builtin/calibration/tone`
was already marked.

**The argument for leaving an achromatic gradient unmarked is the one to remember.** Grey
is always in gamut, so mapping it is a *no-op* — while marking it switches exact-match
pinning *off* across a deliberate dithering test pattern. Marking costs something and buys
nothing. Any future "should this be marked?" question should start by asking whether the
content can even be out of gamut.

**Marking goes on the element that IS continuous-tone, never on a group around a band.**
On `calibration/color` the gradient bar and its label are emitted from the same loop body,
so a `<g>` wrapper would have swallowed the label.

### ⚠️ The mask rasterizes in document order, and that does real work

Measured at 800×480: `calibration/color` marks **262,672 / 384,000** px; `default` marks
**309,125 / 384,000 (80.5%)** — even though its photo is *full-screen*. Unmarked elements
drawn **after** a marked one paint black back over it in the mask document, so `default`'s
hero text, palette swatches and white info bar punch themselves out of the photo's marked
area. Those pixels are opaquely covered, so excluding them is correct.

**Consequence for authors:** text over a photo needs no special handling as long as it
comes later in document order. A group wrapper would still capture it.

### Untracked duplicate — do not sweep it in

`/Users/oetiker/checkouts/byonk/examples/` is an **untracked** near-copy of
`screens/examples/`, now drifted by one file (`gphoto/screen.svg`). This is exactly the
kind of local file that makes `git add -A` dangerous here.

## Open owner decisions

1. **Look at the three newly-marked screens on the panel.** Committed but unjudged. This
   is the outstanding action and has been for two sessions.
2. **The branch.** Still HELD. Eleven sessions of work sitting unmerged.

# The prior initiative: gamut mapping (complete)

Rulings 16 and 17 are implemented, measured and committed. The mapper compresses along a
ray from mid-grey; knee default 0.99. All four measured panel inks come back at
`t_max = 1.000` — yellow, which the fixed-`L` mapper stranded at 42%, is now
indistinguishable from red, blue and green.

**Ruling 16 and ruling 17 are only safe together.** The ray's liability is near-white
tints: a high-`L`, low-chroma colour's ray exits the hull at the *white point*, so it reads
as boundary-saturated even though its chroma was never out of gamut. At knee 0.8 this
darkens `grey 250 tint 4` by **−0.084**; at 0.99 by −0.0035. **Do not lower the knee
without re-measuring** — `gamut::mapper::tests::ray_geometry_diagnostic` prints the table,
`a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee` guards it. Whole-image mean
`|dL|` hides this completely.

**The port is proven equivalent to the prototype, pointwise** — `cusp_anchored_vs_fixed_lightness`,
swept over 5832 colours across the sRGB cube, worst channel diff 0. **Keep this check
working**; it is what makes every other number in that file trustworthy.

Per-pixel cost: **218 ms for a worst-case 800×480 frame**. **No `t_max` lookup table was
built, deliberately** — a sibling to `CmaxTable` would have inherited its bilinear
*overshoot at the pinch* (yellow: exact 0.073 vs sampled 0.093), and an overshot `t_max`
maps pixels outside the hull in exactly the region the change exists to fix.

# ⚠️⚠️ Read this before trusting any dithering picture

**Every visual dither comparison in this tree reads about 30% too dark, and it is the
viewer's fault, not the ditherer's.**

| | mean LINEAR luminance | mean GAMMA-SPACE byte |
|---|---|---|
| portrait | **+10.2%** vs source | −32.4% vs source |
| background | **+4.4%** vs source | −29.3% vs source |

Error diffusion preserves brightness in linear light. A viewer downscaling a PNG without
linearising averages sRGB bytes directly, under-weighting bright pixels in a black/ink
speckle. On the panel the eye averages optically, in linear light.

- **Relative** comparisons between variants remain valid. Use the `-mapped` (undithered)
  renders for absolute judgements.
- **Open defect 1 below was diagnosed this way and should be re-measured in linear light
  before anyone chases it.**

# ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading** — a flat patch is a single colour; every artifact
that matters is at a boundary *between* colours. Task 2 rediscovered this the hard way:
four of its five planned tests failed precisely because flat content diffuses no error.

**Whole-image means are equally misleading.** On the portrait all four anchors scored
0.0545–0.0550 mean chroma and looked identical, because only 7% of pixels are out of gamut
and the untouched 93% swamped them. Restricted to the pixels the mapper acts on, the
spread was 68% to 90%. **Measure the pixels the change touches.**

**Look to find what to measure; measure to decide.** When comparing an old behaviour to a
new one, **render both from the same input in the same image**.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run. Never take a
subagent's "all green" at face value.

# ⚠️⚠️ Read this before dispatching any subagent

**`make check` takes ~10 minutes in this tree.** The subagent stream watchdog fires at
600 s of silence, so **an implementer that runs `make check` in the foreground dies
mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` then
  `CARGO_BUILD_JOBS=2 cargo test -p <crate> --lib`. Say so in the brief.
- **The controller runs the full gate** in a **backgrounded** Bash call
  (`run_in_background: true`) and polls. Do not chain foreground `sleep` calls to wait —
  the harness blocks them; use `run_in_background` with an `until` loop.

**Subagent task briefs scoped to `--lib` cannot see integration tests.** That restriction
exists for the watchdog, and it is why both builtin-inventory guards survived three clean
task reviews in session 9. The controller's full gate is not a formality.

**Tell the implementer the pre-existing failures.** `cargo test -p eink-dither --lib --
--ignored` reports **3 pre-existing failures unrelated to any current work**:
`preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
test_resize_full_pipeline_with_photo_preset}`, which panic at `resize_lanczos` **by design**
(no `image` backend — see the resize fact above). An implementer that does not know this
wastes a round.

**Give the reviewer the cross-task risk to check, by name.** Task 2's reviewer was told
which unchanged function consumed the map it built, and returned a by-construction proof
instead of a shrug. A named risk earns its cost; "check all uses" does not.

When an implementer stalls, **do not resume it blindly** — assess the abandoned working
tree first.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **~10 min — background it.** **Green at `24ce479`.**
- **`make check` runs `cargo fmt`, not `cargo fmt --check`.** It rewrites files in place
  and leaves the tree dirty. Put `cargo fmt` in the implementer's command list.
- **The clippy gate is `-D warnings`.** A single warning anywhere in the workspace fails
  the gate — including in test modules. Session 11 found one (`i32 -> i32` cast) that a
  `--lib`-scoped implementer run had not surfaced.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **`make check` does not run the `#[ignore]` tests**, and most gamut evidence is ignored:
  - `cargo test -p eink-dither --lib gamut::mapper::tests::ray_geometry_diagnostic -- --ignored --nocapture`
  - `cargo test --release -p eink-dither --lib map_frame_cost -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture`
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- Output PNGs land in `target/dither-compare/`.
- **Production applies no preprocessing before dithering** — `src/rendering/svg_to_png.rs`
  goes `rgba → Srgb → (gamut map) → dither`.
- **Rendering a builtin screen needs a device, and the old plan's CLI is wrong.** It is
  `render --mac <MAC> --output <PATH>`, resolved through config. Do **not** edit the
  tracked `config.yaml` — copy it, point `CONFIG_FILE` at the copy, add a throwaway device:
  ```yaml
    "AA:BB:CC:DD:EE:01":
      panel: reterminal_e1002          # WITHOUT this you get a GREYSCALE render
      screen: byonk-builtin/calibration/tone
  ```
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
  **The missing-panel trap is silent** — it renders happily, in greyscale.
- **Before adding a builtin screen, grep for what enumerates the inventory.** Two tests
  hardcode the shipped count as an exact `assert_eq!` on purpose:
  `tests/builtin_package.rs:44` and `tests/screen_schemas_test.rs:128`. Update them, never
  loosen them.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the workspace
  cannot resolve `eink-dither`. Releases unaffected (`Dockerfile.release`, CI-built
  binaries). Out of scope, untouched.
- `make docs` needs `mdbook-mermaid`.

## Useful test assets

`screens/builtin/calibration/color/photo.png` (portrait, 7% out of gamut) and
`screens/builtin/default/background.jpg` (station concourse, 12%) are byonk's own shipping
assets and are what the panel actually renders. Synthetic fields at full saturation are
unrepresentative (`ρ` p50 = 2.87 against a photo's 1.2–1.5).

## Public surface

`eink_dither::{Oklch, GamutMapper, GamutOptions}`; `gamut::hull::{Hull, HullShape}`;
`gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma` (takes `r`); `DitherOptions::pin_carry`;
**`EinkDitherer::dither_with_pinning`, `EinkDitherer::pin_carry`** (Task 2).
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey, panel_measured}` —
**import, never copy**. `six_colour`'s idealised primaries do not reproduce the hull's
pinch, so they cannot guard it.

## The lesson, now proven eleven sessions running

**The plan's code and constants are not evidence.** Measure before believing the plan, your
own diagnosis, a reviewer's "harmless", the spec — or your own eyes on a downscaled PNG.

Sessions 10–11 extend this to **the tests and the comments the plan specifies**: nine
plan-authored tests measured unfounded across two tasks, and three doc comments claimed
mutation coverage their own measurements disproved. The only things that caught them were
an implementer that reported a failure instead of adjusting the test, and a reviewer told
to check a named cross-task risk.

Session 9's, still true:

- **Adding a builtin screen has a fan-out nobody mapped** — see the inventory guards above.
- **A reviewer that fact-checks docs against code earns its cost.** Two factual errors in
  one short docs section, the second introduced *by the fix for the first*. Session 11 hit
  the identical shape in comments.
- **Measure the claim you are actually making.**
- **A ruling can carry a latent defect that only a second ruling masks.** **Measure a
  change at the settings it will actually ship with.**
- **A rewritten test is an unverified test.** Mutate every guard you touch, in both
  directions — and re-verify after a refactor, not just re-read. Task 2's implementer
  re-ran the `!resizing` mutant against the post-extraction fixture for exactly this
  reason.
- **Bound synthetic sweeps to inputs that can occur.**
- **The risk the handover flags loudest may be the cheapest to retire.**

Session 8's, still true:

- **The confident recommendation was wrong.** Cusp anchoring was the principled, cited fix
  and measured 40% against mid-grey's 82%. **Prototype before recommending; a citation is
  not a measurement.**
- **A surprising number is a lead, not noise.**
- **The owner's question was better than the controller's plan.** "Why do panel colours not
  dither to themselves?" produced ruling 17 — and, a session later, this whole initiative.
- **Fixing the code is not fixing the cause.**

Session 7's, still true:

- **Every task passed review. The feature was still wrong.** What saved it was the owner
  looking at a picture. **Ask what the tests do not assert.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.** Eleven sessions on,
  still unbroken: Task 2's pre-flight predicted two of its five bad tests, and Task 3's has
  already found two more issues.

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448 tests
passing).

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made in session
> 6 by task reviewers and the controller **while the owner was absent** — do not present
> them as settled. **13-21** are genuine owner rulings from sessions 7-10.

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
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it must be
    **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session 8, `23a1e39`).
16. **The compression direction is mid-grey anchored** (owner, session 8; implemented
    session 9, `8e30e24`). Chosen over cusp-anchored (40% vs 82% on yellow).
17. **The knee default is 0.99** (owner, session 9; implemented `868544c`). Supersedes ruling 4.
18. **Pinning is eligible everywhere outside a `continuous` region, in every document**
    (owner, session 10). Its cost — `calibration/color`'s photograph becomes eligible — is
    measured in Task 5, not assumed.
19. **The mask marks content that is continuous-tone, not regions of the layout** (owner,
    session 10). **Applied across the whole shipped collection in `fe66ee6`.**
20. **Task 5's `#[ignore]` diagnostics stay non-asserting** (owner, session 10). Plan governs
    over the review rubric; their printed output is the spike's deliverable.
21. **CHANGES.md is not touched by the pinning plan** (owner, session 10). One entry gets
    written at merge prep, covering gamut and pinning together.
23. **A model boundary IS a screen border** (owner, session 11): _"nothing from one side
    goes through to the other, like the border of the screen."_ Error does not diffuse
    between a nominal-model pixel and a measured-model pixel, in either direction; the
    pinned λ-carry obeys the same stop. **Dropped, not renormalised** — the screen border
    does not conserve error either. Each region is dithered as if it were its own frame.
    The analogy is exact: the per-pixel accumulated buffer is the ONLY state carried
    between pixels, so skipping the crossing taps is sufficient — a scanline that leaves
    and re-enters a region resumes with zero inherited error for free, so irregular and
    disjoint regions need no labelling, no separate traversal, no second pass.
    **Complementary to pinning, not a replacement:** the hard stop protects across a
    marked/unmarked boundary; pinning protects *within* one model, which is where the
    original 73.2% defect lives (the unmapped control column, where grid and patches are
    both unmarked).
22. **The unmapped path assumes actual == nominal** (owner, session 11). Unmarked content
    is matched against **official** colours and pinned against them; marked `continuous`
    content keeps **actual/measured** colours and is not pinned. One mask, three
    consumers. **Supersedes Task 3 as planned and extends Task 2's API** — see the
    section at the top of this file. Accepted cost: an unmarked photograph looks bad;
    accepted gain: graphical elements and transitions between panel colours are simple
    and predictable.

**Constants inherited from the plan and never challenged:** `PERCENTILE = 0.99`,
`MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`,
`T_HI = 6.0`, `T_STEPS = 24`, `ACHROMATIC_C = 1e-6`. **`pin_carry = 0.9` is provisional and
awaits Task 5's sweep.**

Standing: **the branch is HELD** — no PR, no merge to `main`.

## Deferred minors

Session 11 (Task 2), all in the active ledger:

- Hostile-field fixture duplicated verbatim 5× and the ink-share closure 2× in
  `builder.rs` tests (~60 lines). Partially addressed by the helper extraction; verify.
- ~65 lines of per-test comments are archaeology of the *brief* ("The brief's original
  version…") rather than of the code. `builder.rs` grew 341 lines in one commit.
- A wrong-length `pin_eligible` silently disables pinning in release builds
  (`debug_assert!` only) and has no test in either direction.
- Stale/duplicated step-numbering comments in `dither_with_pinning`.

Session 10 (Task 1):

- `f32::clamp` does not trap NaN in `pin_carry`; the field is `pub` anyway.

Session 9:

- `six_colour`'s blue vertex cannot reach the knee's design point because the constant-hue
  OKLch ray **bulges outside the linear-RGB hull**. `t_max = 0.861`. Harmless:
  `panel_measured` hits 1.000 on every ink.
- `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` still hardcode the panel palette,
  duplicating `test_support::panel_measured`. Integration tests cannot see the crate's
  `#[cfg(test)]` fixtures; exporting them is the fix if a fourth copy appears.
- `mapped_chroma` is now `#[cfg(test)]`.

Session 8:

- The `Cmax` table's bilinear sample *overshoots* where the hull pinches (yellow: exact
  0.073 vs sampled 0.093). **Now load-bearing knowledge** — it is why no `t_max` table was
  built. Do not "fix" it without re-reading that decision.

Session 7:

- `test_gamut_mapping_preserves_hue_order` would also pass against an **identity** mapper.
  Weak guard, kept deliberately.

Session 6:

- **Task 10:** the unreachable mask-length-mismatch branch returns `RenderError::Dither`, a
  misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)` **cannot**
  detect a restated-constant violation — manual-review-only.
- **Task 11:** `PanelDitherConfig` accepts a `gamut:` key in panel YAML; verify it is live.
- **Task 12 (inherited):** `resolve_effective_tuning` replaces the **whole** struct when any
  override field is set, so an active dev-UI query override resets the previewed gamut to
  default and diverges from production.

Earlier sessions:

- **Task 7:** the winning dilute start was `eps = 0.005`. Optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`; traced —
  failure mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()` while
  `rewrite_start` propagates them. Style wart.
- **Task 8:** element names matched as raw bytes, so `<svg:image>` would be mis-handled and
  `<symbol>` gets no `<defs>`-style stripping. Dormant.
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`.
- **Task 9b:** `resolve_stroke` cannot see stylesheet-only strokes. Deliberate.
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three epsilons want
  a comment; `adapt.rs`'s `max_compression < 1.0` collapse is untested; no test exercises
  literal `NaN`.

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
   **Re-measure in linear light first** — see the brightness section above.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale. **No
   working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. The benign half is
   established and asserted: a flat fill of a *measured ink* dithers to that single ink
   exactly, which is correct — **but only in isolation.** Set next to saturated content,
   27% of those same pure-ink pixels are taken over by diffused error. **This is the defect
   the active initiative addresses**, and Tasks 1–2 now hold the pixel; Task 3 wires it to
   real renders.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does not work.
`AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on both axes — changing
the default alters rendering for every device, so it is the owner's call.
