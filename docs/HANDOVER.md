# Handover — the ditherer's core assumption is false, and that explains everything else

**Date:** 2026-08-23 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `32152e7`
**Base:** `main` @ `5c67c62` (v0.19.0, protected). `main` is an ancestor; no rebase needed.

> **`cargo test --workspace` FAILS ON PURPOSE.** Exactly one test is red:
> `test_neutral_grey_has_no_dominant_chromatic_ink`
> (`crates/eink-dither/src/domain_tests.rs`). It encodes a premise that has now
> been disproved — **delete it** (see §2). Any *other* failure is a regression.

## Resume here

Nothing is half-finished. The panel is fixed and running on the shipped add-on.
The open work is a **new finding** that invalidates an assumption underneath the
whole dither pipeline. Read §1 first.

1. **§1 — state-dependent dot gain.** The new, measured, load-bearing finding.
2. **§2 — the mixture-aware quantiser is dead**, and the real root cause.
3. **§3 — what is applied and live** on the panel right now.
4. **§4 — method rules.** Four expensive mistakes were made today. Do not repeat.
5. **§5 — exact state.** Repo, box, and the measurement kit.
6. **§6 — what to do next.**

---

## 1. The finding: ink coverage depends on what surrounds it

**A pixel does not deliver its nominal colour. What it delivers depends on its
neighbours, by a factor of about 2.5.**

Measured on the real E1004 with a purpose-built pattern in which **every cell
holds exactly 25% ink coverage** and only the surroundings change:

| condition | 1px | 2px | 3px | 4px | 6px | 8px | **mean** |
|---|---|---|---|---|---|---|---|
| RED on white | 0.351 | 0.348 | 0.346 | 0.343 | 0.336 | 0.295 | **0.336** |
| RED on black | 0.162 | 0.130 | 0.108 | 0.104 | 0.118 | 0.171 | **0.132** |
| BLUE on white | 0.355 | 0.348 | 0.353 | 0.342 | 0.351 | 0.311 | **0.343** |
| BLUE on black | 0.177 | 0.126 | 0.109 | 0.100 | 0.117 | 0.154 | **0.131** |
| GREEN on white | 0.338 | 0.352 | 0.356 | 0.358 | 0.333 | 0.302 | **0.340** |
| GREEN on black | 0.274 | 0.105 | 0.051 | 0.040 | 0.084 | 0.280 | **0.139** |
| YELLOW on white | 0.212 | 0.252 | 0.311 | 0.273 | 0.235 | 0.212 | 0.249 |
| YELLOW on black | 0.204 | 0.162 | 0.150 | 0.152 | 0.183 | 0.241 | **0.182** |

Nominal is `0.2500` in every cell. **On white the ink over-delivers by ~36%; on
black it under-delivers by ~46%.**

**Mechanism — one rule covers both directions: the darker state expands into the
lighter one.** On white the coloured dot is darker, so it grows past its
electrode. On black the *background* is darker, so it eats the coloured dots.
Yellow is the lightest ink, is hurt worst on black, and is the only ink that
behaves near-nominally on white.

### What is NOT established

- **The cluster-size trend within a row.** Much weaker than the background
  effect. The U-shape on black (green `0.274 → 0.040 → 0.280`) is most likely
  veiling glare inflating the darkest cells, not physics.
- **Yellow on white** is unreliable — variants A and B disagree by up to 0.25
  there, because yellow and white are too close for the endpoint solve to be
  well-conditioned. Fix by giving yellow its own high-contrast background.

### Why this result is trustworthy when two earlier ones were not

- **Lighting.** The frame was 18.5% brighter on the right, and cluster size ran
  left→right. The pattern is therefore rendered twice, variant **B** with the
  columns reversed; the reported numbers are the **mean of A and B**, which
  cancels any left-right gradient exactly.
- **Registration.** Sampling windows are certified: measuring at window 0.40 and
  0.60 must agree. Got `0.0276` (A) and `0.0262` (B) against a 0.03 threshold.
- **Background effect is gradient-immune anyway** — the on-white and on-black
  rows occupy the *same columns*, so the difference between them cannot be a
  lighting artifact.

### Why it matters

1. **Error diffusion assumes a pixel contributes its nominal colour regardless
   of neighbours.** That is wrong by 2.5×. The ditherer does careful arithmetic
   on a false premise.
