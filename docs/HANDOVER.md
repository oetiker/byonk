# Handover — Byonk

_Last updated: 2026-08-12 (session 12). **Task 4 — the biggest task in the amended plan —
is DONE, reviewed clean, gate green.** Owner **re-framed ruling 23 mid-task from "screen
border" to CONTAINMENT**, which dissolved a review finding; the spec and plan are amended
to match. **Tasks 5–8 remain.** Full `make check` green at `0d73dda` (1056 passed, 0 failed,
0 warnings). `feat/screen-store-authoring-core` remains **HELD** — no PR, no merge to
`main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `0d73dda` — Task 4 fix round 1 |
| Pinning Task 1 | `c74312f`..`f82eedc`, reviewed clean |
| Pinning Task 2 | `89c2069`..`24ce479`, reviewed clean (2 fix rounds) |
| Amended Task 3 | `7c09875`..`574d8c5`, reviewed clean (1 fix round) |
| **Amended Task 4** | **`3711dea`..`0d73dda`, reviewed clean (1 fix round)** |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | tree clean; **full `make check` PASSES: 1056 passed, 0 failed, 0 warnings** |
| Active spec | `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` — **read Amendment 1 + ruling 23 at the end; both are current as of session 12** |
| Active plan | `docs/superpowers/plans/2026-08-11-panel-colour-pinning-amended.md` (Tasks 5–8 remain) |
| Active ledger | `.superpowers/sdd/2026-08-11-panel-colour-pinning-amended/progress.md` (git-ignored) |
| Superseded | the 2026-08-10 plan (its Tasks 1–2 are done and valid; its `task-3-brief.md` is DEAD) and `.superpowers/sdd/2026-08-10-panel-colour-pinning/progress.md`, kept for history |
| Prior initiative | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — complete |

**Resume by:** reading the active ledger, then `git log 0d73dda..HEAD`, then dispatching
**Task 5** via `superpowers:subagent-driven-development` from the **amended** plan. The
ledger is the recovery map; trust it and `git log` over memory.

**⚠️ Pre-flight Task 5's brief before dispatching. Twelve sessions on, that has never once
been clean** — session 12's pre-flight found four issues, two of them Important, and both
Important ones materialised exactly as predicted.

# ⚠️⚠️ START HERE — the design, and the owner's session-12 re-framing

**The unmapped path assumes the actual colours ARE the nominal colours.** One mask
selects the colour model, and it selects three things at once:

| Region | Colour model | Gamut mapping | Pinning |
|---|---|---|---|
| **Unmarked** (structure) | **official/nominal**, substituted for actual | off | **on**, against official |
| **Marked** `continuous` | **actual/measured** | on | **off** |

**Status after Task 4:** the crate now does all three per pixel. `RegionMap { continuous,
pinned }` carries the mask, `model()` selects `Nominal`/`Measured` from it, and a hard stop
drops any kernel tap whose endpoints straddle a model boundary. **But the only production
call site — `api/builder.rs:294` — still passes a uniform all-`true` mask, so shipping
behaviour is still measured-everywhere, bit-for-bit unchanged.** Task 5 exposes the real
API; Task 6 feeds it a real mask. **Nothing a user sees has changed yet.**

## ⚠️ Ruling 23 was RE-FRAMED by the owner in session 12 — spec amended to match

The owner rejected the controller's "screen border" framing and restated the ruling:

> _"errors from a mapped region do not go into an unmapped region and the other way round
> … two dither systems both active and supposed not to step on each other's feet."_

**The property is CONTAINMENT, not spatial impermeability.** This matters because it
dissolved an Important review finding rather than causing a code change.

**The finding:** `crosses` compares only the tap's two endpoints, so a region strictly
narrower than the kernel's reach (1 px wide via Atkinson's `(2,0)`, 1 px tall via `(0,2)`)
has both endpoints on the same side and error hops over it. Under "nothing goes through"
that is a violation.

**Why it is not one under containment, verified in the code:** `error_buf.add_error(nx, dy,
…)` deposits **only at the tap's endpoint** — nothing is written to pixels the tap passes
over. So error is never *deposited* across a model boundary at any region width, 1 px
included. A tap skipping a 1 px sliver lands on a same-model pixel; the error stayed inside
its own system. **The endpoint-only guard is exactly sufficient for containment, and it was
NOT changed.**

What the metaphor was hiding, now stated in the code comment: the behaviour is
**width-dependent, in connectivity rather than containment.** Across a sliver narrower than
the kernel's reach the same-model system stays **connected** (error hops the gap and keeps
diffusing within itself); across a wider one no aimed tap can reach and that error is
dropped, mirroring the frame edge. **Both satisfy containment.**

**✅ The spec and plan now carry the corrected wording** (session 12, at the owner's
instruction). `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md`'s ruling 23
leads with containment, records the superseded border analogy as history, **retracts by
strikethrough** the two sentences that were false under it ("each region is dithered as if
it were its own frame" and "a scanline that re-enters resumes with zero inherited error"),
and carries a width-dependence table plus a new verification item 7 for the sliver case. A
banner at the top of the spec catches anyone who remembers the old framing. The amended
plan's Task 8 Step 3 is corrected too, since it drives a measurement; Task 4's background
block keeps the old wording clearly labelled as the superseded text the completed task was
built against.

### The evidence that produced rulings 22 and 23

`builtin/default` paints its palette swatches in **nominal** colours (`layout.colors`);
`builtin/calibration/color` paints its patches in **measured** ones
(`screens/builtin/calibration/color/script.lua:16`, `device.colors_actual`). Measured off
the session-11 renders at 800×480:

| Swatch fill (nominal) | Nearest measured ink | Rendered result |
|---|---|---|
| `#FF0000` | `#B50303` | 85% red (≈100% on non-label rows) |
| `#FFFF00` | `#FFEE00` | 89.5% yellow (≈100% on non-label rows) |
| `#00FF00` | `#0D876B` | **51% black, 27% red, 17% teal-green** |
| `#0000FF` | `#205497` | **81% black, 13% white, 5% blue** |

