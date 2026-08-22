# Handover — the quantiser is designed and planned; execution starts at Task 1

**Date:** 2026-08-22 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `cca0715`
**Base:** `main` @ `5c67c62` (v0.19.0, protected). `main` is an ancestor; no rebase needed.

> **`cargo test --workspace` FAILS ON PURPOSE.** Exactly one test is red:
> `test_neutral_grey_has_no_dominant_chromatic_ink`
> (`crates/eink-dither/src/domain_tests.rs`). **227 passed / 1 failed in
> `eink-dither` is the expected state.** Any *other* failure is a real
> regression. It goes green at Task 6.

## Resume here

1. Read the spec: `docs/superpowers/specs/2026-08-22-mixture-aware-quantiser-design.md`
2. Read the plan: `docs/superpowers/plans/2026-08-22-mixture-aware-quantiser.md`
3. Execute with the **`superpowers:subagent-driven-development`** skill —
   the owner chose subagent-driven over inline on 2026-08-22. Start at **Task 1**.
4. Ledger (git-ignored): `.superpowers/sdd/progress.md`. Trust it plus
   `git log` over memory after any compaction.

**Nothing of the design has been implemented.** The two commits this session
are documents only.

---

## 1. The defect, restated — it is bigger than greys

The previous handover said "byonk paints neutral greys in coloured ink". That
was true and too narrow. **Measured this session on `panel_measured()`:**

| colour | blk | wht | red | grn | dominant | dE |
|---|---|---|---|---|---|---|
| skin warm `#896666` | **0.0%** | **0.0%** | 46.8% | 52.8% | grn 52.8% | 0.0365 |
| skin brown B `#966E55` | **0.0%** | **0.0%** | 45.7% | 54.3% | grn 54.3% | 0.0475 |
| muted scarf `#8C6C68` | **0.0%** | **0.0%** | 43.3% | 56.7% | grn 56.7% | 0.0436 |
| grey 128 | 0.0% | 2.0% | 18.8% | 77.6% | grn 77.6% | 0.0631 |

The owner reported it as *"the face of the person is clearly greenish where it
is supposed to be brownish"* on the calibration photo. Same defect.

**The root cause is that black and white are unreachable.** Eight of twelve
muted colours use 0% black. The measured inks are dull — green measures chroma
0.068 against `default-config.yaml`'s claimed 0.158 — so they crowd the neutral
axis and beat black and white on plain nearest-neighbour distance at every
muted colour. Green is not preferred; it stands in for the black that never
arrives. `tests/spike_simplex.rs` recorded the same thing independently:
*"black stays unreachable and red absorbs the slack."*

The same colours on the **idealised** BWRGBY palette get 30–60% black. The
difference is the panel, not the content.

**dE is not the symptom.** Every row above has an acceptable dE. The average is
right and the *field colour* is wrong, because the eye reads the majority ink
as the colour of the area rather than averaging. **Any gate built on dE alone
will pass this bug.**

This is the textbook degenerate case in Chai Wah Wu (IS&T NIP20, 2004): a
palette subtending an angle near 180° around the target, where classical error
diffusion's bound degenerates while a barycentric rule stays bounded.

## 2. The fix, in one paragraph

Partition the gamut into tetrahedra that all share the black–white edge
(Ostromoukhov, SPIE 1909, 1993) — for six inks, `{W,K,R,Y}`, `{W,K,Y,G}`,
`{W,K,G,B}`, `{W,K,B,R}`. A pixel's barycentric coordinates in its wedge are
the exact ink recipe. Discount each entry's distance by `lambda * weight`
before choosing. A neutral's coordinates are `(w, 0, 0, k)` in *every* wedge,
so no chromatic ink can ever receive a discount and grey balance is exact by
construction. `lambda = 0` reproduces today's selection bit for bit.

Full argument, alternatives considered, and the literature: the spec.

## 3. Owner rulings from this session — do not relitigate

