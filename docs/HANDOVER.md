# Handover — the panel has two tone curves, and the PNG's byte size picks one

**Date:** 2026-08-21 (late) · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `ce2e3ef`
**Base:** `fix/trmnl-x-ghosting-levers` @ `254705d` (off `main` @ `5c67c62`, protected)

> Supersedes the three earlier 2026-08-21 handovers. **§1 is a reversal of the
> previous §5**: "this panel's ink levels are badly wrong" was the wrong frame.
> The levels are a property of a *firmware grey table*, and which table runs is
> decided by how many bytes the PNG happens to compress to. Read §1 before
> touching calibration. §2 records numbers that are finally trustworthy.

---

## 1. The finding: two grey tables, selected by image size

`src/display.cpp:1903` in the TRMNL firmware:

```c
#define FASTEPD_LARGE_IMAGE_THRESHOLD (100 * 1024)
...
if (data_size > FASTEPD_LARGE_IMAGE_THRESHOLD) {
    bbep.setCustomMatrix(u8_graytable_big, sizeof(u8_graytable_big));   // 38-pass
} else {
    bbep.setCustomMatrix(u8_graytable, sizeof(u8_graytable));           // 9-pass
}
```

Two hand-tuned waveform tables, chosen by the **downloaded PNG's byte count**.
PNG size depends on how compressible the picture is, so **the panel's tone
response flips silently with screen content**: flat cells compress small and get
the 9-pass table, a dithered photo does not and gets the 38-pass one.

That is what produced every confusing observation in the earlier handovers. Ink
Field rendered to **4285 bytes** (9-pass, smooth ramp); Gradient Lab to
**~146 400 bytes** (38-pass, huge jumps). Same panel, same session, same
lighting — different tables.

**Proved by A/B, not inferred.** `min_png_bytes: 102401` padded the *same* Ink
Field screen past the threshold (byonk log: `size_bytes=102401`), and the jumps
appeared on a screen that had just looked clean. The owner also saw the longer
build-up on the glass. Note the A/B needs a **content change** to force a
re-download — byonk names images by the *SVG's* hash, so padding alone leaves
the hash identical and the device keeps its cached copy. `params: {offset: 8}`
on Ink Field does that and doubles as a position-independence check.

**Consequence for byonk:** `colors_actual` is not a property of the panel. It is
a property of (panel, grey table). A single list cannot be right for both. Pin
the table with `min_png_bytes` if calibration is to mean anything.

---

## 2. Measured ink levels — both tables

Method: Ink Field photographed square-on (iPhone ProRAW), `dcraw -4 -T -o 0
-r 1 1 1 1`, **green plane only** (`ffmpeg -vf extractplanes=g`, no colour
matrix, no range remap). Latin-square average per ink, cross-checked against an
ANOVA that fits row and column lighting as free parameters.

**Both frames passed their own quality gate**: the two independent estimators
agreed to **0.27** and **0.22 L\***, residual scatter 5.8% in both. The fitted
lighting field was large — 19% across rows, 22% across columns — and cancelled,
which is exactly what the Latin square is for.

| | contrast (white/black) | steps indistinguishable from zero (2σ) | worst step |
|---|---|---|---|
| **9-pass** | 8.05 : 1 | 2 — inks 0→1, 14→15 | +6.51 L\* |
| **38-pass** | **9.28 : 1** | **4** — inks 0→1, 8→9, 11→12, 13→14 | **+16.20 L\*** |

38-pass L\* (relative to its own white): 39.19, 38.89, 45.69, 50.72, 66.92,
74.44, 77.67, 84.86, 86.60, 86.21, 92.69, 94.46, 95.59, 97.11, 98.47, 100.00.

**So "38-pass is better" is only ~15% more contrast, paid for with twice as many
dead levels**, a 16.2 L\* chasm at 3→4, and the top six levels crushed into
~7 L\*. It is not monotonic within noise (0→1 and 8→9 invert slightly).

**Dead: the old 44:1 figure and the whole `gradient-lab` calibration.** Gradient
Lab's ramp is spatially ordered, so lighting was inseparable from tone. Ink
Field replaces it. The owner's earlier by-eye observations were also made on
Gradient Lab, so they carry the same contamination — that is why they disagree
with §2 and it is not evidence against the measurement.

**Absolute levels are NOT measured.** Neither frame contained a reference of
known reflectance, so only the ramp's *shape* is known; the existing white
(`#B8B8B0`) was carried over as the anchor. Green channel only, so the values
are neutral by construction — nothing is known about the panel's tint. **Put a
white card in the next frame** and this limit disappears. (The owner's idea of
using the bezel as a reference works too, and would let the two tables be
compared in absolute terms — the contrast ratios above did not need it, being
ratios within a single frame.)

---

## 3. Exact state

