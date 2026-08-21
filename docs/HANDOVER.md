# Handover — TRMNL X burn-in: diagnosed, treated, and three fixes landed

**Date:** 2026-08-20 · **Branch:** `fix/trmnl-x-ghosting-levers` · **HEAD:** `254705d`
**Base:** `main` @ `5c67c62` (protected; open a PR, do not push)

> **Read §1 first.** The previous handover's whole premise was wrong, and this
> one supersedes it completely. Do not act on anything from the old §5.

---

## 1. What the ghost actually was

**Burn-in, not refresh residue.** The panel had held near-identical pixels for
weeks (a calibration screen, and later `local/gradient-lab`, whose *bands never
changed* even though it refreshed every 60 s). Pigment particles adhere to the
capsule wall under long static dwell and charge accumulates in the surrounding
material, so the bias lives **in the panel**, not in the last frame. Driving the
next refresh harder cannot remove it.

**The owner diagnosed it, not the code reading.** Two observations settled it:
the panel visibly flashes black/white/black/white before every image *and still
ghosted*; and the ghost was of an image displayed for weeks, not the previous
one.

**Treatment worked.** Hours of hard cycling on `local/panel-recovery` took the
ghost from plainly legible to "virtually gone… not entirely fixed but in a much
better state". Recovery is asymptotic — early progress is fast, the remainder is
slow, and deep cases may never clear completely.

**Status: essentially resolved.** The panel is still cycling. Leave it.

---

## 2. Committed on this branch (3 commits, all verified)

```
254705d  fix(error-screen): wrap the message instead of running it off the panel
0fb5c47  fix(admin): stop a screen assignment from eating the device's other settings
974ed7d  feat(display): per-device anti-ghosting levers, told straight for the X
```

**`974ed7d`** — three per-device settings plus corrected docs:
`temperature_profile` (`default|a|b`), `maximum_compatibility`, `min_png_bytes`
(new, `src/rendering/png_pad.rs`). The docs previously described the wrong
mechanism for a TRMNL X; a **Burn-in** section was added.

**`0fb5c47`** — **a real data-loss bug, found in the field.** `apply_device_patch`
rebuilt the device's YAML block from `DeviceWrite` (8 fields) and
`upsert_device` replaces the block wholesale, so **any screen assignment deleted
every other device setting** — `temperature_profile`, `maximum_compatibility`,
`min_png_bytes`, `error_clamp`, `noise_scale`, `chroma_clamp`, `strength`,
`gamut`. Silent. Hit the admin API, the web UI and MCP `assign_screen` alike.
Fixed by preserving **by exclusion**, so a new `DeviceConfig` field survives
without anyone remembering this file.

**`254705d`** — the error screen drew the whole message as one centred,
unwrappable `<text>`, so it ran off both edges. Lua tracebacks were worst: their
newlines were collapsed into one run. Now wrapped `<tspan>` lines, left-aligned,
ellipsis on truncation.

### Still uncommitted — NOT this branch's work

`config.yaml`, `docs/generate-samples.sh`,
`docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml` are the
owner's separate docs-screenshot task. **Leave them alone.** (`docs/HANDOVER.md`
is this file.)

---

## 3. Firmware facts — verified by reading source, do not re-derive

Clones at `~/scratch/trmnl-firmware` (v1.8.14), `~/scratch/fastepd`,
`~/scratch/epdiy`.

1. **The panel is an E Ink ED103MC2 (Carta 1300), 1872×1404, 16-bit parallel,
   VCOM −11.00 V.** `display.cpp:180` → `initPanel(BB_PANEL_TRMNL_X)` →
   `FastEPD.inl:230`. Matched against `epdiy/src/displays.c:85-91`.
2. **`BB_EPAPER` is not defined for a TRMNL X.** `display.cpp:18` defines it
   `#ifndef BOARD_X_CLASS`. So **`maximum_compatibility` does nothing on an X** —
   every line reading it is inside `#ifdef BB_EPAPER`. There is no partial
   refresh on this board; it always calls `fullUpdate()`.
3. **`temperature_profile` on an X is a boolean, not a waveform.**
   `display.cpp:1923`: any non-`default` value picks `CLEAR_SLOW` (4 phases ×8
   passes) over `CLEAR_FAST` (2) on *every* update instead of every 8th. `a` and
   `b` are indistinguishable. The `dpList[]` waveform table exists only on
   `bb_epaper` boards.