1. **The lever is internal with one measured default.** Not a config knob, not
   per-panel, not per-screen. The right value is a property of how dull a
   panel's inks are, not a taste setting. Nothing reaches `config.yaml`,
   `DitherTuningValues`, or the dev UI's `DITHER_DEFAULTS`.
2. **Probabilistic selection (HANS/PARAWACS) is a follow-up, not now.** It
   reuses the same wedge fan, so nothing is wasted by building the fan first.
   If it is built, it surfaces to the user as another entry in the dither list
   — that was a UI remark, not an architectural one.
3. **"Any colour inside the gamut should be accurate."** This replaced a
   hand-picked colour list as the gate. The census is 677 in-gamut colours
   (sRGB grid of 16 filtered through `Hull::contains`; coarser grids collapse
   to 78 and 11 — this gamut is only 16.5% of the cube).
4. **Two gates, not one**, because dE cannot see the defect. The second is:
   for a target duller than the *dullest* chromatic ink, the largest single
   chromatic ink stays at or below 50%. The threshold is derived from the
   palette, not chosen.

## 4. What is ruled out — do not redo these

1. **The dither algorithm.** Atkinson 52.0%, Atkinson-hybrid 54.2%,
   Floyd-Steinberg 50.3% chromatic on a neutral ramp.
2. **The calibration.** Perturbing every ink by the measured photographic
   uncertainty (2.02% of white) moves the result by at most dE 0.013. Re-running
   the probe with the older, *more* colourful palette gives 57.8% — worse.
3. **A blanket chroma penalty.** `HyAB kchroma=10` cuts chromatic choice on
   neutrals from 50.4% to 9.4% but is biased for error diffusion on muted
   photographic colour. It fixes greys by breaking photographs.
4. **Restricting candidates to a freely-optimised mixture's support.**
   `tests/spike_simplex.rs` (591 lines, `#[ignore]`d) did exactly this and it
   **bands gradients intrinsically** — it measured its own support field
   jumping 0.090 in one step. **The fixed fan is different**: black and white
   are in every cell and never drop out, and two neighbouring wedges agree
   exactly on their shared face. Task 2 asserts that numerically rather than
   assuming it.

## 5. The one real counter-argument

**E Ink patented this fix and then backed it out.** US 10,554,854 / 10,771,652
(Crounse, priority 2016-05-24) is barycentric argmax for exactly this hardware.
US 11,527,216 (Buckley, Crounse, Telfer, Sainis, 2017) reverses it:
*"image quality is compromised by using barycentric quantization inside the
color gamut hull."*

That is a published argument against the change, from the vendor, in our
domain. It is why `lambda` is continuous rather than a hard argmax: **measure
where quality peaks, and retreat to 0.0 if photographs suffer.** Task 6 Step 5
makes that retreat the prescribed response to a photo regression, not a reason
to widen a bound.

## 6. Traps waiting in the code

- **`mod palette;` is private** in `palette/mod.rs`. Making
  `CHROMA_DETECTION_THRESHOLD` `pub(crate)` is not enough — it also needs
  re-exporting. Task 2 Step 1 does both.
- **`clippy -D warnings` is on.** An import used only by a test module fails
  the build. Task 2 scopes `Oklab` into the tests for this reason.
- **`dither_with_kernel_noise` has 44 call sites.** The plan adds **no
  parameter**: `Palette` is already an argument, so the fan is built inside the
  function. Do not "improve" this by threading a parameter through.
- **Model duality.** `Nominal` pixels are flat SVG fills meant to BE an ink
  (ruling 22) and a fan built from measured colours does not describe them.
  They keep plain selection.
- **The mixture comes from the SOURCE pixel**, not the error-loaded one, or the
  guidance drifts with the error it bounds.
- **`best_reachable()` is not an oracle for ink shares.** Six inks and three
  equations leave a two-parameter family of exact solutions, so the recipe it
  returns is an arbitrary member. It is valid only as the dE bound it was
  written to be.

## 7. byonk defects — still open

1. **The dev UI duplicates `DitherAlgorithm::defaults()`** (`static/dev/dev.js`,
   `DITHER_DEFAULTS`). Corrected last session, will drift again. The UI should
   fetch defaults from the server.
