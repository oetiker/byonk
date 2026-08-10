# Handover — Byonk

_Last updated: 2026-08-10 (session 10). The gamut initiative is **paused, complete
and committed**. A new initiative — **panel-colour pinning** — is **1 of 5 tasks
done** and under SDD subagent execution. Separately, **the whole shipped screen
collection has now been marked** (`fe66ee6`), so the gamut feature reaches real
content screens for the first time. `feat/screen-store-authoring-core` remains
**HELD** — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `fe66ee6` — continuous-tone marking across the screen collection |
| Pinning Task 1 | `f82eedc`, reviewed clean |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | tree clean; eink-dither lib **207 passed, 0 failed, 21 ignored**; byonk lib **451 passed, 0 failed, 1 ignored** |
| Active spec | `docs/superpowers/specs/2026-08-10-panel-colour-pinning-design.md` |
| Active plan | `docs/superpowers/plans/2026-08-10-panel-colour-pinning.md` |
| Active ledger | `.superpowers/sdd/2026-08-10-panel-colour-pinning/progress.md` (git-ignored) |
| Prior initiative | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` — complete |

**Resume by:** reading the active ledger, then `git log fe66ee6..HEAD`, then
dispatching **Task 2** via `superpowers:subagent-driven-development`. The ledger is
the recovery map; trust it and `git log` over memory. Note the ledger covers the
pinning plan only — the screen marking (`fe66ee6`) landed outside it and is recorded
here instead.

# ⚠️ START HERE — the active initiative

## Panel-colour pinning: 1 of 5 tasks done

**The defect.** In the tone screen's **unmapped** control column, 2 px pure-black
grid lines between saturated patches come back only **73.2% black** — the rest is
red/blue/green error diffused *into* them from their neighbours. Gamut mapping costs
under 2 points (mapped column 71.4%), so this is a dithering effect. Scope is not the
calibration screen: any black text or logo abutting saturated content is speckled the
same way.

**The design** (spec is current and owner-approved). A pixel that is **eligible** and
**is exactly a nominal palette ink** outputs that ink, ignores the error diffused into
it, and emits `λ · accumulated` — its own quantisation error being zero. λ (`pin_carry`)
decays the carry geometrically per pinned pixel, so at depth *n* the surviving
fraction is `λⁿ`. Thin structure passes error through nearly intact (no seam); a wide
flat region absorbs it within a few pixels of its edge (no far-edge dump). λ=0 is
absorb, λ=1 is pure pass-through — one knob spanning both variants originally
proposed.

### Owner rulings on this initiative (2026-08-10)

18. **Pinning is eligible everywhere outside a `continuous` region, in every
    document**, including documents with no tone markup at all. Taken over the
    narrower "only in documents that carry tone markup" gate. Its cost — that
    `calibration/color`'s photograph becomes eligible — is measured, not assumed
    (plan Task 5, measurement 2).
19. **The mask marks content that is continuous-tone, not regions of the layout.**
    Structure stays unmarked *wherever it sits*. One mask, two consumers, same
    answer: don't gamut-map structure, do pin it. This is why the tone screen's
    backing rect must move out of its marked group (Task 4).
20. **Task 5's `#[ignore]` diagnostics stay non-asserting.** Plan governs over the
    review rubric. Their printed output is the spike's deliverable; asserting a
    threshold now would defend a constant with a test derived from the same
    unvalidated plan.
21. **CHANGES.md is not touched by this plan**, despite the project rule and despite
    Task 4 altering a shipping screen's output. The branch is HELD and unreleased and
    the gamut feature has no entry either; one entry gets written at merge prep.

### Task 1 — DONE (`c74312f`, `f82eedc`)

`DitherOptions::pin_carry` (default 0.9, clamped `[0,1]`, `#[inline]` builder) plus
the pinning branch in `dither_with_kernel_noise`, which gained a 7th parameter
`pinned: Option<&[Option<u8>]>`. The kernel/jitter/serpentine loop is untouched — a
pinned pixel is an ordinary pixel whose output is forced and whose emitted error is
substituted.

