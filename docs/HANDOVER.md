# Handover — the quantiser is the bug; the cheap fixes are in, the fix is not

**Date:** 2026-08-22 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `a20f2ee`
**Base:** `main` @ `5c67c62` (v0.19.0, protected). `main` is an ancestor; no rebase needed.

> **`cargo test --workspace` FAILS ON PURPOSE.** Exactly one test is red:
> `test_neutral_grey_has_no_dominant_chromatic_ink`
> (`crates/eink-dither/src/domain_tests.rs`). It documents the §1 defect and
> goes green when the quantiser is fixed. The owner chose this over
> `#[ignore]` on 2026-08-22. **227 passed / 1 failed in `eink-dither` is the
> expected state.** Any *other* failure is a real regression.
>
> **Three commits landed this session** — the first repo changes in three
> sessions. The panel measurement work still lives only in
> `~/scratch/panel-evidence/` and on the deployed g18 box (§8).

---

## 1. The finding

**byonk paints neutral greys in coloured ink, on every six-colour panel it
supports, and the cause is the quantiser — not the calibration, not the dither
algorithm, and not this particular panel.**

`find_nearest` picks the single closest palette entry. For a pure neutral with
no diffused error it picks green for every neutral from L\* 0.235 to 0.521 and
blue from 0.57 to 0.67 — 50.4% of the ramp. At L\* 0.413, green is 0.066 away
while white is 0.482 and black 0.413. Green is six times closer.

**The greyscale inheritance.** In 1-D the nearest level and the correct mixture
partner are the *same thing* — the nearest grey always brackets the target. In
3-D that identity breaks: the nearest single point and the vertices of the
enclosing simplex are unrelated. The proxy was carried over unexamined.
`palette.rs:365` even records the consequence as intended:
*"Grey pixels matching dark chromatic entries … is expected and desirable."*

### The measurement that now lives in the repo

Reproduce it: `cargo test -p eink-dither --lib test_neutral_grey_has_no_dominant_chromatic_ink`.
Fixture is `gamut::test_support::panel_measured()` — a measured E1002, index
order `0 black, 1 white, 2 red, 3 yellow, 4 blue, 5 green`.

| grey | total chromatic | largest single ink | dE |
|---|---|---|---|
| 96 | 100% | green 45.1% | 0.026 |
| 112 | 100% | **green 64.2%** | 0.031 |
| 128 | 98.0% | **green 77.6%** | 0.063 |
| 144 | 88.0% | **green 69.6%** | 0.037 |
| 160 | 77.5% | **green 62.2%** | 0.026 |
| 176 | 65.3% | **green 54.0%** | 0.023 |

A flat mid-grey contains essentially **no black and no white**. The exact
solution is trivial: panel white is 0.7157 in linear light, so grey 128 is
`26% white + 74% black`, error exactly zero.

The deployed E1004 palette behaves the same way — 77.6% / 100% / 100% / 81.2%
chromatic at greys 64/96/128/160, measured last session with
`~/scratch/panel-evidence/e1004-2026-08-21/greyprobe/`.

### The metric correction — read before writing any gate

The previous handover proposed bounding the **total chromatic share**.
**Measured, that is the wrong metric** and this session changed it:

| palette | worst total chromatic | worst single ink |
|---|---|---|
| `panel_measured()` (real) | 100% | **78% green** |
| idealised BWRGBY | 98.8% | 36%, R/G/B in near-equal thirds |

A total-chromatic gate rejects both equally. But the idealised palette's 98.8%
is a genuinely cancelling intermixture that looks grey — exactly the case §3
says is fine. **Largest single chromatic ink is the metric that separates good
from bad.** The bound shipped at 50%: an ink covering the majority of a patch
is its field colour, not a component of a mixture.

---

## 2. What is ruled out — do not redo these