`calibration/color`'s patches, painted in measured values, are **>99% pure**. Pure green and
pure blue are chased toward a dark teal and a mid navy they cannot reach, and speckle.
**Under ruling 22 that is a bug** — on the unmapped path `#00FF00` *is* green.

### The accepted tradeoff (owner, session 11)

**A photograph left unmarked will look pretty scary**, because nominal matching aims a
continuous-tone image at primaries the panel cannot produce. **In exchange, graphical
elements become simple to work with.** That is the trade the owner has taken — not a defect
to fix later.

**⚠️ This makes ruling 19's marking discipline load-bearing.** Forgetting to mark
continuous-tone content used to cost gamut mapping; now it costs the *colour model*, on
exactly the content least able to survive it. Consequences, carried by the amended plan:

- **`calibration/tone`'s left column is unmarked by design** as the raw-behaviour control,
  and contains a photograph. It will look markedly worse — confirm it stays legible enough
  to serve as a comparison.
- **The shipped collection is covered** (`fe66ee6`); **user-authored screens with
  photographs are not**, and their rendering changes. Documentation and probably
  release-notes obligation — ruling 21 defers the CHANGES.md entry to merge prep.
- **Task 8 must measure the unmarked-photograph case**, not just the pinning sweep.
- **Screen-authoring docs (`docs/src/`) must state the rule plainly**: mark photographs and
  gradients-through-hue, or they render badly.

# The active initiative

## Panel-colour pinning: 4 of 8 tasks done (Tasks 1–4)

**The defect.** In the tone screen's **unmapped** control column, 2 px pure-black grid lines
between saturated patches come back only **73.2% black** — the rest is red/blue/green error
diffused *into* them. Scope is not the calibration screen: any black text or logo abutting
saturated content speckles the same way.