2. **A solid-patch calibration structurally cannot predict dithered output.** A
   solid patch has no neighbours in a different state and therefore no
   spreading. This is why the "correct" photographic green made real images look
   worse than the owner's hand-tuned value (§2).
3. It predicts dark images lose saturation and light images gain it.

## 2. The mixture-aware quantiser is dead — and the real cause was a constant

**Drop it.** The initiative was built to fix *"a flat mid grey renders as 87%
green ink"*. That was never a quantiser defect. It was the ditherer **correctly
following a wrong `colors_actual` green**.

`colors_actual` steers the ditherer's ink choice, not just the preview (defect
7). Green ink on the calibration photo's face, Floyd–Steinberg:

| `colors_actual` green | black | red | **green** |
|---|---|---|---|
| `#1E5645` — the 2026-08-22 photographic measurement | 61.2% | 14.0% | **17.8%** |
| `#00994D` — the value checked into `default-config.yaml` | 68.2% | 17.7% | **8.6%** |

Told green is nearly neutral (`#1E5645`, chroma 0.066), the quantiser picks it
for every dark neutral and paints skin shadows green. Told green is a real
colour (`#00994D`, chroma 0.158), it reserves green for green things. **The
owner's panel went from "extreme green tint" to "much better" on that one line.**

> **Do NOT replace `default-config.yaml`'s `#00994D` with the photographic
> measurement.** It is a validated correction, not a hack. The measurement is
> too dull because veiling glare adds an offset the Latin square cannot cancel
> and `deveil.py` cannot identify — which flattens the darkest inks most, and
> green is the darkest chromatic ink. §1 explains why a solid-patch measurement
> could never have been right for dithered output anyway.

**Re-tested honestly under Floyd–Steinberg**, the quantiser is harmless at
`mixture_bias ≤ 0.25` and useless: the green bias it targets is already
`+0.0012` with the feature off. At 0.5 it posterises — hue sweeps collapse into
flat blocks with hard walls, shadows crush to black. The owner saw this on the
panel and in the renders.

**What to keep:** the wedge fan is currently inert (`dither/mod.rs:356` builds it
only when `mixture_bias > 0.0`, and nothing sets that). ~950 lines with no
consumer. Owner's call: remove / keep gated / park behind a tag. **Recommendation:
remove** — the premise failed twice and the real defect was a constant.

**Also delete** `test_neutral_grey_has_no_dominant_chromatic_ink`. It encodes the
disproved premise.

## 3. What is applied and live

Both changes are on the box in **both** `config.yaml` files
(`/addon_configs/local_byonk/` and `/addon_configs/43664941_byonk/`), so they
survive whichever add-on runs. Backups: `config.yaml.pre-kernel-2026-08-23` and
`config.yaml.pre-green-2026-08-23`.

| | before | now |
|---|---|---|
| device `44:1B:F6:83:93:38` kernel | `atkinson-hybrid` | **`floyd-steinberg`** |
| panel `reterminal_e1004` green | `#1E5645` (overnight) | **`#00994D`** (restored) |

### The kernel evidence

Nine kernels, same photo, measured in `colors_actual`, merged in linear light:

| kernel | colour err | green bias |
|---|---|---|
| sierra-lite / floyd-steinberg | 0.0131 / 0.0133 | **+0.0012** |
| burkes / sierra-two-row / stucki / sierra / jjn | 0.0143–0.0155 | +0.0012 |
| atkinson-hybrid | 0.0193 | **+0.0085** |
| atkinson | 0.0226 | +0.0068 |

**Every full-error kernel has 5–7× less green bias than either Atkinson.**
Atkinson propagates 6/8 and discards 25% of its error by construction
(`kernel.rs:56`), which biases it toward the ink nearest the neutral axis.
Atkinson is fine for greyscale; it is the wrong kernel for colour.

**Caveat: this ranking was measured with the too-dull green.** Floyd–Steinberg
beating Atkinson is structural and safe. The ordering among the seven good
kernels should be re-run on the corrected palette before it is treated as
settled.

`floyd-steinberg` was chosen over the marginally better `sierra-lite` because
`sierra-light` is a **deliberate alias** for `sierra-lite` (`config.rs:214`), so
the panel's existing `sierra-light:` block is *armed*, not dead — selecting
sierra-lite would activate `noise_scale: 5` (defect 4 says 2.5) and a stale
`error_clamp: 0.11`.