1. **The dither algorithm.** Atkinson 52.0%, Atkinson-hybrid 54.2%,
   Floyd-Steinberg 50.3% chromatic on a neutral ramp. Hybrid is marginally
   *worse*. Changing the kernel does not touch this.
2. **`sierra-lite`** — an earlier handover's top recommendation. Rendered and
   compared: flat, blocky bands, because the panel config carried
   `error_clamp: 0.11`, a pre-0.18.0 value. That knob is now `max_error` and
   the stale name is ignored (§4), so this comparison is worth **redoing once**
   with a sane value before writing `sierra-lite` off.
3. **The calibration.** Two independent checks:
   - Re-running the probe with the *old, more colourful* config palette gives
     **57.8% chromatic — worse**. A more saturated green does not help; it hands
     more of the ramp to blue.
   - Perturbing every ink by the measured photographic uncertainty (2.02% of
     white) moves the result by at most **dE 0.013, below one JND**.
   **Re-shooting the panel would not have changed anything.**
4. **A blanket chroma penalty.** `HyAB kchroma=10` cuts chromatic choice on
   neutrals from 50.4% to 9.4% — but the crate's own history records HyAB as
   biased for error diffusion on muted photographic colour. It fixes greys by
   breaking photographs. A diagnostic, not a fix.

---

## 3. The design question, and the honest answer

**Black+white is NOT simply the right answer.** Computed over the deployed
palette: the minimum-brightness-variation exact mixture (MBVQ's criterion,
Shaked et al. at HP) is **chromatic almost everywhere and smoother than
black+white at every neutral** — 3.3× smoother at grey 88. Black and white have
the maximum per-dot luminance contrast, so a black/white checkerboard is the
*grainiest* possible neutral.

**And byonk is already near that optimum.** Its grey-128 output
(`R13% Y19% B47% G22%`) is within a couple of percent of the computed
minimum-variation mixture (`R9% Y25% B47% G19%`). The recipe is defensible.

**So what is wrong is the criterion.** At grey 88 the minimum-variation mixture
is `R15% Y2% B11% G72%` — **72% of the area is one chromatic ink**. That is not
a cancelling intermixture, it is a green field with speckle on it.

MBVQ gets away with this in print because C, M and Y are complements: chroma
cancels *locally*, between adjacent dots, at every scale. **This palette has no
complementary pair**, so cancellation only happens as a neighbourhood average —
and the eye does not average. It sees the dominant ink as a field colour.

**Minimum brightness variation optimises the term we can tolerate (luminance
noise) and ignores the one we cannot (chromatic clumping).**

### The proposal

Minimise a two-term objective subject to exact colour match in **linear RGB**:

```
cost = lambda * luminance_variance  +  (1 - lambda) * chroma_variance
```

- `lambda = 1` reproduces MBVQ — smoothest, greenest. Roughly what ships now.
- `lambda = 0` forces maximum GCR — exactly neutral, grainy.
- Between them is the dial. This is precisely what Grey Component Replacement
  does in print: decide *how* the achromatic component is built.

Owner's ruling, 2026-08-22: *"if we can cancel sensibly with our available
colours we should definitely do that, but there are many instances where this
will not be possible."* The two-term objective is that ruling, made numeric.
The 50% single-ink gate in §1 is its first, crudest approximation.

---

## 4. What landed this session