**`pinned: None` means pinning is OFF, never "eligible everywhere."** Guarded by
`no_pin_map_reproduces_the_unpinned_output_exactly`. Keep it that way — the inverse
silently changes every existing caller's output.

### ⚠️⚠️ Task 1's real lesson: four of my own tests were unfounded

**Four tests I wrote into the plan rested on premises that were false when measured.
Every one was caught only because the implementer refused to tune values until
green.** This is the most important thing in this file.

| Test as planned | What measuring it showed |
|---|---|
| "hostile incoming error" moves a black pixel | Unpinned output was **already all-black** — the scenario was never hostile, and the test **passed against a never-pin mutant** |
| a probe near a decision boundary detects the carry | The carry difference reaching it was **0.0015** against a probe **0.44** from the nearest ink; carry 0.0 and 1.0 gave identical output |
| absorbing the error leaves the field beyond darker | Measured **523 vs 530** — wrong direction, and within noise. Accumulated error has ~zero mean in steady state, so destroying it shifts nothing |
| two runs agreeing proves the map is honoured | A pin-everything mutant **corrupts both sides identically**, so the equality held trivially |

**Two rules came out of this. Apply them to Tasks 2–5 and to anything later.**

- **A test claiming "X rescues this case" must assert, in the same test, that the
  case needed rescuing.** Asserting the outcome while assuming the setup is exactly
  how a guard passes against a mutant that disables the feature.
- **A comparison test must assert its comparison is non-degenerate.** Two runs that
  agree prove nothing if a mutant collapses both.

The replacements are exact rather than statistical, which is why they hold:
`a_fully_absorbing_pin_isolates_what_lies_beyond_it` uses a pinned bar **wider than
the kernel's horizontal reach** (Atkinson max `dx` = 2, bar = 4, full height,
serpentine off), so at λ=0 no error can cross and the field beyond is **bit-identical**
regardless of the field before it; at λ=1 it must differ. No threshold, no direction.
The reviewer independently re-derived that from the kernel entries. **Do not weaken
this test**, and if you refactor its geometry, re-confirm the never-pin mutant still
fails its bar-holds check — the `BAR` const exists so the guard cannot silently stop
guarding.

### Tasks 2–5 — NOT STARTED

Full text in the plan. In brief:

- **Task 2** — `EinkDitherer::dither_with_pinning(pixels, w, h, pin_eligible: Option<&[bool]>)`.
  **The match is resolved on the caller's `Srgb` bytes BEFORE preprocessing**, by
  exact `[u8;3]` equality against `Palette::official(i)` — matching the preprocessed
  `LinearRgb` would silently never fire once saturation or contrast leaves identity.
  Pinning is refused across a resize. Also corrects the wrong comment at
  `preprocess/preprocessor.rs:88`, whose "zero quantisation error" claim ignores
  error diffused *in*.
- **Task 3** — byonk builds `pin_eligible` as the **inverse** of the tone mask in
  `svg_to_png.rs`. The mask is currently rasterized only when `has_tone_markup()`
  **and** `gamut.amount != 0.0`; that inner gate must move, since pinning wants the
  mask for a different reason.
- **Task 4** — move the tone screen's black backing rect out of its marked group.
  Pure black is in gamut so the mapped patches cannot move, but those pixels leave
  the adaptation group and **`R` is a 99th percentile over that set — measure it**.
- **Task 5** — the measurement pass: λ sweep (0.0/0.5/0.8/0.9/0.95/1.0), far-edge
  dump, the photograph's exact-match share, text on a real screen, cost.

### Carried into Task 3 as a deliberate decision, not an omission

`error_clamp` does **not** bound the pinned path's carry. A normal pixel's
contribution is bounded via `apply_error`; a pinned pixel forwards raw
`accumulated * pin_carry`. **Inert at the live default** — `error_clamp` is uniformly
`1.0` for every algorithm (`dither/mod.rs:118`). Note that
`api/builder.rs:56`'s "Atkinson with `error_clamp=0.08`" is a **stale comment**, not
the live default; it misled a reviewer this session.