**`max_error` is noise on photographs** (0.0131 vs 0.0133 at 2.0) and has *zero*
effect under Atkinson — those renders were byte-identical. The previous
handover's §2.2 claim that 2.0 matters was measured on a synthetic census and
does not transfer.

### Nothing from this branch is needed

Verified by byte comparison: the published `0.19.0` add-on with
`floyd-steinberg` produces a **byte-identical** PNG to the branch build. Both
242,481 bytes. The box now runs the shipped add-on.

## 4. Method rules — four mistakes were made today, each one inverted a conclusion

1. **Judge renders in `colors_actual`, never the nominal palette.** The device is
   *sent* the nominal palette, so `/api/image/*.png` comes back nominal and looks
   nothing like the panel. Nominal green is `#00FF00`; the real ink is a dull
   dark teal. Heavy green usage looks catastrophic in nominal and nearly neutral
   in real. This inverted the quantiser verdict — reported as a 16× improvement,
   actually 2× worse. The renders are indexed PNGs, so **swap the PLTE
   index-parallel `colors` → `colors_actual`; no re-render needed.**
2. **Merge dots by averaging in linear light.** An sRGB resize invents a green
   cast. Sibling of the same error; cost the previous session a false finding.
3. **Validate registration before believing any photo measurement.** Two tables
   were reported and then withdrawn: one confounded by an 18.5% lighting
   gradient, one by sampling windows that had walked off the cells. **Overlay the
   sampling boxes on the image and look**, and run the window-size self-check.
   A global panel-rectangle fit is *not* accurate enough — a few percent of scale
   error accumulates across six columns. `locate_grid.py` locks onto the
   pattern's own periodic structure instead.
4. **Design the confound out rather than correcting for it.** The reversed-column
   variant B cancels the lighting gradient exactly; the attempted arithmetic
   correction using the 25px white gaps produced nonsense (yellow at 0.012).

## 5. Exact state

### Repo — nothing committed today

HEAD `32152e7`, unchanged. `git status` shows seven modified files:

| file | whose | what |
|---|---|---|
| `src/rendering/svg_to_png.rs` | **mine — DO NOT COMMIT** | three experiment hooks, marked `EXPERIMENT SCAFFOLDING` at lines 167, 249, 281 |
| `crates/eink-dither/src/domain_tests.rs` | mine, +299/−34 | recipe-agreement rule; bound left at `f32::INFINITY` |
| `crates/eink-dither/src/gamut/mod.rs` | mine, +36 | `panel_e1004()` in `test_support` |
| `config.yaml` | **owner** | never stage |
| `docs/generate-samples.sh` | **owner** | never stage |
| `docs/src/concepts/content-pipeline.md` | **owner** | never stage — still documents pre-0.18.0 `error_clamp` at line 261; **the owner fixes that line** |
| `tools/capture-config.yaml` | **owner** | never stage |

**Never `git add -A` or `git add .`.** Add by explicit path; check
`git diff --cached --name-only` before every commit.

**The scaffolding** reads three files from the add-on config dir each render, so
a sweep needs no rebuild and no restart:

```
/config/mixture_bias      float, 0.0 = feature off (shipped behaviour)
/config/dither_override   kernel name, empty/absent = use device config
/config/max_error         float, applied last so it beats the tuning chain
```

It is genuinely useful for experiments and genuinely unshippable as written. If
kept, it must become real config plumbing (see §6).

### The box — `root@10.46.18.3`, ssh authorised by the owner

| | |
|---|---|
| `43664941_byonk` | **0.19.0, started** — this is the one serving |
| `local_byonk` | 0.19.0-mix2, stopped. Built from this branch + scaffolding. Rebuild recipe below. |
| device `44:1B:F6:83:93:38` | `floyd-steinberg`, panel `reterminal_e1004`, screen **`local/calibration/dotgain2b`** |
| device `94:A9:90:8C:6D:18` | TRMNL Classic, `jarvis-judice-ninke`, unchanged |

**Restore the device to `examples/gphoto` when the dot-gain work is done.**

**Screens created today** (in `/addon_configs/43664941_byonk/screens/calibration/`,
handle `local`, not in the repo): `dotgain`, `dotgain2a`, `dotgain2b`.

The dot-gain screens are a **pre-built indexed PNG placed 1:1**. Because every
source pixel is already exactly a palette entry, the ditherer's nearest match
returns it unchanged with zero error, so the pattern reaches the panel
pixel-exact. **Verified: all 48 cells still at exactly 0.2500 after the full
Lua → template → SVG → resvg → dither → PNG path.** That property is what makes
the screen measure the panel instead of the renderer — preserve it.