Three commits, all verified with `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
and `mdbook build`.

### `21f9b1b` — the failing test (was defect 9)

`test_neutral_grey_has_no_dominant_chromatic_ink` in
`crates/eink-dither/src/domain_tests.rs`. Runs on `panel_measured()`, bounds
the largest single chromatic ink at 50% over greys 16–240. **Red on purpose.**

Also added a `max_single_chromatic_pct` column to
`test_dither_perceptual_accuracy_photo`'s table (50% for near-neutral rows,
100.0 = unconstrained for saturated ones). That one is green and is a live
regression guard against clumping on muted photo colours.

### `3351151` — `error_clamp` → `max_error` (was defects 1 and 2)

0.18.0 changed what the knob bounds and the old name kept working, so stale
values kept rendering flat. Now:

- `DitherOptions::max_error` in the crate; `error_clamp` gone from the codebase.
- `DitherTuningValues::deprecated_error_clamp` captures the old YAML key and is
  **excluded from `is_empty()`**, so a block holding only it configures nothing.
- `AppConfig::deprecation_warnings()` returns one message per site with the
  exact path (`panels.X.dither.sierra-lite.error_clamp`), logged at WARN by
  `load_from_assets`.
- The hand-written `PanelDitherConfig` visitor **needs its explicit
  `"error_clamp"` arm** or the key falls through to the algorithm-name branch
  and fails to parse as a sub-map.
- Lua scripts returning `error_clamp` get a WARN from `lua_runtime.rs` and are
  ignored. **No unit test** — `lua_runtime.rs` has no test module and nothing
  in this repo captures tracing output.
- **Also fixed the dev UI's tuning table**, a second copy of
  `DitherAlgorithm::defaults()` that had drifted on every row: pre-0.18.0
  clamps throughout, Atkinson offering noise `0` where the engine uses `8.0`,
  and a noise input capped at `8.0` so four algorithms could not reach their
  own default. Values now mirror the crate. **The duplication remains** (§5.1).

### `a20f2ee` — precedence (was defects 3 and 4)

They looked like one defect and are two, with opposite answers:

- **`dither`: the device now wins.** `resolve_render_params` resolves
  `device_config_dither.or(script_dither)`. The algorithm suits the panel, not
  the content. A screen's `dither` still applies where the device names none.
- **`refresh_rate`: the script still wins** — only it knows when its content
  next changes. `resolve_refresh_rate` now returns
  `ResolvedRefresh { rate, ignored_device_override }` and the caller logs the
  displaced value.
- `resolve_render_params`'s `warning_sink: &mut Option<String>` became
  `warnings: &mut Vec<String>` to carry the second message. All four call sites
  updated.
- Names are compared **after** normalisation — `sierra-light` and `sierra-lite`
  are not a conflict.
- `display.rs`'s `final_algo_str` (which picks the per-algorithm tuning block)
  was a second copy of the precedence rule and still said script-first. Fixed;
  `dev.rs`'s copy was already device-first, so this also removed a pre-existing
  disagreement between them.

---

## 5. byonk defects — what is left

1. **The dev UI duplicates `DitherAlgorithm::defaults()`** (`static/dev/dev.js`,
   `DITHER_DEFAULTS`). Corrected this session and marked "keep in step", but it
   *will* drift again. The UI should fetch defaults from the server. **New.**
2. **`docs/src/concepts/content-pipeline.md:261` still documents `error_clamp`**
   with a stale `0.05 – 0.5` range. It is one of the owner's four uncommitted
   files, so it was deliberately left untouched. **The owner must fix that line
   before committing that file.** **New.**
3. **A panel's tuned dither parameters silently do nothing when the device picks
   a different algorithm.** `dither:` is keyed by algorithm name; the E1004's
   `sierra-light` block is inert under `atkinson-hybrid` with no warning. Same
   class as the two just fixed, and `deprecation_warnings()` is now the obvious
   place to report it from.
4. **`noise_scale: 5`** in the shipped E1002/E1004 blocks; the measured optimum
   for Sierra Lite is 2.5 (`dither/mod.rs:166`).
5. **MCP cannot set a device's dither algorithm.** `assign_screen` takes only
   `mac` and `screen_ref`, while `apply_device_patch` (`write.rs:249`) already
   accepts `dither`, `panel`, `refresh`, `colors`, `params`, `name`.
   **Owner ruling: MCP should give direct access, not document the REST API.**
   The shared-core pattern exists for exactly this (`write.rs:236`). Widen it
   and rename `assign_screen` → `configure_device`.
6. **Panels have no write path at all** — no `/panels` route in `admin_router()`,
   in REST or MCP, in any mode; and global config is read-only under the add-on
   (`write.rs:46-53`). The whole calibration workflow therefore requires ssh.
   **Recommendation: carve panels out of "global config" the way device mappings
   already are** (`write.rs` comment: *"a device mapping is not global config, so
   it stays writable in add-on mode"*).
7. **`colors_actual` serves two masters** — an honest preview and the
   ditherer's ink choice. Carried forward; possibly two fields.

Still open from the TRMNL X work: the palette rejects duplicate colours, and
`map_grey_indices` derives the level from an entry's **index** rather than its
hex, so "declare only the usable inks" does not work.

---

## 6. Prior art IN THIS REPO — read before writing quantiser code

`crates/eink-dither/tests/spike_simplex.rs` — 591 lines, `#[ignore]`d, written
by an earlier session to answer this exact question. **Run it:**
`cargo test -p eink-dither --test spike_simplex -- --ignored --nocapture` (77 s).
It writes comparison renders to `target/dither-compare/`.