Other deferred minors from Task 1, all in the ledger: no length/range `debug_assert!`
on the pin map (belongs in Task 2, which builds it); `f32::clamp` does not trap NaN.

# The screen collection is marked (`fe66ee6`) — done, out of plan

**This landed outside the pinning plan, at the owner's request, after Task 1.** It
resolves the long-standing "should a real content screen be marked?" decision by
marking the whole collection at once, applying ruling 19.

The owner's framing, which held: *most screens have no continuous tone; only ones
with gradients or photos need this.* Surveyed all 13 shipped screens — **10 needed
nothing.**

| Screen | Marked | Why |
|---|---|---|
| `builtin/default` | background photograph | ~12% of its pixels out of gamut |
| `builtin/calibration/color` | each gradient bar, hue sweep, photo | owner chose full marking over keeping it as a raw reference |
| `examples/gphoto` | the full-screen photo | real user photography |
| `builtin/calibration/tone` | unchanged | already marked, deliberately |
| `builtin/calibration/grey` | **nothing** | both gradients run white→black |
| `builtin/default`'s vertical bar | **nothing** | same — white→black |
| `examples/mandelbrot` | **nothing** | rects already palette-exact from `layout.colors` |
| 7 others + `byonk-base` | **nothing** | no continuous tone at all |

**The argument for leaving an achromatic gradient unmarked is the one to remember.**
Grey is always in gamut, so mapping it is a *no-op* — while marking it switches
exact-match pinning *off* across a deliberate dithering test pattern. Marking costs
something and buys nothing. Any future "should this be marked?" question should start
by asking whether the content can even be out of gamut.

**Marking goes on the element that IS continuous-tone, never on a group around a
band.** On `calibration/color` the gradient bar and its label are emitted from the
same loop body, so a `<g>` wrapper would have swallowed the label; a per-element
attribute needs no restructuring and cannot catch structure by accident.
`tone_mask.rs`'s `self_closing_marked_element_does_not_leak_scope` proves a leaf
element can carry the attribute without marking its siblings.

### ⚠️ The mask rasterizes in document order, and that does real work

Measured at 800×480: `calibration/color` marks **262,672 / 384,000** px;
`default` marks **309,125 / 384,000 (80.5%)** — even though its photo is
*full-screen*. The shortfall is not a bug. Unmarked elements drawn **after** a marked
one paint black back over it in the mask document, so `default`'s hero text, palette
swatches and white info bar punch themselves out of the photo's marked area. Those
pixels are opaquely covered, so excluding them is correct.

**Consequence for authors:** text over a photo does not need special handling as long
as it comes later in document order. A group wrapper around the region would still
capture it — document order saves the element-level marking, not the group approach.

### What this changes for a user

`default`, `calibration/color` and `gphoto` now render differently. This is the first
time the gamut work alters output anyone actually looks at. **`calibration/color` no
longer has an unmapped photo reference** — the raw-behaviour comparison for a
photograph now exists only on `calibration/tone`, whose left column is unmarked by
design.

**Not yet looked at on a panel.** That is the outstanding action.

### Untracked duplicate — do not sweep it in

`/Users/oetiker/checkouts/byonk/examples/` is an **untracked** byte-identical copy of
`screens/examples/`. It was left alone and will now drift by one file
(`gphoto/screen.svg`). This is exactly the kind of local file that makes
`git add -A` dangerous here.

## Open owner decisions

1. **Look at the three newly-marked screens on the panel.** The marking is committed
   but unjudged.
2. **The branch.** Still HELD. Ten sessions of work sitting unmerged.

# The prior initiative: gamut mapping (complete)

Rulings 16 and 17 are implemented, measured and committed. The mapper compresses
along a ray from mid-grey; knee default 0.99. All four measured panel inks come back
at `t_max = 1.000` — yellow, which the fixed-`L` mapper stranded at 42%, is now
indistinguishable from red, blue and green.