**byonk** (`feat/panel-clean-recovery`), this session's commits:
- `485773f` handover: `panel_clean` abandoned, use the firmware's wiper
- `ce2e3ef` **fix: a wipe run sets its own poll cadence** (§4)

Verified at `ce2e3ef`: `cargo fmt`, `clippy -D warnings` clean,
**43 test binaries / 1229 tests / 0 failures**.

**Uncommitted: only the owner's four docs-screenshot files** — `config.yaml`,
`docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`,
`tools/capture-config.yaml`. **Never stage them.** Never `git add -A` here.

**Deployed on homeio** (`local_byonk`, `/addon_configs/local_byonk/config.yaml`),
**none of this is in the repo**:

| key | value | backup taken |
|---|---|---|
| `panels.trmnl_x.colors_actual` | measured 38-pass (below) | `config.yaml.pre-measured-colors` |
| device `min_png_bytes` | `102401` — pins the 38-pass table | `config.yaml.pre-graytable` |
| device `params` | `{offset: 8}` — leftover from Ink Field, clear it | — |
| device `screen` | `local/gradient-lab` | — |

```
colors_actual: "#404040,#414141,#4C4C4C,#555555,#747474,#838383,#898989,#989898,
                #9B9B9B,#9C9C9C,#A8A8A8,#ACACAC,#AEAEAE,#B1B1B1,#B4B4B4,#B7B7B7"
```

Renders correctly (`measured_source: panel.colors_actual`). **The device had not
yet polled it when this was written** — it sleeps on `refresh_rate=3600` and
needs a middle-pad tap. Judging it: bars **B and D** (marked, continuous tone)
are the test — they go through gamut mapping and should smooth out across the
old 3→4 gap. The **ink ramp is the control**: it is sent as literal palette
indices and must look unchanged.

**Measurement kit preserved** at `~/scratch/panel-evidence/inkfield-2026-08-21/`
— both source DNGs, `run.py` (fit + measure + cross-check), `build_colors.py`,
`colors.py`, `validate.py`, plus the extracted per-cell arrays. Needs numpy;
the session venv is gone, so `python3 -m venv venv && venv/bin/pip install numpy`.
Usage: `run.py <green.raw> <ink offset> <tag> [x0 x1 y0 y1]`.

**Firmware / flash** unchanged from the last handover: device runs the 1.8.14
control build, full flash backup at
`~/scratch/panel-evidence/flash-backup-2026-08-21.bin`, `feat/panel-clean` @
`30f9ae0` abandoned.

---

## 4. What shipped in `ce2e3ef`, and why

byonk served the **screen's** `refresh_rate` during a recovery run. The poll that
matters is the firmware's follow-up after a wipe: it is answered with content, so
the device then slept for however long the *content* takes to go stale. With Ink
Field's `refresh: 3600` a 10-wipe run became a **10-hour** one — observed live,
one wipe ran and the run sat idle.

A run now serves `RECOVERY_REFRESH_RATE_SECS = 5`. That is the firmware's own
fast-poll interval (`RefreshInterval::fastPollSeconds`), and `applyServerRate()`
stores whatever the server sends **without clamping**, so it is a cadence the
device already uses.

The subtle part, and the reason for a comment in the code: the check asks the
registry **again** rather than reusing `on_poll`'s result. They differ exactly
where it matters — the follow-up poll returns `None` while the run is still
going. And because `on_poll` drops the session as the final wipe goes out, the
screen's own rate returns by itself.

Also fixed two pinned builtin-screen counts left at 5 by `c880e32`, which had
the suite red (`tests/builtin_package.rs`, `tests/screen_schemas_test.rs`).

---

## 5. Open byonk defects found today

1. **The tone response flips silently with content size.** Nothing in byonk
   knows about `FASTEPD_LARGE_IMAGE_THRESHOLD`. A user gets one grey table for a
   text screen and another for a photo, with no way to tell. At minimum byonk
   should warn when a `trmnl_x` render lands near 100 KiB; arguably
   `min_png_bytes` should default on for that panel.
2. **The palette cannot say "these two inks look identical."**
   `EinkPalette::new` rejects duplicates (`palette error: duplicate color found
   at index 9`) — it was deployed and *did* break the render until the pair was
   nudged apart by one 8-bit step. The panel really does have two dead pairs.
3. **The palette is positional, so usable levels cannot be declared.** The
   owner's idea — declare only the 14 inks that do something — does not work
   today: `map_grey_indices` (`src/rendering/svg_to_png.rs`) derives the
   transmitted grey from an entry's **index**, spreading the list evenly over
   the output range. Dropping two of sixteen would skip levels **4 and 11**, not
   the dead 1 and 9, and shift everything above the first gap.
   **Proposed fix: derive the level from the entry's hex value instead.** The hex
   already carries it (`#111111` *is* level 1), and for every palette byonk ships
   the two agree exactly — so it is backwards compatible and only differs when a
   list has gaps, which is the wanted capability. Note `EinkPalette::new` and
   `resolve_measured_colors` both require `colors` and `colors_actual` to be the
   same length, so both lists must shorten together.

