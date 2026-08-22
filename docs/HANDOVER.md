# Handover — the quantiser is built and measured; its justification did not survive

**Date:** 2026-08-22 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `0888e59`
**Base:** `main` @ `5c67c62` (v0.19.0, protected). `main` is an ancestor; no rebase needed.

> **`cargo test --workspace` FAILS ON PURPOSE.** Exactly one test is red:
> `test_neutral_grey_has_no_dominant_chromatic_ink`
> (`crates/eink-dither/src/domain_tests.rs`). **247 passed / 1 failed / 28
> ignored in `eink-dither` is the expected state.** Any *other* failure is a
> real regression.

## Resume here

**Do not start Task 6.** The plan's Tasks 1–5 are committed and reviewed. Task 6
sets the production value, and the evidence that would justify any value is the
thing that fell over this session. Read §1 before doing anything else.

1. **§1 — the correction.** The premise the whole initiative rests on does not
   survive being checked. This is the most important section.
2. **§2** — what *is* proven, and it is worth having.
3. **§3** — the one experiment that decides whether this ships.
4. Spec: `docs/superpowers/specs/2026-08-22-mixture-aware-quantiser-design.md`
   (now partly superseded — see §4). Plan:
   `docs/superpowers/plans/2026-08-22-mixture-aware-quantiser.md`.
5. Ledger (git-ignored):
   `.superpowers/sdd/2026-08-22-mixture-aware-quantiser/progress.md`. It holds
   every ruling, every deferred minor, and the commit ranges. Trust it plus
   `git log` over memory. Reports and renders sit beside it in the same
   directory.

---

## 1. The correction — "the eye reads the majority ink" is not supported

The spec's §1 and the last two handovers all rest on one claim:

> **dE is not the symptom.** The average is right and the *field colour* is
> wrong, because the eye reads the majority ink as the colour of the area
> rather than averaging.

**Measured this session, that does not hold for flat patches.** Take the
rendered neutral ramp at `mixture_bias = 0.0` and merge the dots the way an eye
at reading distance does — averaging in **linear light**, which is the only
correct way — and it comes out neutral:

| merge scale | worst green excess, bias 0.0 | bias 0.5 |
|---|---|---|
| 8×8 | +1 of 255 | +0 |
| 16×16 | +5.9 of 255 (in the near-black end) | +0.8 |
| 32×32 | +4.1 of 255 | +0.3 |

Column averages of the bias-0.0 ramp: `#3F4041`, `#656566`, `#8C8C8C`. Grey.

**A trap worth recording:** averaging the same image in **sRGB** instead of
linear light *does* show a green cast and a large lightness error. That is an
artifact of averaging gamma-encoded values, and it is what I nearly reported as
a finding. The codebase already insists on this distinction
(`test_gamma_correctness_dither_ratios`); apply it to analysis too, not only to
the ditherer.

So: it is true that a mid grey is **87% green ink with 0% black**. It is *not*
established that this looks green. The two were treated as the same thing.

**What remains true and unexplained:** the owner reported *"the face of the
person is clearly greenish where it is supposed to be brownish"* on the real
panel, from the calibration photo. That observation is the only ground truth in
this whole initiative, and **the synthetic flat-patch renders do not reproduce
it.** Something outside the flat-patch model causes it. Until that is found, no
value of `mixture_bias` can be justified by the evidence collected so far.

Candidate explanations, none tested:

- Real photographic content has gradients and structure; uniform patches do not.
  Clumping may behave differently there.
- The deployed panel runs `atkinson-hybrid`, which no synthetic render here used
  for the before/after pair.
- The panel's physical inks may differ from `colors_actual` in a way that an
  87%-single-ink mixture amplifies and a black/white mixture does not. The spec
  measured calibration perturbation as worth at most dE 0.013, which argues
  against this, but it was measured for a different question.
- Viewing conditions: a glossy panel under room light is not a monitor.

## 2. What *is* proven, and it is worth keeping

### 2.1 Atkinson is the single biggest accuracy cost in the pipeline

`kernel.rs:56` — Atkinson is six taps of weight 1 over a divisor of 8. **It
propagates 6/8 and discards 25% of its error by construction.** Floyd–Steinberg
is 16/16.

**With the new feature switched off, on the E1004 palette:**

| kernel | mean dE at `mixture_bias = 0.0` |
|---|---|
| Atkinson | 0.0286 |
| Floyd–Steinberg | **0.0029** |

**Ten times.** The device `44:1B:F6:83:93:38` is set to `atkinson-hybrid`. This
finding is independent of the quantiser and is worth acting on by itself.