**Ruling 16 and ruling 17 are only safe together.** The ray's liability is near-white
tints: a high-`L`, low-chroma colour's ray exits the hull at the *white point*, so it
reads as boundary-saturated even though its chroma was never out of gamut. At knee
0.8 this darkens `grey 250 tint 4` by **−0.084**; at 0.99 by −0.0035. **Do not lower
the knee without re-measuring** — `gamut::mapper::tests::ray_geometry_diagnostic`
prints the table, `a_tinted_near_white_keeps_its_lightness_at_the_shipping_knee`
guards it. Whole-image mean `|dL|` hides this completely (0.009–0.012, all anchors).

**The port is proven equivalent to the prototype, pointwise** —
`cusp_anchored_vs_fixed_lightness`, swept over 5832 colours across the sRGB cube,
worst channel diff 0. **Keep this check working**; it is what makes every other number
in that file trustworthy.

Per-pixel cost was the loudest stated risk and the cheapest to retire: **218 ms for a
worst-case 800×480 frame**. **No `t_max` lookup table was built, deliberately** — a
sibling to `CmaxTable` would have inherited its bilinear *overshoot at the pinch*
(yellow: exact 0.073 vs sampled 0.093), and an overshot `t_max` maps pixels outside
the hull in exactly the region the change exists to fix.

**As of `fe66ee6` the feature reaches four shipping screens** — `calibration/tone`,
`calibration/color`, `default` and `examples/gphoto`. Before that commit it reached
exactly one, and that sentence is still what most of this file was written against.

# ⚠️⚠️ Read this before trusting any dithering picture

**Every visual dither comparison in this tree reads about 30% too dark, and it is the
viewer's fault, not the ditherer's.**

| | mean LINEAR luminance | mean GAMMA-SPACE byte |
|---|---|---|
| portrait | **+10.2%** vs source | −32.4% vs source |
| background | **+4.4%** vs source | −29.3% vs source |

Error diffusion preserves brightness in linear light. A viewer downscaling a PNG
without linearising averages sRGB bytes directly, under-weighting bright pixels in a
black/ink speckle. On the panel the eye averages optically, in linear light.

- **Relative** comparisons between variants remain valid. Use the `-mapped`
  (undithered) renders for absolute judgements.
- **Open defect 1 below was diagnosed this way and should be re-measured in linear
  light before anyone chases it.**

# ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading** — a flat patch is a single colour; every
artifact that matters is at a boundary *between* colours.

**Whole-image means are equally misleading.** On the portrait all four anchors scored
0.0545–0.0550 mean chroma and looked identical, because only 7% of pixels are out of
gamut and the untouched 93% swamped them. Restricted to the pixels the mapper acts
on, the spread was 68% to 90%. **Measure the pixels the change touches.**

**Look to find what to measure; measure to decide.** When comparing an old behaviour
to a new one, **render both from the same input in the same image**.

**IDE diagnostics lie in this tree.** Verify with an actual `cargo` run. Never take a
subagent's "all green" at face value.

# ⚠️⚠️ Read this before dispatching any subagent

**`make check` takes ~10 minutes in this tree.** The subagent stream watchdog fires at
600 s of silence, so **an implementer that runs `make check` in the foreground dies
mid-run.**

- **Implementers get:** `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets`
  then `CARGO_BUILD_JOBS=2 cargo test -p <crate> --lib`. Say so in the brief.
- **The controller runs the full gate** in a **backgrounded** Bash call
  (`run_in_background: true`) and polls.

**Subagent task briefs scoped to `--lib` cannot see integration tests.** That
restriction exists for the watchdog, and it is why both builtin-inventory guards
survived three clean task reviews in session 9. The controller's full gate is not a
formality.

**Tell the implementer the pre-existing failures.** `cargo test -p eink-dither --lib
-- --ignored` reports **3 pre-existing failures unrelated to any current work**:
`preprocess::preprocessor::tests::{test_process_with_resize,
test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`, which
panic at `resize_lanczos` **by design**. An implementer that does not know this wastes
a round.

When an implementer stalls, **do not resume it blindly** — assess the abandoned
working tree first.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **~10 min — background it.**
- **`make check` runs `cargo fmt`, not `cargo fmt --check`.** It rewrites files in
  place and leaves the tree dirty. Code transcribed from a plan is usually not
  rustfmt-clean; put `cargo fmt` in the implementer's command list.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. **Never `git add -A`.**