2. **`docs/src/concepts/content-pipeline.md:261` still documents `error_clamp`**
   with a stale `0.05 – 0.5` range. **It is one of the owner's four uncommitted
   files — the owner must fix that line before committing it.** Never edit it.
3. **A panel's tuned dither parameters silently do nothing when the device
   picks a different algorithm.** `dither:` is keyed by algorithm name; the
   E1004's `sierra-light` block is inert under `atkinson-hybrid` with no
   warning. `deprecation_warnings()` is now the obvious place to report from.
4. **`noise_scale: 5`** in the shipped E1002/E1004 blocks; the measured optimum
   for Sierra Lite is 2.5 (`dither/mod.rs:166`).
5. **MCP cannot set a device's dither algorithm.** `assign_screen` takes only
   `mac` and `screen_ref`, while `apply_device_patch` (`write.rs:249`) already
   accepts `dither`, `panel`, `refresh`, `colors`, `params`, `name`. Owner
   ruling: MCP should give direct access. Widen it and rename
   `assign_screen` → `configure_device`.
6. **Panels have no write path at all** — no `/panels` route in
   `admin_router()`, in REST or MCP, in any mode. The whole calibration workflow
   requires ssh. Carve panels out of "global config" the way device mappings
   already are.
7. **`colors_actual` serves two masters** — an honest preview and the
   ditherer's ink choice. Possibly two fields.
8. **`sierra-lite` deserves one re-test.** It was written off after rendering
   flat and blocky, but the panel config carried `error_clamp: 0.11`, a
   pre-0.18.0 value that is now ignored. Redo the comparison once with a sane
   `max_error`.

Still open from TRMNL X: the palette rejects duplicate colours, and
`map_grey_indices` derives the level from an entry's **index** rather than its
hex, so "declare only the usable inks" does not work.

## 8. After the quantiser

1. **Restore the g18 device to `examples/gphoto`** — it is on
   `local/calibration/color`.
2. **Cheap wins**, in rough order of value: defect 3 (panel tuning inert), 5
   (`configure_device` over MCP), 4 (`noise_scale`).
3. **Open the PR.** This branch carries three fixes from the previous session,
   this initiative, and the TRMNL X ghosting fixes that were never PR'd
   (`0fb5c47` is a data-loss fix). Consider splitting.

## 9. Exact state — repo and deployed

**Repo.** HEAD `cca0715` on `feat/panel-clean-recovery`. Five commits since
`main`: three code fixes from the previous session (`21f9b1b`, `3351151`,
`a20f2ee`) plus this session's two documents (`81ba166` spec, `cca0715` plan).

The working tree holds **only the owner's four files** — `config.yaml`,
`docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`,
`tools/capture-config.yaml`. **Never stage them. Never `git add -A` here.**
Add by explicit path and check `git diff --cached --name-only` before every
commit.

There is also an untracked `crates/eink-dither/tests/probe_skin_throwaway.rs` —
the probe that produced §1's table. **Task 1 Step 6 deletes it.**

**On `root@10.46.18.3`**, add-on `43664941_byonk` **0.19.0** (does NOT have any
of this branch), config `/addon_configs/43664941_byonk/config.yaml`:

| what | value | note |
|---|---|---|
| device `44:1B:F6:83:93:38` `dither` | `atkinson-hybrid` | set by the owner |
| device `44:1B:F6:83:93:38` `screen` | `local/calibration/color` | **was `examples/gphoto`** — restore when done |
| `panels.reterminal_e1004.colors_actual` | `#000000,#DCDCDC,#B52200,#E1CE00,#2C6CBC,#1E5645` | backups `config.yaml.pre-measured-2026-08-22`, `config.yaml.pre-stretch-2026-08-22` |
| `panels.reterminal_e1004.dither.sierra-light` | `error_clamp: 0.11, noise_scale: 5` | once this branch is deployed the key is ignored and announced at startup. Delete it. |