4. **`c` was never implemented.** `parse_response_api_display.cpp:36-38` has it
   commented out, so a device reads `c` as `default` — *disabling* the extra
   clearing. Byonk now refuses it.
5. **File size selects the grayscale table.** `display.cpp:1902`: over
   **102 400 bytes** the firmware uses a 38-pass table, under it a 9-pass one.
   The 38-pass recipe for levels 7/8 drives each pixel **fully white, then fully
   black, then trims** — a complete particle swing the 9-pass table never
   performs. `MAX_IMAGE_SIZE` is 750 000 on X-class, 90 000 elsewhere (so the
   38-pass table is unreachable on every other panel).
6. **Three repaint states, not two** (`bl.cpp:1555-1571`, `2079-2088`):
   same-as-last-displayed → no download **and no repaint**; on flash but not
   last → **no download, full repaint**; unknown → download + repaint.
7. **Byonk's filenames defeat device caching.** The purge reads the **last 10
   characters** of a filename as a Unix timestamp and deletes anything not
   within 24 h (`filesystem.cpp:141`). Byonk names images by content hash, so
   the parse is garbage and **every download wipes every other byonk image**.
   TRMNL's own names end in an epoch (`mashup-066cc3-1771674964`).
8. **Possible firmware bug, unreported:** `FastEPD.inl:823-828` has the
   TPS65185 `UPSEQ0=0xE1 / UPSEQ1=0xAA` writes **commented out** in
   `EPDiyV7EinkPower()`. epdiy flags this exact panel `DISPLAY_UPSEQ_MC2` and
   calls `tps_set_upseq_carta1300()` **before** power-up
   (`epd_board_v7_103.c:216-222`); `SensoriaEinkPower()` in the same FastEPD
   file writes them live. Unproven link to ghosting — worth reporting to TRMNL.

---

## 4. Measured on hardware — numbers, not estimates

**`refresh_rate` is the gap *after* the update, not a period.** Confirmed:
period = update + refresh_rate, with update constant.

| Configuration | Cycle | Cycles/h |
|---|---|---|
| `gradient-lab` @ 60 s (the morning baseline) | ~100 s | ~36 |
| Flat grey @ 10 s, profile `default` | 17.8 s | 202 |
| Flat grey @ 1 s, profile `default` | 8.80 s | 409 |
| Flat grey @ 1 s, profile `a` | 9.56 s | 377 |
| **Noise @ 1 s, profile `a`, 38-pass** | **19.05 s** | **189** |

- **~6 s of every cycle is fixed radio overhead** — a miss (poll, nothing to do)
  costs 7.0 s at `refresh_rate: 1`. This is why 1 s and 10 s give 8.8 s and
  17.8 s.
- `CLEAR_SLOW` costs only **+0.76 s** (~9%) for double the clearing. Good trade.
- The 38-pass table roughly **doubles** the cycle (not 4×, as passes alone would
  suggest — much of the rest is overhead and a 346 KB transfer).
- byonk spends **1.83 s per cycle** rendering the turbulence (vs 0.36 s flat).
  Fine for one panel; would not scale to many.
- **Two-image rotation was tried and is worse**: 9 fetches for 9 repaints (zero
  cache hits, because of §3.7), 44% miss rate, 19.0 s → **24.5 s**. Reverted.
- Download vs decode is **still unseparated**. `local/panel-recovery` now takes a
  `freq` param for exactly this test, and
  `GET /api/admin/devices/{key}/preview` returns the PNG so sizes can be
  measured without waiting on the device.

---

## 5. `local/panel-recovery` — how it works, and why each part is load-bearing

Lives in the **`local` repo on `homeio`** (writable via MCP), not in this repo.

- **Per-pixel noise between levels 7 and 8** (`#777777`/`#888888`) via
  `feTurbulence` → `feColorMatrix` → `feComponentTransfer type="discrete"`.
- **`discrete` onto exact palette entries, and the rect is unmarked**, so
  exact-match pinning passes every pixel through the dither untouched. *The
  owner's point.* Without it the dither remakes the field into its own pattern,
  which compresses well and drops the file under the threshold.
- Result: **~346 KB** vs 2.7 KB flat — 3.4× the 102 400 threshold, stable across
  seeds (±289 bytes). This reaches the 38-pass table **with no byonk deploy and
  no `min_png_bytes`**.
- **It still reads as flat.** Adjacent levels are 1/16 apart, so the ghost stays
  readable — black and white have too much drive margin to show a bias.