The spec's central defence of the whole approach — *"the error term is
untouched, so the average still converges"* — is only true for a kernel that
propagates all of it. Under Atkinson the discarded quarter compounds and the
mean drifts, worst on the dark near-neutrals whose recipes call for the most
black. That was the hypothesis; it survived in mechanism.

### 2.2 The lever does what it was designed to do

Floyd–Steinberg, `max_error = 2.0`, `mixture_bias = 0.5`, on `panel_e1004()`:

| | bias 0.0 | bias 0.5 |
|---|---|---|
| `#505050` fraction wearing an ink the recipe never called for | 1.0000 | **0.0031** |
| `#606060` | 1.0000 | 0.0156 |
| max dE over the in-gamut census | 0.0778 | **0.0741** (bound 0.10) |

Accuracy *improves*. The same setting also improves `panel_measured()` (max dE
0.0962 → 0.0692), so **no panel is traded for another**. `max_error` 1.5/3.0/5.0
also pass; 1.0 fails at 0.1169 and fully unclamped fails at 0.1037, so 2.0 is a
real optimum rather than a limit trick. The default `max_error` of 1.0 was
clamping 17% of channel applications for Floyd–Steinberg and costing accuracy.

### 2.3 It introduces a visible seam

Largest single-step change in an ink's share between adjacent colours:

| | bias 0.0 | bias 0.5 |
|---|---|---|
| e1004 | 0.0290 (`#B0A77E → #B1A87F`) | **0.1680** (`#55332A → #56332B`) |
| measured | 0.0268 | 0.1445 (`#AC6756 → #AD6756`) |

Both on the warm-brown / skin ramp; the neutral ramp stays ≤ 0.079. For scale,
an earlier attempt at this fix was abandoned after measuring **0.090**, and
`wedges.rs`'s own continuity bound is 0.020.

**It is visible at 1:1, not only magnified** — I looked. A hard vertical edge
about halfway along the brown gradient: sparse dots on near-black one side,
dense red/green mottle the other. Raising `max_error` makes banding worse
(measured: 0.1311 → 0.1445 → 0.1708 as the clamp goes 1.0 → 2.0 → ∞) while
improving accuracy, so those two pull against each other.

**Why it bands.** The weights vary continuously — Task 2 proves that
numerically. But the *winner* is a threshold on the score gap, so when the
discount is large relative to the error excursions error diffusion produces, a
whole uniform region flips at once instead of easing over. Lowering the lever
should restore smoothness at the cost of repair. **Nobody has measured where
that knee is** — that is a cheap, obvious next measurement if the initiative
continues.

### 2.4 The renders

Twelve PNGs and a written index in
`.superpowers/sdd/2026-08-22-mixture-aware-quantiser/renders/`, plus the two
`06_neutral_AT_DISTANCE_*` images generated for §1. The neutral-ramp pair is the
clearest: bias 0.0 is blue/yellow/red/green confetti at 1:1, bias 0.5 is clean
black-and-white. **The owner's reaction to the confetti was "that looks pretty
cool" — which is fair, and prompted §1.**

## 3. The one experiment that decides this

**Reproduce the owner's actual observation.** Render the calibration photo with
the face — `local/calibration/color` on the g18 box — before and after, on the
real panel, and look.

- If the fix visibly helps there, it earns its place and the seam becomes a
  tuning problem (find the knee, §2.3).
- If it does not, this whole initiative rests on a test artifact and should be
  dropped, keeping only §2.1 (the Atkinson finding) and the wedge fan itself,
  which is sound geometry with its own tests.

`mixture_bias` is internal, so `render_screen` cannot vary it — a before/after
pair needs two builds on the VM (plan Task 7 Step 1, and the
`ha-vm-from-source-addon-build` memory has the recipe). **The owner has not yet
authorised the ssh for this.** Ask.

## 4. Rulings made this session — do not relitigate, but §1 reopens some

Full text with costs-if-wrong in the ledger. The load-bearing ones:

1. **Out-of-gamut colours are projected onto the fan, not extrapolated across
   it.** The spec's §3.2 claim that clamp-and-renormalise "degrades to the
   nearest wedge" is **measured false** — the weight field jumped **0.9137**
   between two colours one 8-bit step apart. `weights()` now keeps the wedge
   whose nearest point is nearest. Continuity went to 0.019047. Proof and
   measurement are in commit `88d0f61`'s message.