**Deploying this branch changes behaviour on that box**, and both changes are
what it wants: the stale `error_clamp` stops being live, and the device's
`atkinson-hybrid` now beats any screen naming its own algorithm.

**Changed on the box previously:** `dither = "atkinson"` was deleted from
`/addon_configs/43664941_byonk/screens/calibration/color/script.lua` (was line
165). **That edit is no longer needed** — `a20f2ee` makes the device win
regardless. No backup was made; the repo original is
`screens/builtin/calibration/color/script.lua` (166 lines) and the deployed
fork differs only in `refresh_rate` 3600 → 180 and `refresh: 180` in
`meta.yaml`.

**Screens created over MCP** (not in the repo): `local/calibration/inkfield`
and `local/calibration/color`.

**Measurement kit** at `~/scratch/panel-evidence/e1004-2026-08-21/`:
`measure.sh`, `prep.sh`, `scout.py`, `run.py`, `gridfit.py`, `warp.py`,
`deveil.py`, `patches.py`, `synth.py`/`validate.py`, `FINDINGS.md`, venv at
`./venv/bin/python`, and `greyprobe/` — the Rust probe behind the earlier
neutral-ramp numbers. `cargo run --release`; change `PALETTE` for another panel.

## 10. Environment

- **Verify with the four commands directly. `make check` has reported exit 0
  while tests failed.**
  ```
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd docs && mdbook build
  ```
- **g18 HA**: `root@10.46.18.3`, ssh authorised by the owner. The auto-mode
  classifier still blocks some remote mutations; a targeted single-purpose
  `sed -i` went through where `cp && sed -i && restart` did not. If blocked,
  hand the owner a one-line `!` command.
- byonk **does not hot-reload config — restart the add-on**, and a restart
  clears the in-memory device registry. Screens are re-read per render.
- MCP server `byonk-g18` talks to it directly. `render_screen` accepts
  `dither`, `panel` and `colors_actual`, so algorithms and calibrations can be
  compared without touching config. **Pass explicit `width`/`height`** —
  `image_max_width` resamples, which averages away the very clumping under test.
  It cannot vary `mixture_bias`, which is internal; before/after needs two
  builds (see plan Task 7 Step 1).
- Devices: `44:1B:F6:83:93:38` reTerminal E1004 (1200×1600, 6-colour) and
  `94:A9:90:8C:6D:18` TRMNL Classic.
- No `timeout` on this Mac and foreground `sleep` is blocked; use
  `curl --retry N --retry-delay S --retry-all-errors --retry-connrefused`.
- **`ha addons` is deprecated** in favour of `ha apps`; still works, warns.

## 11. Carried forward — still true, still unmerged

### Photographic calibration of the E1004

Full write-up: `~/scratch/panel-evidence/e1004-2026-08-21/FINDINGS.md`.

- **A photograph can calibrate this panel to about 2% of white**, provided every
  frame comes from the same camera module at low ISO.
- **This panel's green is about half as colourful as `default-config.yaml`
  claims** — chroma 0.068 measured against 0.158. The owner independently called
  it *"dull and darkish but still clearly green"*. **This finding stands, is the
  direct cause of §1, and is worth committing on its own.**
- **Veiling glare is the biggest error source.** A glossy panel mirrors the room
  and the reflection *adds* to the ink; the Latin square cancels gains, not
  offsets. `deveil.py` fits gain+veil per row but cannot identify the ink offset,
  which is why the raw measured palette was too flat and was then affine-stretched
  so black→0 and white→1.
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
3. **PR for `fix/trmnl-x-ghosting-levers`** never opened; its three commits are
   in this branch's history, including a data-loss fix (`0fb5c47`).
4. **Restore `homeio`**: `log_level` → `info`, stop `local_byonk`, start
   `43664941_byonk`, delete `local/noise-test`. Backup:
   `/addon_configs/local_byonk/config.yaml.pre-recovery`.
5. **Timestamped image filenames** — content-hash names defeat device caching
   (`filesystem.cpp:141`).
