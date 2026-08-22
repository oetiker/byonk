# Handover — photographic colour calibration of the reTerminal E1004

**Date:** 2026-08-22 (early) · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `814d061`
**Base:** `fix/trmnl-x-ghosting-levers` @ `254705d` (off `main` @ `5c67c62`, protected)

> **New initiative. Nothing was committed this session.** The work moved to a
> different panel (reTerminal E1004, 6-colour) on a **different Home Assistant
> instance** (`root@10.46.18.3`), and it is all deployed-only. The previous
> TRMNL X grey-table findings are still true and still unmerged — see §8.
>
> Read §1 and §5 before touching this. §5 is the live open question and it is
> **not** a calibration problem.

---

## 1. What was established

**A photograph can calibrate this panel, to about 2% of white** — provided every
frame comes from the same camera module at low ISO. Two shots from different
angles, with different reflections, agreed to **2.02% of white** on every ink
and channel.

The full write-up, with numbers, lives at
`~/scratch/panel-evidence/e1004-2026-08-21/FINDINGS.md`. Read it before
re-deriving anything.

**The single durable result:** this panel's **green is about half as colourful
as `default-config.yaml` claims**. Chroma 0.068 measured (both photos agree to
3%) against 0.158 in the config. The owner independently described the panel's
green as *"dull and darkish but still clearly green"*. Every other ink comes back
near the config once the veil is removed. **This is the finding worth keeping**
regardless of what happens to the rest of the numbers.

**The config was badly wrong in an obvious way.** It claimed black `#000000` and
white `#FFFFFF` — the extremes — for a panel whose black is ~4% reflectance and
whose white is about half the picture frame's brightness.

---

## 2. Method, and the four things that fought it

Ink Field (Latin square) photographed in ProRAW, `dcraw -4 -T -o 1` with the
white balance taken off the frame border via `-A` (camera space, *before* the
colour matrix — the only correct order), then measured through a homography.

Validated end to end: `synth.py` plants six known colours, wrecks the picture
with a 22% row gradient, 19% column gradient, 13% vignette, 1.2% noise and a 16%
veil, and `run.py` recovers the ratios to **0.38% of white**. A wrong number from
a real photo is therefore the photo or the panel, not the code.

1. **Veiling glare — the biggest error by far.** A glossy panel mirrors the room
   and the reflection is **added** to the ink. The Latin square cancels *gains*,
   not offsets, so it cannot help. Photo 1 carried 16% of white in its top row.
   Uncorrected it read black as 12.9% of white instead of 6.7% — the panel looked
   half as good as it is. `deveil.py` fits gain+veil per row (R² ≥ 0.99).
   **It has an irreducible limit**: `ink + d` and `veil − gain·d` fit identically,
   so the ink offset is not identifiable. It is anchored by assuming the least
   veiled 15% of rows have none, which means **any veil present everywhere
   survives**, and a veil desaturates. That is why the first measured palette was
   too flat.
2. **Camera module.** See §6.1. This one silently destroys a frame set.
3. **Mixed illuminant.** Photo 1's border ran B/G 1.09 at the top to 1.02 at the
   bottom — daylight above, warm light below. No single white balance is right
   for such a frame.
4. **Geometry — mattered less than expected.** Both panels were rotated in-plane
   (0.47° and 1.25°) with ~2% keystone. `warp.py` finds the four corners from the
   panel outline and samples through a homography; it moved the answer by ~1%,
   well under the veil and illuminant errors. **The originally-planned fiducial
   marks are therefore not the bottleneck** — the reflection and the light are.

---

## 3. The stretch, and why the deployed numbers are not the measured ones

The measured palette was too desaturated (residual uniform veil), and the visible
symptom was **green speckle in neutral ramps**: measured green had chroma 0.032
against measured *black's* 0.025 — green was effectively a neutral, so the
ditherer used it for greys.

The owner proposed stretching so black→0 and white→1. **This is principled, not a
fudge**: a uniform veil is an additive constant in linear light, and subtracting
the measured black removes exactly that. It keeps what was measured well (where
the four colours sit *relative to* black and white) and discards what the veil
corrupted (absolute black, overall contrast). Cost: it asserts a perfect black
and white, so the *preview* now overstates the panel.

Then, empirically, from the glass:

| observation | change made |
|---|---|
| "colours too bright → image too dark" | white `#FFFFFF`→`#DCDCDC`, yellow `#FFED00`→`#E1CE00` |
| "dark green band in the grey ramp — green too light" | green L\* 0.522 → 0.412 (`#3F7663`→`#1E5645`) |