- **`make check` does not run the `#[ignore]` tests**, and most gamut evidence is
  ignored. Run explicitly:
  - `cargo test -p eink-dither --lib gamut::mapper::tests::ray_geometry_diagnostic -- --ignored --nocapture`
  - `cargo test --release -p eink-dither --lib map_frame_cost -- --ignored --nocapture`
  - `cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture`
  - `cargo test -p eink-dither --test visual_compare visual_gamut -- --ignored --nocapture`
- Output PNGs land in `target/dither-compare/`.
- **Production applies no preprocessing before dithering** — `src/rendering/svg_to_png.rs`
  goes `rgba → Srgb → (gamut map) → dither`.
- **Rendering a builtin screen needs a device, and the old plan's CLI is wrong.** It is
  `render --mac <MAC> --output <PATH>`, resolved through config. Do **not** edit the
  tracked `config.yaml` — copy it, point `CONFIG_FILE` at the copy, and add a
  throwaway device:
  ```yaml
    "AA:BB:CC:DD:EE:01":
      panel: reterminal_e1002          # WITHOUT this you get a GREYSCALE render
      screen: byonk-builtin/calibration/tone
  ```
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
  **The missing-panel trap is silent** — it renders happily, in greyscale.
- **Before adding a builtin screen, grep for what enumerates the inventory.** Two
  tests hardcode the shipped count as an exact `assert_eq!` on purpose:
  `tests/builtin_package.rs:44` and `tests/screen_schemas_test.rs:128`. Update them,
  never loosen them.
- **`Dockerfile` is broken independently** — it never copies `crates/`, so the
  workspace cannot resolve `eink-dither`. Releases unaffected (`Dockerfile.release`,
  CI-built binaries). Out of scope, untouched.
- `make docs` needs `mdbook-mermaid`.

## Useful test assets