2. **The inks are ordered by linear-RGB dihedral angle about the black–white
   axis, not OKLab hue** (spec §3.1 step 2 superseded). Choosing the partition
   is mixture geometry, and mixture geometry is linear RGB. The order happens to
   be identical for both test palettes, so this removed a latent failure.
3. **The tessellation premise is enforced, not assumed.** `from_palette` refuses
   the fan unless the wedge volumes sum to the hull volume (1e-3 relative).
   `Hull::volume()` was added for it. A palette that fails renders exactly as it
   does today rather than silently contouring.
4. **The census and the wiring test dither `for_error_diffusion()`.** Euclidean
   is the only metric production ever dithers under (`builder.rs:313`), and the
   defect exists only on that path — under raw HyAB a mid grey is already 72.4%
   black and there is nothing to fix.
5. **The field-colour rule is replaced by recipe agreement** — half the L1
   distance between the rendered ink histogram and the fan's exact recipe. The
   old rule condemned correct output; see §5. **§1 does not invalidate this
   measure**, which is a good one; it invalidates the claim that a bad score is
   visible.
6. **`panel_e1004()` is added to `test_support`** and the census gates on it.

## 5. Why the old gate had to go — do not restore it

The spec justified its field-colour threshold with: *"On `panel_measured()` the
dullest ink is green at chroma 0.068."* **That number is not `panel_measured()`'s
green.** Computed directly:

| ink | hex | OKLab chroma |
|---|---|---|
| red | `#B50303` | 0.1982 |
| yellow | `#FFEE00` | 0.1973 |
| blue | `#205497` | 0.1227 |
| **green** | `#0D876B` | **0.1062** |

0.068 is the **E1004's** green, `#1E5645` (0.0655). Spec §1 itself says
`panel_measured()` is an **E1002**. The spec took its evidence from one panel and
its threshold from another.

With the threshold at 0.106 the rule condemned 261 of 677 colours, including
`#108070` (a teal, dE 0.0137) for being 92.8% green. **Decisive proof the rule
was wrong rather than the fix:** `worst 1ink` sat at exactly 100.0% at every
lambda on the **idealised** BWRGBY palette, where the dullest ink is ~0.21, so a
near-pure yellow counts as "duller than the dullest ink" and rendering it 100%
yellow is called a defect. No lever value could ever pass.

## 6. Uncommitted work in the tree — decide before touching anything

`git status` shows six modified files. **Two are this session's unfinished work
and four are the owner's.**

| file | whose | what |
|---|---|---|
| `crates/eink-dither/src/domain_tests.rs` | **mine, +299/−34** | the recipe-agreement rule replacing the field-colour rule, bound left at `f32::INFINITY` and printed rather than asserted |
| `crates/eink-dither/src/gamut/mod.rs` | **mine, +36** | `panel_e1004()` in `test_support` |
| `config.yaml` | **owner** | never stage |
| `docs/generate-samples.sh` | **owner** | never stage |
| `docs/src/concepts/content-pipeline.md` | **owner** | never stage — and it still documents the pre-0.18.0 `error_clamp` at line 261 with a stale `0.05 – 0.5` range; **the owner fixes that line** |
| `tools/capture-config.yaml` | **owner** | never stage |

**Never `git add -A` or `git add .` here.** Add by explicit path and check
`git diff --cached --name-only` before every commit. There is no backup of the
owner's four files.

The two mine are committable on their own merits — `panel_e1004()` is a fact
about a real panel, and the recipe-agreement measure is better than what it
replaces — but **the bound cannot be set until §3 resolves**, because its whole
purpose is to be red before the fix and green after.

## 7. Deferred review findings — for whoever opens the PR

None blocking; all are in the ledger with context.

- **Task 1:** `worst_de` is a dead accumulator now that its `println!` is gone.
- **Task 2:** `a_greyscale_palette_carries_no_fan` passes on the `len < 5` guard,
  so the `chromatic.len() < 3`, `white == black`, non-mappable-hull and
  `invert3` branches have no coverage. Each K–W–c face is solved twice on the
  slow path (4 of 16 point-triangle cases are duplicates). `TOL = 1e-4` in the
  tessellation test is empirical (measured worst boundary rounding 8e-5) and
  does not say so. The continuity assert records the conclusion that 0.019047 is
  a slope but not how to re-derive it. `hull_area_2d` discards points interior to
  a facet, and neither grounding fixture exercises that.
- **Task 3:** `find_nearest_biased` duplicates `find_nearest`'s `pixel_chroma`
  and `entries` setup. Its doc says `bias` sums to one, which the code neither
  requires nor enforces — the all-zero give-up vector from `WedgeFan::weights`
  deliberately violates it and degrades to plain nearest-neighbour, correctly.