**The design.** A pixel that is **eligible** and **is exactly a nominal palette ink** outputs
that ink, ignores the error diffused into it, and emits `λ · accumulated` — its own
quantisation error being zero. λ (`pin_carry`) decays the carry geometrically per pinned
pixel. λ=0 is absorb, λ=1 is pure pass-through. **Pinning is still required under ruling
22**: nominal matching makes an exact official colour match itself at distance zero, but
error diffused *into* it can still take it over. That is the original defect and it is
unaffected by which palette is matched.

### What Task 4 landed, and what Task 5 must know

`RegionMap<'a> { continuous: &'a [bool], pinned: &'a [Option<u8>] }` in `dither/mod.rs`,
**replacing** Task 1's `pinned: Option<&[Option<u8>]>` parameter of
`dither_with_kernel_noise`. Plus `RegionMap::model()`, per-pixel model threaded into **both**
the match and the error term, and the boundary hard stop in the distribution loop.

- **`regions: None` means the feature is OFF** — measured everywhere, no pinning, no stops,
  bit-for-bit today's output. Guarded by
  `regions_none_reproduces_the_measured_unpinned_output_exactly`. Never "on everywhere".
- **`api/builder.rs` passes a uniform all-`true` mask, not `None`** — the plan said `None`,
  which would have disabled pinning outright and broken three Task-2 tests
  (`builder.rs:515/569/630`). **Task 5 replaces this placeholder with the real mask.** The
  code comment and the report both flag it as such.
- **The pinned carry has NO separate emit path.** Both branches assign the same
  `strength_error` binding and there is exactly one distribution loop with exactly one
  guard, so the stop covers the pinned carry **by construction**.
- **`pinned` is the caller's already-resolved decision.** `dither/mod.rs` reads
  `regions.and_then(|r| r.pinned[idx])` and never consults `continuous`. **Eligibility is
  resolved upstream and is NOT re-checked in the crate** — a doc comment claiming otherwise
  was the round's second Important finding. **Task 5 must therefore resolve eligibility
  itself; do not assume the dither loop enforces it.**

### Tasks 5–8 of the AMENDED plan — NOT STARTED

- **Task 5** — `dither_with_regions` on the builder, replacing `dither_with_pinning`.
  **Polarity flips here**: the crate takes the tone mask itself, not its inverse. That slip
  is silent and produces a plausible image either way, so it needs its own guard. Also where
  `builder.rs`'s placeholder uniform mask gets replaced, and where pin eligibility must
  actually be resolved against `continuous` (see above).
- **Task 6** — byonk passes the mask through unchanged. **This changes the rendering of
  every unmarked screen.**
- **Task 7** — the tone screen's backing rect leaves the marked group. Pure black is in
  gamut so the mapped patches cannot move, but those pixels leave the adaptation group and
  **`R` is a 99th percentile over that set — measure it before and after.**
- **Task 8** — the measurement pass: λ sweep, the unmarked-photograph cost, the boundary
  artefact, the swatch win, per-frame cost.

### Carried forward as a deliberate decision, not an omission

`error_clamp` does **not** bound the pinned path's carry. A normal pixel's contribution is
bounded via `apply_error`; a pinned pixel forwards raw `accumulated * pin_carry`. **Inert at
the live default** — `error_clamp` is uniformly `1.0` for every algorithm
(`dither/mod.rs:118`).

# ⚠️⚠️ THE FIXTURE TRAP — still the highest-value warning for every remaining task

**Several palette helpers in `eink-dither` are built with `Palette::new(x, None)`, which sets
`actual = official` (`palette.rs:167`). Under any of them the two colour models are
IDENTICAL, so a test written against one passes against every mutant, silently.**

`dither/mod.rs`'s own test module has two — `pin_test_palette()` and `panel_palette()` — and
Tasks 4's tests live in that very file. A fresh implementer reaches for the module's own
helper by default. **That is the trap.**