---

## 6. Traps

**Do not trust a grid fit that has not proved itself.** `run.py` prints two
independent estimates and their disagreement; **0.2–0.3 L\* is a good fit, and
anything above ~1 L\* means the grid is misaligned, not that the panel is
strange.** A bad fit produced a confident, monotone-looking, completely wrong
table (49 L\* disagreement, 38% residuals, L\* above 100). The gate caught it.

**The 17 grid lines are evenly spaced, so the fit aliases by whole cells.**
Nothing pins the phase except the panel border. Weighting the two outer lines
helps but can latch onto the bezel's *outer* edge. A coarse bbox hint read off
the brightness profile is the reliable route — the bezel is far brighter
(~10000–13000) than any ink (≤7458). A full auto-detector is still unbuilt; it
is the "panel auto-calibrator" idea and it is harder than it looks.

**Tapping the touchbar forces a server fetch** — this is the fastest way to make
the device poll. Not because the buttons fetch, but because `src/bl.cpp:975`
takes any **non-timer** wake as a reason to show the logo, clear the displayed
image and un-register, which forces a refetch. Use the **middle** pad: left and
right are Back/Next and call `show_cached_image_by_offset()`, which paints a
*cached* image and sleeps.

**`ha addons` is deprecated** in favour of `ha apps`; it still works but warns.

**homeio's address changed mid-session** and a wedged ssh ControlMaster made
everything hang. `-o ControlMaster=no -o ControlPath=none` diagnoses it. There is
no `timeout` binary on this Mac; `curl --retry N --retry-delay S
--retry-all-errors --retry-connrefused` is the way to wait for a restart, since
foreground `sleep` is blocked.

**byonk does not hot-reload `/config/config.yaml`.** Restart it — and change
config *before* starting a recovery run, since a restart cancels in-memory
sessions.

**Earlier traps still true:** esptool's deps live in the Homebrew venv; a new
PlatformIO env silently gets a default sdkconfig and still reports SUCCESS;
PlatformIO cannot flash this device (drive esptool directly, ~1.5 MB regions);
never `erase_flash`. See `841ed1f` for the full text of those four.

---

## 7. Environment

- Device `1C:DB:D4:66:5B:50`, `/dev/cu.usbmodem101`, firmware 1.8.14 control.
- **homeio**: `root@homeio.oetiker.ch` (ssh is fine to run), add-on `local_byonk`,
  runtime config `/addon_configs/local_byonk/config.yaml`, source `/addons/byonk`.
  Host has `jq`, **no `python3`**. Admin token:
  `ha addons info local_byonk --raw-json | jq -r .data.options.admin_token` —
  keep it in a shell variable, **never print it**.
  Full redeploy recipe unchanged — see `841ed1f` §7.
- Raw workflow: `dcraw -4 -T -o 0 -r 1 1 1 1 x.DNG` for linear 16-bit; prefer
  `ffmpeg -vf extractplanes=g -pix_fmt gray16le` over a luma conversion.
  **iCloud share links deliver JPEG** — export the original from Photos.
- byonk verify: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`. **`make check` has reported exit 0 while tests failed.**

---

## 8. Next

1. **Look at the calibrated Gradient Lab on the glass** (tap the middle pad).
   That is the open question this handover stops mid-way through.
2. **Decide the palette model** — §5.3. The owner favours declaring only the
   usable inks; that needs the hex-derived level change first, test-first.
3. **Re-measure with a white card in frame** to get absolute levels, and with a
   second `offset` to confirm position independence.
4. **Get the calibration into the repo.** It only exists on homeio.
   `default-config.yaml` still ships the interpolated generic curve.
5. **Flush the panel properly before any further measurement** — only one wipe
   of ten ran, so both frames carry some residual ghost from Gradient Lab.

## 9. Still open, unrelated

1. **PR for `fix/trmnl-x-ghosting-levers`** never opened; its three commits are
   in this branch's history, including a data-loss fix (`0fb5c47`).
2. **Restore `homeio`**: `log_level` → `info`, stop `local_byonk` and start
   `43664941_byonk` (currently `error`), delete `local/noise-test`. Pre-session
   config backup: `/addon_configs/local_byonk/config.yaml.pre-recovery`.
3. **Timestamped image filenames** — content-hash names defeat device caching
   (`filesystem.cpp:141`). Bit us today: an unchanged hash means the device will
   not re-download even when the served bytes change.
4. **SDD ledger** (git-ignored):
   `.superpowers/sdd/2026-08-21-panel-clean-recovery/progress.md`