**Building `local_byonk` from source** (needed only to change the scaffolding):
scaffold lives in `/addons/byonk` (config.yaml with `image:` removed, plus a
Dockerfile on `rust:1.97-slim-bookworm` — Debian, not Alpine, because
`utoipa-swagger-ui` downloads with `curl` at build time). Ship the build inputs
by `tar | ssh`, **strip macOS `._*` AppleDouble files** (931 of them got embedded
by rust-embed from `fonts/` and `screens/` on the first attempt), bump
`version:` in the scaffold's `config.yaml` (the `ARG BUILD_VERSION` cache-bust
depends on it), then `ha store reload` → `ha addons update local_byonk`. On this
HAOS a bare `store reload` *did* pick up the version bump; no supervisor restart
needed.

### Measurement kit — `~/scratch/panel-evidence/dotgain-2026-08-23/`

| | |
|---|---|
| `scripts/find_panel.py` | panel rectangle from the bezel/panel luminance step |
| `scripts/locate_grid.py` | **the good one** — locks onto the cell grid's own periodic structure |
| `scripts/measure2.py` | A+B analysis with the registration self-check. `measure2.py <linA.tiff> <linB.tiff>` |
| `scripts/make_dotgain2.py` | regenerates both pattern variants; verifies exact 25% coverage |
| `patterns/` | `dotgain2a.png`, `dotgain2b.png` |
| `photos/` | `IMG_2729` (macro), `IMG_2733` (v1), `IMG_2734` (v2 A), `IMG_2735` (v2 B) |
| `renders/` | the nine-kernel sweep and the bias sweep |

Raw workflow: `dcraw -4 -T -o 1 -w -q 3 -b N`. **`-4` is linear — pick `N` per
frame so nothing clips** (A needed `-b 8`, B needed `-b 1`); check
`h[255]+h[511]+h[767]`. Python has PIL but **no numpy and no rawpy**.

The older kit at `~/scratch/panel-evidence/e1004-2026-08-21/` still holds
`FINDINGS.md`, the photographic calibration and the `greyprobe/` Rust probe.

## 6. What to do next

**In rough order of value.**

1. **Pin down the dot-gain law properly.** The background effect is solid; the
   cluster-size trend is not. Improve the pattern:
   - give **yellow a high-contrast background** so its solve conditions well;
   - add **mid-grey backgrounds**, not just black and white, to get the shape of
     the law rather than two endpoints;
   - sweep **coverage** (12.5% / 25% / 50%), since a spreading model needs more
     than one coverage to fit;
   - add **corner fiducials** so registration is trivial instead of inferred;
   - kill veiling glare — shoot in a dark room with a single diffuse source off
     axis. It is the prime suspect for the U-shape on black.
2. **Decide what byonk does with it.** A state-dependent ink model is a real
   change to the ditherer: the error term for a pixel should depend on its
   committed neighbours. Cheapest useful version is probably a per-ink
   "effective coverage" correction applied to the error, not a new quantiser.
   **Brainstorm before building** — the last initiative that skipped that step
   cost ~950 lines and was chasing a bad constant.