The only fixture whose official and actual sets genuinely differ is
`crate::gamut::test_support::panel_measured()` (`gamut/mod.rs:40`). Probe indices 2–5
(red/yellow/blue/green); **black and white are degenerate even there.**

Tasks 3 and 4 both had their fixtures audited test-by-test by the reviewer, which is why
their tests are trustworthy. **Put this in every remaining dispatch verbatim.** Task 4's
audit also found the one legitimate use of degenerate black: the pinned branch writes the
ink index directly and never calls `find_nearest` or `representative_linear`, so a pinned
pixel consults **no** model and a test about the *carry* may use black freely.

# ⚠️⚠️ Eleven plan-authored tests have measured unfounded — and in session 12, one finally survived

**This is still the single most important thing in this file.** Every unfounded one was
caught only because an implementer or reviewer refused to accept the plan's premise.

Running tally: **4 (Task 1) + 5 (Task 2, one caught by the reviewer) + 1 (Task 3) + 1 (Task
4)** = 11.

**Task 4's was `a_pinned_pixel_on_the_boundary_emits_nothing_across_it`**, and it failed in a
new way worth recognising: it *could not attribute anything to the thing it named*. It pinned
one column at the seam, but Atkinson reaches `dx = 2`, so the **unpinned** column at
`SPLIT-2` also tapped across — any bleed it detected was ordinary diffusion, making it a
silent duplicate of the general boundary test. It also carried no non-degeneracy assertion.
Fixed by pinning the **full kernel-reach band** via a named const checked against the kernel
at runtime, so every crossing tap provably originates on a pinned pixel.

**The counter-example, and the first of its kind here:** the containment test offered in the
fix round — hold a 1 px marked sliver fixed, vary the surrounding unmarked field, assert the
sliver does not move — **was measured and it discriminated** (fails with the guard
hand-removed). Offering a test as a hypothesis does not mean expecting it to die; it means
the measurement decides.

**The rules now in force. Apply them to Tasks 5–8.**

- **A test claiming "X rescues this case" must assert, in the same test, that the case needed
  rescuing.**
- **A comparison test must assert its comparison is non-degenerate.**
- **A doc comment asserting a mutation or invariant property is an unverified claim.** Check
  it against the executed mutation table like any other. Three times in Task 2, twice more in
  Task 4 — **including once in code written to fix the first instance.**
- **NEW (Task 4): a test must be able to attribute its result to the mechanism it names.**
  Non-degeneracy is not enough; ask what *else* could produce the same pass.