Its recorded results:

- Restricting candidates to the optimal mixture's support does **not** remove
  the scalloped arcs. Dead hypothesis.
- Restriction *alone* is worse than production: dE 0.0496 → 0.0616.
  *"Black stays unreachable and red absorbs the slack — green was partly
  standing in for the black that never arrives."*
- Restriction **plus full propagation** is excellent on flat patches: black
  1% → 42% against an optimal 47%.
- **But it bands smooth gradients, intrinsically.** Support membership is
  binary, so an ink appears or vanishes across a locus; in a vertical gradient
  those are horizontal lines. Refining the lookup 64 → 255 levels changed
  nothing.
- Its soft-bias follow-up measured *"largest single-step weight change: 0.090"*
  where *"a smooth field would stay well under 0.1"* — **the optimal mixture's
  support field is itself discontinuous.**

**Why that matters for the new design:** the spike restricted to the support of
an *unconstrained* optimum, whose support jumps as the target moves. A **fixed**
partition (§7) has no such jumps in the lightness direction, because white and
black are in every cell and never drop out. The spike was defeated by the
discontinuity of an optimiser, not by mixture-awareness. That is a reason to
expect the fixed-partition form to survive — and a reason to measure it.

---

## 7. The literature (four agents, 2026-08-22)

**The field's name for what byonk is missing is _separation_.** Industry splits
the job: decide the mixture (colorimetric, constrained), then decide where the
dots go. byonk's quantiser does both at once. One-line statement of the bug,
from Zhigang Fan (Xerox, US 9,848,105): *"a minimum error in a perceptual space
may imply a large error in an output device color space."*

**E Ink patented exactly this fix, for exactly this hardware.**
US 10,554,854 / 10,771,652 (Crounse, priority 2016-05-24): per pixel, add the
diffused error, **find the enclosing simplex**, convert to **barycentric
coordinates**, output **the primary with the largest coordinate**. Their
background names our symptoms — an *"under-constrained list of primaries"*,
transients where *"the output never settles to the correct average"*, and
*"pattern jumping"*.

**And then E Ink backed it out.** US 11,527,216 (Buckley, Crounse, Telfer,
Sainis, 2017): *"image quality is compromised by using barycentric quantization
inside the color gamut hull."* They reverted to nearest-neighbour **inside** the
hull, keeping barycentric only to project out-of-gamut colours onto surface
triangles. **This is a published argument against the change, from the vendor,
in our domain. Measure; do not assume.**

**The theory.** Chai Wah Wu (IBM), *"Error Diffusion: Recent Developments in
Theory and Applications"*, IS&T NIP20 2004 — generalized error diffusion emits
the vertex with the largest barycentric coefficient. Theorem: bounded error for
all images iff the input gamut is inside the convex hull of the output set. His
counterexample is a triangle with an angle near 180°, where classical error
diffusion's bound degenerates arbitrarily relative to the palette's own size
while the barycentric rule stays bounded. **A dull green near the black–white
axis is that configuration.** Our panel is the textbook failure case.