`screens/builtin/calibration/color/photo.png` (portrait, 7% out of gamut) and
`screens/builtin/default/background.jpg` (station concourse, 12%) are byonk's own
shipping assets and are what the panel actually renders. Synthetic fields at full
saturation are unrepresentative (`ρ` p50 = 2.87 against a photo's 1.2–1.5).

## Public surface

`eink_dither::{Oklch, GamutMapper, GamutOptions}`; `gamut::hull::{Hull, HullShape}`;
`gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma` (takes `r`); `DitherOptions::pin_carry`.
Byonk: `models::GamutTuningValues` (`or`/`is_empty`/`resolve`), `DitherTuningValues::gamut`,
`DeviceConfig::gamut`, `RenderOpts::gamut`, `RenderParams::gamut`, `CachedContent::gamut`,
`content_pipeline::ScriptResult::script_gamut`, `DeviceContext::dither_gamut_{knee,amount,max_compression}`,
`lua_runtime::ScriptResult::gamut`, `svg_to_png::DitherTuning::gamut`.
Shared test fixtures: `gamut::test_support::{six_colour, four_grey, panel_measured}`
— **import, never copy**. `six_colour`'s idealised primaries do not reproduce the
hull's pinch, so they cannot guard it.

## The lesson, now proven ten sessions running

**The plan's code and constants are not evidence.** Measure before believing the plan,
your own diagnosis, a reviewer's "harmless", the spec — or your own eyes on a
downscaled PNG. Session 10 extends this to **the tests the plan specifies**: four in a
row were unfounded, and the only thing that caught them was an implementer that
reported a failure instead of adjusting the test.

Session 9's, still true:

- **Adding a builtin screen has a fan-out nobody mapped** — see the inventory guards
  above. Each cost a ten-minute cycle, discovered one at a time.
- **A reviewer that fact-checks docs against code earns its cost.** Two factual errors
  in one short docs section, the second introduced *by the fix for the first*.
- **Measure the claim you are actually making.** "The two columns differ only by the
  mapping" is false at pixel level (27.4% differ with mapping off) and true
  perceptually.
- **A ruling can carry a latent defect that only a second ruling masks.** Ruling 16
  measured alone looked broken; measured at the knee the owner had already accepted,
  it was fine. **Measure a change at the settings it will actually ship with.**
- **A rewritten test is an unverified test.** Mutate every guard you touch, in both
  directions.
- **Bound synthetic sweeps to inputs that can occur.** Three failures in one session
  were samples outside sRGB sitting on the asymptote, tying in `f32`.
- **The risk the handover flags loudest may be the cheapest to retire.**

Session 8's, still true:

- **The confident recommendation was wrong.** Cusp anchoring was the principled, cited
  fix and measured 40% against mid-grey's 82%. **Prototype before recommending; a
  citation is not a measurement.**
- **A surprising number is a lead, not noise.**
- **The owner's question was better than the controller's plan.** "Why do panel colours
  not dither to themselves?" produced ruling 17 — and, a session later, this whole
  initiative.
- **Fixing the code is not fixing the cause.** The spec said two incompatible things
  for six sessions.

Session 7's, still true:

- **Every task passed review. The feature was still wrong.** What saved it was the
  owner looking at a picture. **Ask what the tests do not assert.**
- **Say "I verified" only after verifying.** Reading is not measuring.
- **Pre-flight the brief, every time — it has never once been clean.**

Session 6's: a green suite proves nothing about a site the compiler cannot reach
(`CachedContent::with_tuning` copies fields by hand; deleting one left all 448 tests
passing).

## Standing rulings

> **Provenance matters.** **1-9** are genuine owner rulings. **10-12** were made in
> session 6 by task reviewers and the controller **while the owner was absent** — do
> not present them as settled. **13-21** are genuine owner rulings from sessions 7-10.

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`).
2. **Task 4 — ACES 1.3 RGC `powerP` curve, `t/(1+t^p)^(1/p)`, `SHOULDER_POWER = 1.2`** (`b986caf`, `0d7053d`).
3. **Task 5 — `select_nth_unstable_by` + `total_cmp`** (`0d7053d`).
4. ~~**Knee default 0.6 → 0.8**~~ (`3fd9ab8`). **Superseded by ruling 17.**
5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`). `linear_to_srgb`
   has an epsilon-free `debug_assert!` — unclamped panics under `cargo test`, behaves
   identically in release. **Global Constraint.**
6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `IN_LIMIT_MAX_RATIO = 0.05`,
   `BEYOND_LIMIT_MIN_RATIO = 0.3`. **Verified still valid under ruling 16.**
7. **Task 8 — strip CSS paint case-insensitively, write the inline style too** (`ba8859c`).
8. **Task 9b — the mask must not invent a stroke** (`297b10a`).
9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Removed by Task 10.
10. **Task 11 — silent `.ok()` coercion stays, for consistency.** If ever fixed it must
    be **one pass across the whole family**, never gamut alone.
11. **Task 11 — range validation belongs in the mapper, not the config layer.**
12. **Task 12 — `dev.rs`'s `query_tuning` keeps `Default::default()` deliberately.**
13. **Amendment B — the CLI is gamut-aware** (owner, session 7).
14. **`amount` is clamped to `[0,1]` in the mapper** (owner, session 7, `5d14fd3`).
15. **The adaptation is redesigned so `R` scales only the tail** (owner, session 8, `23a1e39`).
16. **The compression direction is mid-grey anchored** (owner, session 8; implemented
    session 9, `8e30e24`). Chosen over cusp-anchored (40% vs 82% on yellow).
17. **The knee default is 0.99** (owner, session 9; implemented `868544c`). Supersedes ruling 4.
18. **Pinning is eligible everywhere outside a `continuous` region, in every document**
    (owner, session 10).
19. **The mask marks content that is continuous-tone, not regions of the layout**
    (owner, session 10). **Applied across the whole shipped collection in `fe66ee6`.**
20. **Task 5's `#[ignore]` diagnostics stay non-asserting** (owner, session 10).
21. **CHANGES.md is not touched by the pinning plan** (owner, session 10).