- **NEW (Task 4): a mutation-table row can describe an IMPOSSIBLE mutation.** Row 5 ("pinned
  carry skips the boundary test") had no second emit path to mutate — the implementer
  reported it as a plan defect and built a *named proxy mutant* to show the test was still
  sound, rather than inventing a code path to satisfy the table. **That is the correct
  response; expect and reward it.**
- **Write one mutant per site.** A row that flips several sites at once proves nothing about
  any one of them (Task 3's row 4).

**Corollary for writing Tasks 5–8 briefs:** put the plan's test bodies in as *hypotheses to
measure*, and say so explicitly in the dispatch. Tell the implementer that a mutant surviving
its named test is a plan defect to report, not a value to tune. **That single instruction has
caught all eleven.**

# ⚠️ Pre-flight findings from session 12 that generalise

Both Important findings were structural, not typos, and both would have cost a round:

- **A plan step that says "pass `None` here so it compiles" can silently disable a feature an
  earlier task's tests assert.** The brief's Step 5 would have broken three Task-2 tests.
  Before accepting any "temporary placeholder" instruction, **grep for what asserts the
  behaviour being placeholdered.** The fix — a uniform mask that is behaviour-preserving by
  construction — was better than the plan's.
- **Check whether a mutation-table row is reachable at all** against the code's actual
  structure, before dispatching. Shared code paths make some mutants impossible to express.

Also verified once and worth not re-deriving: **Atkinson has divisor 8, `max_dy` 2, and its
entries include `(2,0)`, so its maximum horizontal reach is |dx| = 2.**

### ⚠️ NEW environment fact (session 11): this build cannot resample

`resize_lanczos` **panics on any real dimension change** — no `image` crate backend is
compiled into `eink-dither`. Same root cause as the three "pre-existing failures" below.
Note `image` *is* a real dependency of the **byonk** crate (`Cargo.toml:17`) — the limitation
is eink-dither's build only.

# The screen collection is marked (`fe66ee6`) — done, out of plan

Applies ruling 19 across all 13 shipped screens; **10 needed nothing**. Marked:
`builtin/default` (background photograph), `builtin/calibration/color` (each gradient bar,
hue sweep, photo), `examples/gphoto` (full-screen photo). `builtin/calibration/tone` was
already marked.

**The argument for leaving an achromatic gradient unmarked is the one to remember.** Grey is
always in gamut, so mapping it is a *no-op* — while marking it switches exact-match pinning
*off* across a deliberate dithering test pattern. **Any future "should this be marked?"
question should start by asking whether the content can even be out of gamut.**

**Marking goes on the element that IS continuous-tone, never on a group around a band.** On
`calibration/color` the gradient bar and its label come from the same loop body, so a `<g>`
wrapper would have swallowed the label.

### ⚠️ The mask rasterizes in document order, and that does real work

At 800×480: `calibration/color` marks **262,672 / 384,000** px; `default` marks **309,125 /
384,000 (80.5%)** — even though its photo is *full-screen*. Unmarked elements drawn **after**
a marked one paint black back over it, so `default`'s hero text, swatches and white info bar
punch themselves out of the photo's marked area. Those pixels are opaquely covered, so
excluding them is correct. **For authors: text over a photo needs no special handling as long
as it comes later in document order.**

### Untracked duplicate — do not sweep it in

`/Users/oetiker/checkouts/byonk/examples/` is an **untracked** near-copy of
`screens/examples/`, drifted by one file (`gphoto/screen.svg`). Exactly the kind of local file
that makes `git add -A` dangerous here.

## Open owner decisions

1. **The panel judgement on the marked screens — deliberately DEFERRED to after Task 6.**
   The owner's look at the session-11 renders (`~/byonk-marked-screens/`) is **what produced
   rulings 22 and 23**, so the review already paid for itself. Judging them now would measure
   a build about to change: Task 6 makes every unmarked screen match nominal inks. Re-render
   after Task 6 and judge then. **Sequenced, not stale.**
2. **The branch.** Still HELD. Twelve sessions of work sitting unmerged.

# The prior initiative: gamut mapping (complete)

Rulings 16 and 17 are implemented, measured and committed. The mapper compresses along a ray
from mid-grey; knee default 0.99. All four measured panel inks come back at `t_max = 1.000`.

**Ruling 16 and ruling 17 are only safe together.** The ray's liability is near-white tints:
a high-`L`, low-chroma colour's ray exits the hull at the *white point*, so it reads as
boundary-saturated even though its chroma was never out of gamut. At knee 0.8 this darkens
`grey 250 tint 4` by **−0.084**; at 0.99 by −0.0035. **Do not lower the knee without
re-measuring** — `gamut::mapper::tests::ray_geometry_diagnostic` prints the table,
`a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee` guards it. Whole-image mean
`|dL|` hides this completely.

**The port is proven equivalent to the prototype, pointwise** —
`cusp_anchored_vs_fixed_lightness`, swept over 5832 colours across the sRGB cube, worst
channel diff 0. **Keep this check working.**

Per-pixel cost: **218 ms for a worst-case 800×480 frame**. **No `t_max` lookup table was
built, deliberately** — a sibling to `CmaxTable` would have inherited its bilinear *overshoot
at the pinch* (yellow: exact 0.073 vs sampled 0.093), and an overshot `t_max` maps pixels
outside the hull in exactly the region the change exists to fix.

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
that matters is at a boundary *between* colours. Task 2 rediscovered this the hard way: four
of its five planned tests failed precisely because flat content diffuses no error.

**Whole-image means are equally misleading.** On the portrait all four anchors scored
0.0545–0.0550 mean chroma and looked identical, because only 7% of pixels are out of gamut
and the untouched 93% swamped them. Restricted to the pixels the mapper acts on, the spread
was 68% to 90%. **Measure the pixels the change touches.**

**Look to find what to measure; measure to decide.** When comparing an old behaviour to a new
one, **render both from the same input in the same image**.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run — session 11 saw a
phantom "unresolved import" that `cargo` disproved. Never take a subagent's "all green" at
face value.

# ⚠️⚠️ Read this before dispatching any subagent

**`make check` takes ~10 minutes in this tree.** The subagent stream watchdog fires at 600 s
of silence, so **an implementer that runs `make check` in the foreground dies mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` then
  `CARGO_BUILD_JOBS=2 cargo test -p <crate> --lib`. Say so in the brief.
- **The controller runs the full gate** in a **backgrounded** Bash call
  (`run_in_background: true`) and polls with an `until` loop. Foreground `sleep` is blocked.

**Subagent task briefs scoped to `--lib` cannot see integration tests.** That restriction
exists for the watchdog, and it is why both builtin-inventory guards survived three clean
task reviews in session 9. The controller's full gate is not a formality.

**Tell the implementer the pre-existing failures.** `cargo test -p eink-dither --lib --
--ignored` reports **3 pre-existing failures unrelated to any current work**:
`preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
test_resize_full_pipeline_with_photo_preset}`, which panic at `resize_lanczos` **by design**.
An implementer that does not know this wastes a round.

**Give the reviewer the cross-task risk to check, by name.** Task 2's reviewer was told which
unchanged function consumed the map it built and returned a by-construction proof instead of
a shrug. Task 4's was given three named risks and cleared all three with mechanism — including
the one that mattered, that `dither_with_kernel_noise` has exactly **one** production call
site. **A named risk earns its cost; "check all uses" does not.**

**Hand deviations to a reviewer as "judge these on their merits", never as "these were
approved."** Task 4 declared three; all three were upheld *with mechanism*, which is worth
far more than acceptance.

When an implementer stalls, **do not resume it blindly** — assess the abandoned working tree
first.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **~10 min — background it.** **Green at `0d73dda`: 1056 passed,
  0 failed, 0 warnings.**
- **`make check` runs `cargo fmt`, not `cargo fmt --check`.** It rewrites files in place and
  leaves the tree dirty. Put `cargo fmt` in the implementer's command list.
- **The clippy gate is `-D warnings`**, including test modules. A `--lib`-scoped implementer
  run will not surface everything.
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
  `render --mac <MAC> --output <PATH>`, resolved through config. Do **not** edit the tracked
  `config.yaml` — copy it, point `CONFIG_FILE` at the copy, add a throwaway device:
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
`EinkDitherer::dither_with_pinning`, `EinkDitherer::pin_carry` (Task 2);
**`palette::ColourModel`, `Palette::{find_nearest(_, model), representative_linear(idx,
model)}` (Task 3); `dither::RegionMap` — `pub(crate)` (Task 4).**
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`,
`DeviceContext::dither_gamut_{knee,amount,max_compression}`, `lua_runtime::ScriptResult::gamut`,
`svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey, panel_measured}` —
**import, never copy**. `six_colour`'s idealised primaries do not reproduce the hull's pinch,
so they cannot guard it.

## The lesson, now proven twelve sessions running

**The plan's code and constants are not evidence.** Measure before believing the plan, your
own diagnosis, a reviewer's "harmless", the spec — or your own eyes on a downscaled PNG.

Sessions 10–12 extend this to **the tests and the comments the plan specifies**: eleven
plan-authored tests measured unfounded, and five doc comments claimed properties their own
code disproved. What caught them: implementers that reported a failure instead of adjusting
the test, and reviewers told to check a named cross-task risk.

**Session 12 adds one the others did not cover: the owner's framing of their own ruling can
be the thing that is wrong.** Ruling 23 was carried for a full session as "a screen border",
and a reviewer found a real gap against that wording. The owner's re-framing to *containment*
dissolved the gap without a line of logic changing — the code had been right and the
*metaphor* had been wrong. **When a finding says the code violates a ruling, check what the
ruling is actually protecting before changing the code.**

Session 9's, still true:

- **Adding a builtin screen has a fan-out nobody mapped** — see the inventory guards above.
- **A reviewer that fact-checks docs against code earns its cost.** Two factual errors in one
  short docs section, the second introduced *by the fix for the first*. Sessions 11 and 12 hit
  the identical shape in comments.
- **Measure the claim you are actually making.**
- **A ruling can carry a latent defect that only a second ruling masks. Measure a change at
  the settings it will actually ship with.**
- **A rewritten test is an unverified test.** Mutate every guard you touch, in both
  directions — and re-verify after a refactor, not just re-read.
- **Bound synthetic sweeps to inputs that can occur.**
- **The risk the handover flags loudest may be the cheapest to retire.**

Session 8's, still true:

- **The confident recommendation was wrong.** Cusp anchoring was the principled, cited fix and
  measured 40% against mid-grey's 82%. **Prototype before recommending; a citation is not a
  measurement.**
- **A surprising number is a lead, not noise.**
- **The owner's question was better than the controller's plan.** "Why do panel colours not
  dither to themselves?" produced ruling 17 — and, a session later, this whole initiative. It
  happened again in session 11 (an owner looking at a render produced rulings 22 and 23) and
  **again in session 12**, where a one-sentence re-framing retired an Important finding.
  **Budget for that, and render something to look at early.**
- **Fixing the code is not fixing the cause.**

Session 7's, still true:

- **Every task passed review. The feature was still wrong.** What saved it was the owner
  looking at a picture. **Ask what the tests do not assert.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.** Twelve sessions on,
  still unbroken.

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448 tests passing).

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made in session 6
> by task reviewers and the controller **while the owner was absent** — do not present them as
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
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it must be
    **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session 8, `23a1e39`).