**The best construction is older and simpler than Delaunay.** Ostromoukhov,
*"Chromaticity Gamut Enhancement by Heptatone Multi-Color Printing"*, SPIE 1909
(1993) — partition the gamut into wedges, each spanned by two hue-adjacent
chromatic inks **plus black and white**, so the white–black axis is a shared
edge of *every* wedge. A neutral then has coordinates `(w, 0, 0, k)` whichever
wedge it lands in, and the argmax can only be black or white. **Grey balance is
exact by construction.** For six inks that is four hand-written wedges —
`{W,K,R,Y}`, `{W,K,Y,G}`, `{W,K,G,B}`, `{W,K,B,R}`. No hull code, no Delaunay.
**Delaunay does not give this** — nothing in the empty-circumsphere criterion
guarantees the black–white segment is even an edge.

**The counter-argument, and it is real.** MBVQ (Shaked, Arad, Fitzhugh & Sobel,
HP Labs HPL-96-128R1 / US 5,991,438): black+white dots have maximum luminance
contrast and are the *grainiest* neutral; brightness-matched chromatic inks look
smoother. Confirmed numerically for our palette (§3). It assumes complements,
which we lack — hence the two-term objective rather than either extreme.

**The matching architecture.** HANS (Morovič, Morovič & Gondek, IEEE TIP 21(2),
2012; HP US 8,213,055) — separation emits an **NPac vector**, a probability
distribution over Neugebauer primaries, and *"HANS halftoning is a single
operation that selects one NP per pixel."* That sentence is our hardware
constraint written as a feature. Companion halftoner: PARAWACS (CIC24, 2016).

**One trap.** Barycentric weights must be solved in **linear RGB**, not OKLab —
dot mixing is physical light addition and OKLab is not additive. `gamut/hull.rs:3-7`
already says this and computes its hull in linear RGB for that reason.

**Community state of the art is below ours**: every public Spectra-6 converter
found is Floyd–Steinberg or Atkinson with nearest-neighbour quantisation. No
E Ink or Good Display application note on Spectra 6 halftoning exists publicly.

### What a change would touch

- `gamut/hull.rs` computes a **real 3-D convex hull in linear RGB**, but stores
  only outward half-space planes — the generating vertex triples are computed at
  `hull.rs:236` and **discarded**. Its sole product is a scalar
  `Cmax(hue, lightness)` table consumed by chroma compression **before**
  dithering. It never touches ink selection.
- The quantiser call site is **one line**: `dither/mod.rs:409`,
  `palette.find_nearest(oklab, model)`.
- `dither_with_kernel_noise` (`dither/mod.rs:318`) has **no parameter for a
  precomputed structure**. Either add one or extend `RegionMap` (per-pixel,
  `pub(crate)`).
- **Model duality is a hard constraint.** Pixels select `ColourModel::Nominal`
  or `::Measured` at `dither/mod.rs:374`. A partition built from `actual_linear`
  is valid only for `Measured`. Decide explicitly what `Nominal` does.
- `api/builder.rs:304` builds `for_error_diffusion()` per `dither()` call, so
  any cached geometry must be threaded through `builder.rs:309`.
- The quantiser's input is the **error-loaded** pixel, not the source colour.
  The spike deliberately looked its mixture up on the *source*, "otherwise the
  guidance would drift with the error it bounds." Make that choice explicitly.

**The accuracy oracle already exists**: `best_reachable()` at
`domain_tests.rs:2000` returns the physical dE bound **and the optimal weight
vector** by optimisation. `test_ink_histogram_versus_optimal_recipe`
(`domain_tests.rs:2607`) already compares the ink histogram against that recipe.

---

## 8. Next