**Why overstating an ink's brightness makes the picture darker:** byonk concludes
each such pixel goes a long way and lays down fewer of them.

**Currently deployed:** `#000000,#DCDCDC,#B52200,#E1CE00,#2C6CBC,#1E5645`

---

## 4. Exact state — all of it deployed-only

**Repo: nothing committed, nothing added.** Working tree still has only the
owner's four docs-screenshot files (`config.yaml`, `docs/generate-samples.sh`,
`docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`).
**Never stage them. Never `git add -A` here.**

**On `root@10.46.18.3`**, add-on `43664941_byonk`, config
`/addon_configs/43664941_byonk/config.yaml`:

| what | value | restore |
|---|---|---|
| `panels.reterminal_e1004.colors_actual` | `#000000,#DCDCDC,#B52200,#E1CE00,#2C6CBC,#1E5645` | `config.yaml.pre-measured-2026-08-22` (original), `config.yaml.pre-stretch-2026-08-22` |
| device `44:1B:F6:83:93:38` screen | `local/calibration/color` | **was `examples/gphoto`** |
| device `dither` | `atkinson` | — see §6.2 |

**Screens created over MCP** (not in the repo):
- `local/calibration/inkfield` — Ink Field copied from this branch, because the
  released byonk on that host has no such builtin. Pure Lua+SVG, needs no deploy.
- `local/calibration/color` — fork of the Color Calibrator with
  **`refresh_rate` 3600 → 180** in `script.lua` *and* `refresh: 180` in
  `meta.yaml`, so iterating does not cost an hour a round (§6.3).