16. **The compression direction is mid-grey anchored** (owner, session 8; implemented session
    9, `8e30e24`). Chosen over cusp-anchored (40% vs 82% on yellow).
17. **The knee default is 0.99** (owner, session 9; implemented `868544c`). Supersedes ruling 4.
18. **Pinning is eligible everywhere outside a `continuous` region, in every document**
    (owner, session 10). Its cost — `calibration/color`'s photograph becomes eligible — is
    measured in Task 8, not assumed.
19. **The mask marks content that is continuous-tone, not regions of the layout** (owner,
    session 10). **Applied across the whole shipped collection in `fe66ee6`.**
20. **Task 5's `#[ignore]` diagnostics stay non-asserting** (owner, session 10). Plan governs
    over the review rubric; their printed output is the spike's deliverable.
21. **CHANGES.md is not touched by the pinning plan** (owner, session 10). One entry gets
    written at merge prep, covering gamut and pinning together.
22. **The unmapped path assumes actual == nominal** (owner, session 11). Unmarked content is
    matched against **official** colours and pinned against them; marked `continuous` content
    keeps **actual/measured** colours and is not pinned. One mask, three consumers. Accepted
    cost: an unmarked photograph looks bad; accepted gain: graphical elements and transitions
    between panel colours are simple and predictable.