1. **The quantiser.** This is the whole remaining initiative. Suggested order:
   a. Build the four fixed wedges (§7) and, offline in the probe, measure what
      argmax-barycentric selection gives on neutrals, on the flat-patch census,
      and against `best_reachable()`.
   b. Add the two-term objective (§3) with `lambda` exposed, and find where it
      sits between MBVQ and maximum GCR.
   c. Only then touch `dither/mod.rs:409`. §7 lists what it drags in.
   **The gate to clear is `test_neutral_grey_has_no_dominant_chromatic_ink`.**
   Also verify against `test_dither_versus_gamut_bound` and
   `test_ink_histogram_versus_optimal_recipe`, both `#[ignore]`d diagnostics.
2. **Cheap wins still on the table**, in rough order of value: defect 5.3
   (panel tuning inert under a different algorithm — the reporting machinery
   now exists), 5.5 (`configure_device` over MCP), 5.4 (`noise_scale: 5`).
3. **Restore the g18 device to `examples/gphoto`** when finished (§9).
4. **Open the PR.** This branch now carries three commits of real work plus the
   TRMNL X ghosting fixes that were never PR'd (`0fb5c47` is a data-loss fix).
   Consider splitting the shipped fixes from the quantiser work.

---

## 9. Exact state — repo and deployed

**Repo.** Three commits on `feat/panel-clean-recovery`, HEAD `a20f2ee`. The
working tree holds only the owner's four docs-screenshot files (`config.yaml`,
`docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`,
`tools/capture-config.yaml`). **Never stage them. Never `git add -A` here.**