- **Re-seeded every round.** A fixed seed looks like noise but is a *static
  pattern*, holding every pixel at one level for hours — the very condition that
  causes burn-in.
- **Nonce in an SVG comment** forces a new filename each round, guaranteeing a
  repaint. Required until §3.7 is fixed.

---

## 6. Test environment — `homeio.oetiker.ch`

HAOS 18.2, amd64, root SSH passwordless. **Sandbox blocks outbound TCP: every
`ssh`/`curl` needs `dangerouslyDisableSandbox: true`.**

- `local_byonk` **0.19.0-dev3** running on `:3000` — built **before** this
  branch, so it has none of these fixes (notably §2's config-eating bug).
- **Do not call `assign_screen` against it.** It will wipe
  `temperature_profile: a` from the device block again. That is how the bug was
  found.
- Published add-on `43664941_byonk` (0.19.0) is **stopped** (same port).
- `log_level` is **debug** — restore to `info`.
- Admin token is in `~/.claude.json` under this project's MCP config. **Never
  print it**; read it into a shell variable.
- Config: `/addon_configs/local_byonk/config.yaml`. Byonk has **no file
  watcher** — edit then `ha addons restart local_byonk`. Backups exist as
  `config.yaml.bak` and `config.yaml.pre-tempprofile`.
- Rebuild recipe unchanged from the previous handover: scaffold in
  `/addons/byonk/`, Debian rust base (not Alpine — `utoipa-swagger-ui` needs
  `curl`), `ARG BUILD_VERSION` right before `cargo build`, sync with
  `COPYFILE_DISABLE=1 tar`, then bump `version:` → `ha store reload` →
  `ha addons update local_byonk`.

---

## 7. Build / verify

```
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --lib          # 529 passed, 0 failed, 3 ignored
```

- **`make check` reported exit 0 while tests FAILED.** Do not trust its exit
  code; read the output.
- **`lua_api_test` has 2 known-flaky TLS tests** under `--workspace` contention
  (see `7902ef5`). They pass when the binary runs alone: `cargo test --test
  lua_api_test` → 125/125. Unsandboxed only — the sandbox blocks loopback TLS.

---

## 8. TODO, in order

1. **Leave the panel cycling.** The remaining ghost is the slow part.
2. **Owner: take the "after" photo** — same angle/light, panel resting on grey.
   The before/after pair is what TRMNL asked for, and we have a named panel
   model and firmware version to go with it.
3. **Restore `homeio`** when the run ends: `log_level` → `info`,
   `ha addons stop local_byonk && ha addons start 43664941_byonk`, reassign
   `local/gradient-lab` and restore its refresh, **delete the `local/noise-test`
   scratch screen**.
4. **Open the PR** for this branch.
5. **Timestamped image filenames** (§3.7) — own issue, unrelated to ghosting.
   Devices could then skip downloads entirely. Constraints: first 14 characters
   of two names must differ (`strncmp(name, szTemp, 14)` means "older version of
   the same image"), and `filesystem_fix_filename()` keeps only the first 7 plus
   last 17 characters beyond 31.
6. **The interactive recovery feature** — now justified rather than speculative.
   Owner's framing: *a temporary mode that exits on its own*, so nobody has to
   remember to switch back. Design input gathered above; needs a brainstorm.
   Note an hour is likely **not** enough, and byonk has per-device state (a
   server), so it could do the two-image rotation properly once §3.7 lands.
7. Optional: separate download from decode cost (§4).
8. Optional: the error SVG is hard-coded to `viewBox="0 0 800 480"`, so it is
   stretched on a 1872×1404 X. Fixing it means threading device dimensions into
   `render_error`.
9. Optional: report the `UPSEQ` finding (§3.8) to TRMNL.

---

## 9. Lessons worth carrying

- **Refresh frequency is not protection against burn-in.** `gradient-lab`
  refreshed every 60 s for weeks and still burned in, because only its clock
  changed. What matters is *pixels moving*, not the panel waking up.
- **Two of the three commits here are bugs found by accident** — one because a
  setting vanished off a live panel, one because a broken screen was put on a
  display. Neither would have surfaced from tests.
- **Do not estimate firmware costs; measure them.** Predictions of 11–13 s and
  25–30 s were both wrong (real: 9.56 s and 19.05 s).
- **Never match a clock quantum to an assumed fetch interval** — still true, and
  it is why the two-image rotation had to use a coin flip rather than
  alternation.