3. **Close out the branch.** Remove the wedge fan (owner's ruling pending),
   delete the red test, strip the scaffolding from `svg_to_png.rs`, keep
   `panel_e1004()`. Then `cargo test --workspace` should be green.
4. **Re-run the kernel ranking on the corrected green** to confirm
   floyd-steinberg over the other six.
5. **Promote the scaffolding to real config** if the experiment hooks stay
   useful: `mixture_bias` is gone with the fan, but a `dither` override per
   render and a working `max_error` are legitimate. Note `DitherTuningValues`
   plumbing touches ~25 sites (`config.rs`, `display.rs`, `dev.rs`, `main.rs`,
   `svg_to_png.rs`).
6. **Restore the g18 device to `examples/gphoto`.**
7. **Open the PR.** This branch carries three fixes from an earlier session, the
   quantiser initiative, and the TRMNL X ghosting fixes that were never PR'd
   (`0fb5c47` is a data-loss fix). Consider splitting.

## 7. byonk defects — carried forward, still open

1. **The dev UI duplicates `DitherAlgorithm::defaults()`** (`static/dev/dev.js`,
   `DITHER_DEFAULTS`). It should fetch defaults from the server.
2. **`docs/src/concepts/content-pipeline.md:261`** — stale `error_clamp` range.
   The owner's file; the owner fixes it.
3. **A panel's tuned dither parameters silently do nothing when the device picks
   a different algorithm.** `dither:` is keyed by algorithm name. Confirmed live
   today: the E1004's `sierra-light` block was completely inert under
   `atkinson-hybrid`, with no warning. `deprecation_warnings()` is the obvious
   place to report from.
4. **`noise_scale: 5`** in the shipped E1002/E1004 blocks; the measured optimum
   for Sierra Lite is 2.5 (`dither/mod.rs:166`).
5. **MCP cannot set a device's dither algorithm.** `assign_screen` takes only
   `mac` and `screen_ref` while `apply_device_patch` (`write.rs:249`) already
   accepts `dither`, `panel`, `refresh`, `colors`, `params`, `name`. Widen it and
   rename `assign_screen` → `configure_device`. **Felt directly today** — every
   kernel change needed ssh and a restart.
6. **Panels have no write path at all** — no `/panels` route in `admin_router()`,
   REST or MCP, in any mode. The whole calibration workflow requires ssh.
7. **`colors_actual` serves two masters** — an honest preview and the ditherer's
   ink choice. §1 and §2 make this urgent: they are not the same number, and a
   solid-patch measurement cannot serve the second. **Split them.**
8. **`sierra-lite` deserves one re-test** with a sane `max_error` and the
   `sierra-light` block cleaned up.

Still open from TRMNL X: the palette rejects duplicate colours, and
`map_grey_indices` derives the level from an entry's **index** rather than its
hex, so "declare only the usable inks" does not work.

## 8. Environment

- **Verify with the four commands directly. `make check` has reported exit 0
  while tests failed.**
  ```
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd docs && mdbook build
  ```
- **`git` needs `--no-pager`** in this harness, or `diff --stat`/`log` can return
  nothing at all and look like a clean tree.
- **The auto-mode classifier blocks some remote mutations inconsistently.** A
  single-purpose `sed -i` over ssh went through on one file and was refused on
  the next, identical one. If blocked, hand the owner a one-line `!` command.
- byonk **does not hot-reload config — restart the add-on**, and a restart clears
  the in-memory render cache, so `/api/image/<hash>.png` 404s until the device
  polls again. Screens *are* re-read per render.
- **`/api/image/<hash>.png` needs no token** and serves the exact PNG the panel
  received. Best way to capture renders without spending context on images. Get
  the hash from the add-on log; **filter by `width=1200`** or you will grab the
  TRMNL Classic's 800×480 image by mistake.
- MCP server `byonk-g18` talks to the box directly. `render_screen` accepts
  `dither`, `panel` and `colors_actual`. **Pass explicit `width`/`height`** —
  `image_max_width` resamples, which destroys the dither pattern.
- **`ha addons` is deprecated** in favour of `ha apps`; still works, warns.
- The box has **`jq` but no `python3`**. The Mac has **PIL but no numpy**.
- No `timeout` on this Mac and foreground `sleep` is blocked; use
  `curl --retry N --retry-delay S --retry-all-errors --retry-connrefused`, or
  loop with `sleep` inside a remote `ssh` command.
- Devices: `44:1B:F6:83:93:38` reTerminal E1004 (1200×1600, 6-colour) and
  `94:A9:90:8C:6D:18` TRMNL Classic.

## 9. Carried forward — still true, still unmerged

### Photographic calibration of the E1004

Full write-up: `~/scratch/panel-evidence/e1004-2026-08-21/FINDINGS.md`.
**Read it together with §1 and §2**, which change how to interpret it: its green
is not usable for the ditherer, and a solid-patch method cannot produce a number
that predicts dithered output.

- **Veiling glare is the biggest error source**, and the ink offset it introduces
  is not identifiable from the Latin square. This is the unsolved problem.
- **Never mix camera modules in a frame set.** One telephoto frame at ISO 500
  among ISO 64 main-camera frames was a blue-channel outlier by up to 9.2% of
  white. Check `exiftool -s -UniqueCameraModel -ISO -FocalLength`. (Today's A and
  B came from *different* modules; that is tolerable only because effective
  coverage is normalised within each frame against its own references.)
- **Position independence was never proved** — every frame used `offset: 0`.
- **iCloud share links deliver JPEG** — export the original from Photos.

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