23. **Error is CONTAINED within a colour model** (owner, session 11; **re-framed by the owner
    in session 12 — this wording supersedes the "screen border" one**): _"errors from a mapped
    region do not go into an unmapped region and the other way round … two dither systems both
    active and supposed not to step on each other's feet."_ Error is never **deposited** across
    a model boundary, in either direction; the pinned λ-carry obeys the same stop. **Dropped,
    not renormalised.** Since a tap deposits only at its endpoint, dropping taps whose
    endpoints straddle the boundary is **exactly sufficient**, at any region width. The
    resulting behaviour is width-dependent in **connectivity**, not containment — see the ⚠️
    section at the top. **Complementary to pinning, not a replacement:** the stop protects
    across a marked/unmarked boundary; pinning protects *within* one model, which is where the
    original 73.2% defect lives. **The spec and plan carry this corrected wording as of session 12.**

**Constants inherited from the plan and never challenged:** `PERCENTILE = 0.99`,
`MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`, `T_HI = 6.0`,
`T_STEPS = 24`, `ACHROMATIC_C = 1e-6`. **`pin_carry = 0.9` is provisional and awaits Task 8's
sweep.**

Standing: **the branch is HELD** — no PR, no merge to `main`.

## Deferred minors

Session 12 (Task 4), all in the active ledger — **and the ledger holds the full text; the
final whole-branch review must triage it**:

- Mutation-table row 5 remains unreachable by construction; documented, not faked.
- `builder.rs`'s uniform `continuous: true` mask is a **placeholder Task 5 must replace**.

Session 11 (Task 3):

- **TWO stale rustdoc references to the deleted `find_second_nearest`** survive at
  `palette.rs:38` (`DistanceMetric::HyAB`) and `palette.rs:355-358` (`for_error_diffusion`).
  Both PREDATE that task and exist as historical rationale for the kchroma=10 tuning decision,
  but they now name a method that does not exist, in user-facing rustdoc. Two-line fix. **For
  the final whole-branch review to triage.**

Session 11 (Task 2):

- Hostile-field fixture duplicated verbatim 5× and the ink-share closure 2× in `builder.rs`
  tests. Partially addressed by the helper extraction; verify.
- ~65 lines of per-test comments are archaeology of the *brief* rather than of the code.
- A wrong-length `pin_eligible` silently disables pinning in release builds (`debug_assert!`
  only) and has no test in either direction.
- Stale/duplicated step-numbering comments in `dither_with_pinning`.

Session 10 (Task 1):

- `f32::clamp` does not trap NaN in `pin_carry`; the field is `pub` anyway.

Session 9:

- `six_colour`'s blue vertex cannot reach the knee's design point because the constant-hue
  OKLch ray **bulges outside the linear-RGB hull**. `t_max = 0.861`. Harmless.
- `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` still hardcode the panel palette,
  duplicating `test_support::panel_measured`. Integration tests cannot see the crate's
  `#[cfg(test)]` fixtures; exporting them is the fix if a fourth copy appears.
- `mapped_chroma` is now `#[cfg(test)]`.

Session 8:

- The `Cmax` table's bilinear sample *overshoots* where the hull pinches (yellow: exact 0.073
  vs sampled 0.093). **Now load-bearing knowledge** — it is why no `t_max` table was built.

Session 7:

- `test_gamut_mapping_preserves_hue_order` would also pass against an **identity** mapper.
  Weak guard, kept deliberately.

Session 6:

- **Task 10:** the unreachable mask-length-mismatch branch returns `RenderError::Dither`, a
  misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)` **cannot** detect
  a restated-constant violation — manual-review-only.
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
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three epsilons want a
  comment; `adapt.rs`'s `max_compression < 1.0` collapse is untested; no test exercises literal
  `NaN`.

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
   **Re-measure in linear light first** — see the brightness section above.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale. **No
   working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. The benign half is
   established and asserted: a flat fill of a *measured ink* dithers to that single ink
   exactly, which is correct — **but only in isolation.** Set next to saturated content, 27%
   of those same pure-ink pixels are taken over by diffused error. **This is the defect the
   active initiative addresses**, and Tasks 1–4 now hold the pixel; Tasks 5–6 wire it to real
   renders.

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does not work.
`AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on both axes — changing the
default alters rendering for every device, so it is the owner's call.