- **Task 4:** the `mixture` buffer is allocated even when the feature is off.
- **Task 5:** the sweep table prints no way to identify *which* colour produced a
  worst case — that cost a whole separate investigation. Its doc comment's
  suggested invocation is broken: `--release` sits after `--` so cargo never
  sees it, and the bare `lambda_sweep` filter collides by substring with
  `lambda_sweep_diag`. The working form is
  `cargo test -p eink-dither --release --lib lambda_sweep -- --ignored --nocapture --exact domain_tests::domain_tests::lambda_sweep`.

## 8. byonk defects — still open, unchanged from last session

1. **The dev UI duplicates `DitherAlgorithm::defaults()`** (`static/dev/dev.js`,
   `DITHER_DEFAULTS`). It should fetch defaults from the server.
2. **`docs/src/concepts/content-pipeline.md:261`** — see §6.
3. **A panel's tuned dither parameters silently do nothing when the device picks
   a different algorithm.** `dither:` is keyed by algorithm name; the E1004's
   `sierra-light` block is inert under `atkinson-hybrid` with no warning.
   `deprecation_warnings()` is the obvious place to report from.
4. **`noise_scale: 5`** in the shipped E1002/E1004 blocks; the measured optimum
   for Sierra Lite is 2.5 (`dither/mod.rs:166`).
5. **MCP cannot set a device's dither algorithm.** `assign_screen` takes only
   `mac` and `screen_ref` while `apply_device_patch` (`write.rs:249`) already
   accepts `dither`, `panel`, `refresh`, `colors`, `params`, `name`. Widen it and
   rename `assign_screen` → `configure_device`.
6. **Panels have no write path at all** — no `/panels` route in `admin_router()`,
   REST or MCP, in any mode. The whole calibration workflow requires ssh.
7. **`colors_actual` serves two masters** — an honest preview and the ditherer's
   ink choice. Possibly two fields.
8. **`sierra-lite` deserves one re-test** with a sane `max_error`; it was written
   off while the panel config carried a pre-0.18.0 `error_clamp: 0.11`.

Still open from TRMNL X: the palette rejects duplicate colours, and
`map_grey_indices` derives the level from an entry's **index** rather than its
hex, so "declare only the usable inks" does not work.

## 9. After this

1. **Restore the g18 device to `examples/gphoto`** — it is on
   `local/calibration/color`.
2. **§2.1 is shippable on its own.** Atkinson being 10× less accurate than
   Floyd–Steinberg is a finding about byonk's defaults, independent of the
   quantiser.
3. **Cheap wins**, in rough order of value: defect 3 (panel tuning inert), 5
   (`configure_device` over MCP), 4 (`noise_scale`).
4. **Open the PR.** This branch carries three fixes from an earlier session, this
   initiative, and the TRMNL X ghosting fixes that were never PR'd (`0fb5c47` is
   a data-loss fix). Consider splitting.

## 10. Exact state — repo and deployed

**Repo.** HEAD `0888e59` on `feat/panel-clean-recovery`. Ten commits since
`main` @ `5c67c62`. This session added five:

| commit | task |
|---|---|
| `a31edf1` | Task 1 — the in-gamut census gate, red on purpose |
| `88d0f61` | Task 2 — the wedge fan (two fix rounds; see §4.1–4.3) |
| `6335754` | Task 3 — `find_nearest_biased` (one fix round) |
| `3c57db8` | Task 4 — wired into the dither loop, shipped off |
| `0888e59` | Task 5 — the `lambda_sweep` diagnostic |

All five were reviewed clean. Uncommitted work: §6.

**On `root@10.46.18.3`**, add-on `43664941_byonk` **0.19.0** (does NOT have any
of this branch), config `/addon_configs/43664941_byonk/config.yaml`:

| what | value | note |
|---|---|---|
| device `44:1B:F6:83:93:38` `dither` | `atkinson-hybrid` | set by the owner — see §2.1 |
| device `44:1B:F6:83:93:38` `screen` | `local/calibration/color` | **was `examples/gphoto`** — restore when done |
| `panels.reterminal_e1004.colors_actual` | `#000000,#DCDCDC,#B52200,#E1CE00,#2C6CBC,#1E5645` | backups `config.yaml.pre-measured-2026-08-22`, `config.yaml.pre-stretch-2026-08-22` |
| `panels.reterminal_e1004.dither.sierra-light` | `error_clamp: 0.11, noise_scale: 5` | once this branch is deployed the key is ignored and announced at startup. Delete it. |