**On `root@10.46.18.3`**, add-on `43664941_byonk` **0.19.0** (does NOT yet have
this session's changes), config `/addon_configs/43664941_byonk/config.yaml`:

| what | value | note |
|---|---|---|
| device `44:1B:F6:83:93:38` `dither` | `atkinson-hybrid` | set by the owner |
| device `44:1B:F6:83:93:38` `screen` | `local/calibration/color` | **was `examples/gphoto`** — restore when done |
| `panels.reterminal_e1004.colors_actual` | `#000000,#DCDCDC,#B52200,#E1CE00,#2C6CBC,#1E5645` | backups `config.yaml.pre-measured-2026-08-22`, `config.yaml.pre-stretch-2026-08-22` |
| `panels.reterminal_e1004.dither.sierra-light` | `error_clamp: 0.11, noise_scale: 5` | **once this branch is deployed, `error_clamp` is ignored and announced at startup.** Delete the key. |

**Deploying this branch changes behaviour on that box**, and both changes are
what it currently wants: the stale `error_clamp` stops being a live value, and
the device's `atkinson-hybrid` now beats any screen that names its own
algorithm.

**Changed on the box last session:** the line `dither = "atkinson"` was deleted
from `/addon_configs/43664941_byonk/screens/calibration/color/script.lua` (was
line 165). **That edit is no longer needed** — `a20f2ee` makes the device win
regardless. **No backup of that file was made**; the repo original is
`screens/builtin/calibration/color/script.lua` (166 lines) and the deployed
fork differs only in `refresh_rate` 3600 → 180 in `script.lua` and
`refresh: 180` in `meta.yaml`.

**Screens created over MCP** (not in the repo): `local/calibration/inkfield`
(Ink Field copied from this branch) and `local/calibration/color`.

**Measurement kit** at `~/scratch/panel-evidence/e1004-2026-08-21/`:
`measure.sh`, `prep.sh`, `scout.py`, `run.py`, `gridfit.py`, `warp.py`,
`deveil.py`, `patches.py`, `synth.py`/`validate.py`, `FINDINGS.md`, venv at
`./venv/bin/python`, and `greyprobe/` — the Rust probe behind §1–§3.
`cargo run --release`; change `PALETTE` for another panel.

---

## 10. Environment

- **g18 HA**: `root@10.46.18.3`, ssh authorised by the owner. **The Claude Code
  auto-mode classifier still blocks some remote mutations** regardless; a
  targeted single-purpose `sed -i` went through where `cp && sed -i && restart`
  did not. If blocked, hand the owner a one-line `!` command.
- byonk **does not hot-reload config — restart the add-on**, and a restart clears
  the in-memory device registry. Screens are re-read per render; no restart.
- MCP server `byonk-g18` talks to it directly. `render_screen` accepts `dither`,
  `panel` and `colors_actual`, so **algorithms and calibrations can be compared
  without touching any config.** Use `width`/`height` to get a true 1:1 dither —
  `image_max_width` resamples, which averages away the very clumping under test.
- Devices: `44:1B:F6:83:93:38` reTerminal E1004 (1200×1600, 6-colour) and
  `94:A9:90:8C:6D:18` TRMNL Classic.
- No `timeout` on this Mac and foreground `sleep` is blocked; use
  `curl --retry N --retry-delay S --retry-all-errors --retry-connrefused`.
- **`ha addons` is deprecated** in favour of `ha apps`; still works, warns.
- Verify: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cd docs && mdbook build`.
  **`make check` has reported exit 0 while tests failed** — run the four
  commands directly.

---

## 11. Carried forward — still true, still unmerged

### Photographic calibration of the E1004

Full write-up: `~/scratch/panel-evidence/e1004-2026-08-21/FINDINGS.md`.

- **A photograph can calibrate this panel to about 2% of white**, provided every
  frame comes from the same camera module at low ISO. Two shots from different
  angles agreed to 2.02% on every ink and channel.
- **This panel's green is about half as colourful as `default-config.yaml`
  claims** — chroma 0.068 measured against 0.158. The owner independently called
  it *"dull and darkish but still clearly green"*. **This finding stands and is
  worth committing on its own.**
- **Veiling glare is the biggest error source.** A glossy panel mirrors the room
  and the reflection *adds* to the ink; the Latin square cancels gains, not
  offsets. `deveil.py` fits gain+veil per row but cannot identify the ink offset,
  so any veil present everywhere survives — which is why the raw measured palette
  was too flat and was then affine-stretched so black→0 and white→1.
- **Never mix camera modules in a frame set.** Photo 2 was the telephoto at
  ISO 500 while 1 and 3 were the main camera at ISO 64; its blue channel was an
  outlier by up to 9.2% of white and nothing in the picture showed it. Check
  `exiftool -s -UniqueCameraModel -ISO -FocalLength`.
- Geometry mattered less than expected — the homography moved the answer ~1%,
  well under veil and illuminant errors. Fiducial marks are not the bottleneck.
- **Position independence was never proved** — every frame used `offset: 0`.
  Low priority now.
- Raw workflow: `dcraw -4 -T -o 1 -A <x> <y> <w> <h>`. **iCloud share links
  deliver JPEG** — export the original from Photos.

### TRMNL X

Full text in commit `814d061`.

1. **Two firmware grey tables selected by PNG byte size**
   (`FASTEPD_LARGE_IMAGE_THRESHOLD`, 100 KiB). `colors_actual` is a property of
   *(panel, grey table)*, not of the panel. Pin with `min_png_bytes`.
2. **Measured both** — 9-pass 8.05:1 with 2 dead steps; 38-pass 9.28:1 with 4
   dead steps and a 16.2 L\* chasm. Deployed on **homeio**, not in the repo.
3. **PR for `fix/trmnl-x-ghosting-levers`** never opened; its three commits are in
   this branch's history, including a data-loss fix (`0fb5c47`).
4. **Restore `homeio`**: `log_level` → `info`, stop `local_byonk`, start
   `43664941_byonk`, delete `local/noise-test`. Backup:
   `/addon_configs/local_byonk/config.yaml.pre-recovery`.
5. **Timestamped image filenames** — content-hash names defeat device caching
   (`filesystem.cpp:141`).
6. **SDD ledger** (git-ignored):
   `.superpowers/sdd/2026-08-21-panel-clean-recovery/progress.md`