**Constants inherited from the plan and never challenged:**
`PERCENTILE = 0.99`, `MIN_DISCARD = 32`, `HUE_BINS = 128`, `LIGHTNESS_BINS = 64`,
`C_SEARCH_HI = 0.5`, `T_HI = 6.0`, `T_STEPS = 24`, `ACHROMATIC_C = 1e-6`.
`max_compression = 2.5` can no longer touch sub-knee chroma. **`pin_carry = 0.9` is
provisional and awaits Task 5's sweep.**

Standing: **the branch is HELD** — no PR, no merge to `main`.

## Deferred minors

Session 10 (pinning), all in the active ledger:

- No length/range `debug_assert!` on the pin map — belongs in Task 2, which builds it.
- `error_clamp` does not bound the pinned path's carry (inert at the live default of
  1.0; see the Task 3 note above).
- `api/builder.rs:56` documents a stale `error_clamp` default of 0.08.
- `f32::clamp` does not trap NaN in `pin_carry`; the field is `pub` anyway.

Session 9:

- `six_colour`'s blue vertex cannot reach the knee's design point because the
  constant-hue OKLch ray **bulges outside the linear-RGB hull** — the hull is convex
  in linear RGB, and a constant-hue line is straight in Oklab, not there.
  `t_max = 0.861`. Harmless: `panel_measured` hits 1.000 on every ink.
- `gamut_cusp_prototype.rs` and `gamut_adaptation_diag.rs` still hardcode the panel
  palette, duplicating `test_support::panel_measured`. Integration tests cannot see
  the crate's `#[cfg(test)]` fixtures; exporting them is the fix if a fourth copy
  appears.
- `mapped_chroma` is now `#[cfg(test)]`.

Session 8:

- The `Cmax` table's bilinear sample *overshoots* where the hull pinches (yellow:
  exact 0.073 vs sampled 0.093). **Now load-bearing knowledge** — it is why no `t_max`
  table was built. Do not "fix" it without re-reading that decision.

Session 7:

- `test_gamut_mapping_preserves_hue_order` would also pass against an **identity**
  mapper. Weak guard, kept deliberately.

Session 6:

- **Task 10:** the unreachable mask-length-mismatch branch returns `RenderError::Dither`,
  a misnomer; no better variant exists.
- **Task 11:** `assert_eq!(GamutTuningValues::default().resolve(), defaults)` **cannot**
  detect a restated-constant violation — manual-review-only.
- **Task 11:** `PanelDitherConfig` accepts a `gamut:` key in panel YAML; verify it is live.
- **Task 12 (inherited):** `resolve_effective_tuning` replaces the **whole** struct when
  any override field is set, so an active dev-UI query override resets the previewed
  gamut to default and diverges from production.

Earlier sessions:

- **Task 7:** the winning dilute start was `eps = 0.005`. Optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`; traced
  — failure mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()` while
  `rewrite_start` propagates them. Style wart.
- **Task 8:** element names matched as raw bytes, so `<svg:image>` would be mis-handled
  and `<symbol>` gets no `<defs>`-style stripping. Dormant.
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`.
- **Task 9b:** `resolve_stroke` cannot see stylesheet-only strokes. Deliberate.
- Two pre-existing rustdoc warnings in `eink-dither`; `gamut/hull.rs`'s three epsilons
  want a comment; `adapt.rs`'s `max_compression < 1.0` collapse is untested; no test
  exercises literal `NaN`.

## Open dithering defects — independent of this work

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an optimal 47%.
   **Re-measure in linear light first** — see the brightness section above.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise scale.
   **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red. The benign half is
   established and asserted: a flat fill of a *measured ink* dithers to that single ink
   exactly, which is correct — **but only in isolation.** Set next to saturated content,
   27% of those same pure-ink pixels are taken over by diffused error. **This is the
   defect the active initiative addresses.**

The selector work that tried to fix these is **three-for-three refuted**;
`crates/eink-dither/tests/spike_simplex.rs` is the deliberate record of what does not
work. `AtkinsonHybrid` remains an unlanded candidate that beats Atkinson on both axes —
changing the default alters rendering for every device, so it is the owner's call.