**Deploying this branch changes behaviour on that box**, and both changes are
wanted: the stale `error_clamp` stops being live, and the device's
`atkinson-hybrid` now beats any screen naming its own algorithm.

**Changed on the box previously:** `dither = "atkinson"` was deleted from
`/addon_configs/43664941_byonk/screens/calibration/color/script.lua` (was line
165). **That edit is no longer needed** — `a20f2ee` makes the device win
regardless. No backup was made; the repo original is
`screens/builtin/calibration/color/script.lua` (166 lines) and the deployed fork
differs only in `refresh_rate` 3600 → 180 and `refresh: 180` in `meta.yaml`.

**Screens created over MCP** (not in the repo): `local/calibration/inkfield` and
`local/calibration/color`.

**Measurement kit** at `~/scratch/panel-evidence/e1004-2026-08-21/`: `measure.sh`,
`prep.sh`, `scout.py`, `run.py`, `gridfit.py`, `warp.py`, `deveil.py`,
`patches.py`, `synth.py`/`validate.py`, `FINDINGS.md`, venv at
`./venv/bin/python`, and `greyprobe/` — the Rust probe behind the earlier
neutral-ramp numbers. `cargo run --release`; change `PALETTE` for another panel.

## 11. Environment

- **Verify with the four commands directly. `make check` has reported exit 0
  while tests failed.**
  ```
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd docs && mdbook build
  ```
- **`git` needs `--no-pager`** in this harness, or `diff --stat`/`log` can return
  nothing at all and look like a clean tree. That nearly cost a wrong conclusion.
- **Subagents must run long commands in the foreground.** A subagent that ends
  its turn waiting on a background job is never resumed, and the work stalls
  silently. Two did this session.
- **Always build the review package before dispatching a reviewer**
  (`scripts/review-package PLAN BASE HEAD`). A reviewer with no diff file crawls
  the repo and stalls — one died at the 600s watchdog for exactly this.
- **g18 HA**: `root@10.46.18.3`, ssh authorised by the owner *for that box* — but
  **§3's build-and-deploy has not been authorised. Ask.** The auto-mode
  classifier blocks some remote mutations; a targeted single-purpose `sed -i`
  went through where `cp && sed -i && restart` did not. If blocked, hand the
  owner a one-line `!` command.
- byonk **does not hot-reload config — restart the add-on**, and a restart clears
  the in-memory device registry. Screens are re-read per render.
- MCP server `byonk-g18` talks to it directly. `render_screen` accepts `dither`,
  `panel` and `colors_actual`, so algorithms and calibrations can be compared
  without touching config. **Pass explicit `width`/`height`** —
  `image_max_width` resamples. It cannot vary `mixture_bias`, which is internal.
- Devices: `44:1B:F6:83:93:38` reTerminal E1004 (1200×1600, 6-colour) and
  `94:A9:90:8C:6D:18` TRMNL Classic.
- No `timeout` on this Mac and foreground `sleep` is blocked; use
  `curl --retry N --retry-delay S --retry-all-errors --retry-connrefused`.
- **`ha addons` is deprecated** in favour of `ha apps`; still works, warns.
- Python has **PIL 12.1.0 but no numpy**. Pure-Python pixel loops are fine at
  these image sizes.

## 12. Carried forward — still true, still unmerged

### Photographic calibration of the E1004

Full write-up: `~/scratch/panel-evidence/e1004-2026-08-21/FINDINGS.md`.

- **A photograph can calibrate this panel to about 2% of white**, provided every
  frame comes from the same camera module at low ISO.
- **This panel's green is about half as colourful as `default-config.yaml`
  claims** — chroma 0.0655 measured against 0.158. The owner independently called
  it *"dull and darkish but still clearly green"*. This finding stands and is
  worth committing on its own.
- **Veiling glare is the biggest error source.** A glossy panel mirrors the room
  and the reflection *adds* to the ink; the Latin square cancels gains, not
  offsets. `deveil.py` fits gain+veil per row but cannot identify the ink offset,
  which is why the raw measured palette was too flat and was then
  affine-stretched so black→0 and white→1.
- **Never mix camera modules in a frame set.** One telephoto frame at ISO 500
  among ISO 64 main-camera frames was a blue-channel outlier by up to 9.2% of
  white, invisible in the picture. Check
  `exiftool -s -UniqueCameraModel -ISO -FocalLength`.
- Geometry mattered less than expected — the homography moved the answer ~1%.
- **Position independence was never proved** — every frame used `offset: 0`.
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