**Measurement kit** at `~/scratch/panel-evidence/e1004-2026-08-21/`:
`measure.sh <DNG> <tag>` runs the whole chain. Also `prep.sh` (DNG→linear RGB),
`scout.py` (find the panel), `run.py` (measure), `gridfit.py`, `warp.py`,
`deveil.py`, `patches.py` (measure the Color Calibrator's six ink patches),
`synth.py`/`validate.py` (known-truth check), and `FINDINGS.md`. Needs its venv:
`./venv/bin/python`. Four source DNGs are in `~/Downloads/IMG_2719..2722.DNG`.

---

## 5. THE LIVE QUESTION — and it is not a calibration problem

The owner asked: *why does the ditherer pick green for a mid-grey instead of
dithering black and white, which covers greys perfectly and adds no colour?*

**Answer, from `crates/eink-dither/src/palette/palette.rs:365`** — it is
deliberate. `for_error_diffusion()` downgrades HyAB to plain Euclidean OKLab, and
the comment says grey pixels matching dark chromatic entries "is expected and
desirable". Error diffusion is **greedy per pixel**: for a mid-grey at L\*≈0.5 the
distances are green **0.110**, white 0.39, black 0.50. Green wins every pixel.

**This panel has no mid-grey ink** — only black (L\* 0) and white (L\* ≈0.89).
Red, blue and green all sit near L\*≈0.50, and green has the lowest chroma of the
three. So *some* green in a neutral ramp is unavoidable here and no value of
`colors_actual` removes it.

**The owner's criticism is nonetheless correct.** For a neutral target, black+white
is exactly correct and exactly neutral, needing no compensation; the green route
is only correct after diffusion compensates nearby, and that compensation is
imperfect locally. The eye objects far more to low-frequency chromatic clumps
than to fine luminance noise, so the algorithm optimises the wrong thing.

Three routes, **untested — this is where to resume**:

1. **Switch the device to `sierra-light`.** The panel config already carries
   `dither: {sierra-light: {error_clamp: 0.11, noise_scale: 5}}` and the device
   uses `atkinson`, so **that tuning is currently inert** (§6.2). `error_clamp` is
   exactly what stops error diffusion chasing an out-of-gamut target — the likely
   cause of green contaminating the *yellow* ramp, where the target `#FFFF00` is
   far outside gamut. Try this first; it is one config key.
2. **The blue-noise / graphics path**, which keeps `HyAB` and its chroma-coupling
   penalty (kchroma=10 exists precisely to stop chromatic inks capturing greys).
   Correct-by-design for graphics-like content such as ramps.
3. **Fix it in byonk**: apply the chroma penalty to the *first* match while
   leaving the diffused error unbiased, so greys prefer neutrals without
   distorting chromatic averages. This is the one actually worth having.

---

## 6. byonk defects found this session

1. **Nothing warns when a calibration frame set mixes camera modules.** Not a
   byonk bug but the trap that cost the most: photo 2 was
   `iPhone18,2 back telephoto camera` at ISO 500 while 1 and 3 were
   `back camera` at ISO 64. Its blue channel was an outlier by up to 9.2% of
   white. The phone switches lenses on its own and **nothing in the picture shows
   it**. Always check `exiftool -s -UniqueCameraModel -ISO -FocalLength`.
2. **A panel's tuned dither parameters silently do nothing when the device picks
   a different algorithm.** `dither:` in the panel config is keyed by algorithm
   name; a device-level `dither: atkinson` bypasses a `sierra-light` block with no
   warning. Should warn, or the keying should be reconsidered.
3. **A screen's `refresh_rate` overrides the device's `refresh`.** The device is
   configured for 300 s; the Color Calibrator returns 3600, and the panel slept
   for an hour. Same class as the recovery-cadence bug fixed in `ce2e3ef` on this
   branch. The device setting should win, or cap the screen's value.
4. **`colors_actual` serves two masters.** It must make the preview honest *and*
   tell the ditherer which inks to pick. A veil-contaminated measurement is bad
   for both; the stretched value is good for the ditherer and dishonest for the
   preview. Worth a design decision — possibly two fields.

Still open from the TRMNL X work: the palette rejects duplicate colours, and
`map_grey_indices` derives the level from an entry's **index** rather than its
hex, so "declare only the usable inks" does not work. Full text in `814d061` §5.

---

## 7. Environment

- **New HA**: `root@10.46.18.3` (ssh authorised by the owner), add-on
  `43664941_byonk`, config `/addon_configs/43664941_byonk/config.yaml`, screens
  under `/addon_configs/43664941_byonk/screens/`. **byonk does not hot-reload
  config — restart the add-on**, and a restart clears the in-memory device
  registry. MCP server `byonk-g18` talks to it directly.
- Devices there: `44:1B:F6:83:93:38` reTerminal E1004 (1200×1600, 6-colour) and
  `94:A9:90:8C:6D:18` TRMNL Classic.
- **Raw workflow**: `dcraw -4 -T -o 1 -A <x> <y> <w> <h>` — `-o 1` for sRGB
  primaries while `-4` keeps it linear, `-A` white-balances off the frame border.
  **iCloud share links deliver JPEG** — export the original from Photos.
- No `timeout` on this Mac and foreground `sleep` is blocked; use
  `curl --retry N --retry-delay S --retry-all-errors --retry-connrefused` to wait
  for a restart.
- **`ha addons` is deprecated** in favour of `ha apps`; still works, warns.
- byonk verify: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`. **`make check` has reported exit 0 while tests failed.**

---

## 8. Next

1. **§5.1 — switch the device to `sierra-light`** and look at the yellow ramp.
   One config key, and the most likely win.
2. **Re-shoot with a black trap in frame** (black velvet, or a deep matte-black
   box). Whatever it measures *is* the veil, so subtract it. This is the one
   thing that would end the guessing about green's true chroma, and it also fixes
   the absolute anchor, which currently assumes the frame border is a perfect 1.0.
3. **Prove position independence** — every frame so far used `offset: 0`. Set
   `params: {offset: 3}` on Ink Field and re-measure. Still not done.
4. **Get something into the repo.** Nothing from this session is committed.
   The green finding (§1) is defensible on its own; the rest is tuned by eye.
5. **Restore the device to `examples/gphoto`** when finished.

## 9. Carried forward — the TRMNL X work, still unmerged

Unchanged and still true; full text in `814d061`:

1. **Two firmware grey tables selected by PNG byte size**
   (`FASTEPD_LARGE_IMAGE_THRESHOLD`, 100 KiB). `colors_actual` is a property of
   *(panel, grey table)*, not of the panel. Pin with `min_png_bytes`.
2. **Measured both tables** — 9-pass 8.05:1 with 2 dead steps; 38-pass 9.28:1
   with 4 dead steps and a 16.2 L\* chasm. Deployed on **homeio**, not in the repo.
3. **PR for `fix/trmnl-x-ghosting-levers`** never opened; its three commits are in
   this branch's history, including a data-loss fix (`0fb5c47`).
4. **Restore `homeio`**: `log_level` → `info`, stop `local_byonk`, start
   `43664941_byonk`, delete `local/noise-test`. Backup:
   `/addon_configs/local_byonk/config.yaml.pre-recovery`.
5. **Timestamped image filenames** — content-hash names defeat device caching
   (`filesystem.cpp:141`); an unchanged hash means no re-download even when the
   served bytes change.
6. **SDD ledger** (git-ignored):
   `.superpowers/sdd/2026-08-21-panel-clean-recovery/progress.md`
