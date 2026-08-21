# Handover — the split hypothesis died, the wiper shipped, and the panel needs a real calibration

**Date:** 2026-08-21 (evening) · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `c880e32`
**Base:** `fix/trmnl-x-ghosting-levers` @ `254705d` (off `main` @ `5c67c62`, protected)

> Supersedes the two earlier 2026-08-21 handovers. **Section 1 is a reversal** —
> the diagnosis those handovers were built on did not survive contact with the
> panel. Read §1 and §6 before doing anything.

---

## 1. What Gate A actually showed — the hypothesis is dead

The plan assumed the mid-panel split was a FastEPD power-up sequencing fault
(TPS65185 UPSEQ writes positioned after PWRUP, where the chip has already
reloaded its defaults). Gate A ran. **The split was gone before the patch was
ever flashed.**

Sequence, all measured:

| Time | Firmware | Split |
|---|---|---|
| 09:07 | 1.8.13 (as found) | **+21** at band 55 |
| 11:21 | 1.8.14, FastEPD **unpatched** | **+2** — noise floor |
| ~11:35 | 1.8.13 restored byte-for-byte | **still gone** (owner, by eye) |

The split did not return when the original firmware was restored, so it was a
persistent change to the **panel**, not firmware behaviour. Most likely cause:
1.8.14 ships `display_wipe()` (`6bcf466 Add screen wiper (#537)`), and a
full-panel clear equalises charge at the gate-driver boundary.

**Consequences.** The FastEPD UPSEQ bug is still a genuine code defect — the
writes cannot take effect where they sit — but the evidence for why it mattered
is gone. It is an upstream **defect report**, not a fix with a measured
before/after. Tasks 3-8's original framing is obsolete; see §3.

Two things also eliminated by measurement: the FastEPD pin `855ce9a4` never
changed between 1.8.13 and 1.8.14 (`git log -S "FastEPD.git#" -- platformio.ini`
shows only the two commits that introduced it), and `73fdc73 Refactor power
(#529)` reads USB/charging status pins, not the panel rails.

---

## 2. The feature already existed upstream

`display_wipe()` in 1.8.14 is **server-triggerable today**: `src/bl.cpp:1548`
runs it whenever the display response's `filename` is `screen_wiper.png`. It
does 100 x `fullUpdate(CLEAR_SLOW, bKeepOn=true)` — panel power held for the
whole burst, which was the design point the plan called the entire reason to
build `panel_clean`.

So byonk now drives TRMNL's own mechanism and needs **no firmware fork**.

**Measured on hardware 2026-08-21:** one wipe = **191 s** (~200 black/white
cycles at ~1/s, matching `display_wipe`'s own source comment — an earlier
"~1000 passes" figure was arithmetic from an enum comment and was wrong). A
full poll cycle at `refresh_rate=60` = **268 s ± 0.4 s**. So 20 wipes ≈ 89 min,
and `MAX_WIPES=200` ≈ 15 h.

**The firmware polls TWICE per wipe** — once for the instruction, then again via
`downloadAndShow()` the moment the wipe finishes. Counting both spends two wipes
per wipe performed, and answering the second with the wiper again trips the
`wiped_this_wake` guard, which returns early and leaves the panel blank.
`RecoveryRegistry::on_poll`'s `awaiting_post_wipe_poll` handles this and was
**confirmed correct on hardware** (done went 1 -> 2, not 1 -> 3).

---

## 3. Exact state

**byonk** (`feat/panel-clean-recovery`), 4 commits this session:
- `0bfe38e` previous handover
- `166480e` **admin-driven panel recovery** — `GET`/`POST`/`DELETE
  /api/admin/devices/{key}/recover`, in-memory sessions, 13 tests
- `cc983a2` measured wipe dose replacing the guessed one
- `c880e32` **Ink Field** calibration screen (§5)

Verified: build clean, `clippy -D warnings` clean, **542 tests pass**.

**Deployed:** `local_byonk` **0.19.0-dev5** running on `homeio.oetiker.ch:3000`.
Recovery endpoints confirmed live (401 unauth vs 404 on a bogus path). A 5-wipe
run completed successfully on device `1C:DB:D4:66:5B:50`.

**`~/scratch/trmnl-firmware`** — branch `feat/panel-clean` @ `30f9ae0`
(`panel_clean` parsing, 7/7 tests). **Task 4 was never written** and probably
should not be; see §4. `local/validation` @ `d5af13e` is its base.
**Device currently runs the 1.8.14 control build** (stock FastEPD, reports
1.8.15 — the version bump is in shared `config.h`, so control and patched
builds are indistinguishable by version; use the FastEPD object fingerprint:
control `3208620b…`, patched `18bb0faa…`).

**`~/scratch/fastepd`** — `validate/upseq-carta1300` checked out; both branches
carry a byte-identical patch. Now only useful as an upstream defect report.

**Full flash backup:** `~/scratch/panel-evidence/flash-backup-2026-08-21.bin`
(16777216 bytes, contains 1.8.13). Restore writes ~1.5 MB regions — see §6.

---

## 4. The open question: is `panel_clean` worth building?

Measured, not guessed: of each 268 s cycle, **191 s is wiping and 77 s (29%) is
overhead** — boot, WiFi, image download, content repaint, sleep. A single long
`panel_clean` burst would pay that overhead once instead of per wipe, and would
skip repainting the burnt-in image between wipes.

- **Worth building** if the panel needs hours of wiping.
- **Not worth it** if 20 wipes clears it.

The wipe rate is panel-limited (~1 black/white cycle per second), so
`panel_clean` cannot wipe *faster* — only with less overhead. Decide after a
longer run. Task 3 (`30f9ae0`) is committed and costs nothing to drop.

---

## 5. The new front: this panel's ink levels are badly wrong

The owner spotted it by eye on the glass. **Confirmed by linear raw** (iPhone
ProRAW, `dcraw -4 -T -o 0 -r 1 1 1 1`, flat-field corrected per column):

| Owner said | Measured ΔL* |
|---|---|
| 0 and 1 almost the same | **+0.68** |
| 3 and 4 wide gap | **+25.10** |
| 7 8 9 almost the same | **+1.95, +0.57** |
| jump to 10 | **+6.31** |
| 11 and 12 almost the same | **+1.37** |

Even spacing would be 6.06 L*. Real range **0.57 to 25.10 — a 44:1 ratio**,
plus a second chasm at 1→2 (+20.52). byonk assumes a smooth ramp whose extremes
differ only 2.6:1. Its `colors_actual` for `trmnl_x` (`default-config.yaml:35`)
is interpolated from a generic curve six panels share and was never measured.

**Do not use the calibration derived from `gradient-lab`.** Its ramp is
spatially ordered, so the room's lighting is indistinguishable from the tone
curve. That is what produced a bogus "two-halves black level step" which a
second photo from another angle disproved — it was glare.

**`local/inkfield` / `screens/builtin/calibration/inkfield/` (`c880e32`) is the
fix.** A 16x16 Latin square, ink `(3*row + col) mod 16`, 117x88 px cells. Every
ink appears once per row and once per column, so a separable lighting field
cancels *by construction*. Black and white land near every position, so each
cell normalises against local references — dark-frame plus flat-field from one
hand-held shot. Reads `layout.colors`, so it generalises to colour panels.
Renders correctly; **not yet photographed or measured**.

Next: **flush the panel first** (ghosting biases every cell), show `inkfield`,
one square-on DNG, measure. Then the owner's idea of a **panel auto-calibrator**
— grid detect, per-cell means, local black/white normalisation, average per ink
— which is a plain program with no model in the loop.

The screen exists twice: `screens/builtin/…` is its home but needs a byonk
rebuild; `local/inkfield` on homeio is the live copy. Fold into one once settled.

---

## 6. Traps that cost real time today

**The owner's eye beat the instrument twice.** Once on 7-8-9 (the JPEG's local
tone mapping invented separation that hid a real collision, and invented a
collision at 5/6 that does not exist), once on the black-level step (glare).
When a processed measurement disagrees with what the panel looks like, suspect
the measurement. A phone JPEG's tone mapping is **local**, so it is not a
monotonic transform and *can* reverse the order of two tones.

**esptool's dependencies live in the Homebrew venv.** `pio` here runs
`/opt/homebrew/Cellar/platformio/*/libexec/bin/python`, not `~/.platformio/penv`.
`brew upgrade platformio` will break `merge_bin` again:
```bash
/opt/homebrew/Cellar/platformio/*/libexec/bin/python -m pip install \
  "bitstring>=3.1.6,!=4.2.0" "cryptography>=43.0.0" "pyserial>=3.3" \
  "reedsolo>=1.5.3,<1.8" "PyYAML>=5.1" intelhex "rich_click<2" "click<9"
```

**A new PlatformIO env silently gets a default ESP-IDF config.**
`sdkconfig_path = sdkconfigs/sdkconfig.${this.__env__}` is inherited by
`extends`, so a new env resolves to a file that does not exist and PlatformIO
generates one 330 lines from the shipped config — **and still reports SUCCESS**.
Seed it and commit it. Also: changing an sdkconfig does **not** regenerate
`memory.ld`; run `pio run -e <env> -t clean` first or get a bogus
`rtc_reserved_seg overflowed by 16 bytes`.

**PlatformIO cannot flash this device.** `-t upload` spends 75 s rebuilding and
merging while the sleeping device drops USB; `-t nobuild` breaks its esptool 5.x
argument construction. Drive esptool directly with a retry loop, and **keep
writes to ~1.5 MB regions** — a 16 MB or even 3 MB write dies with "chip stopped
responding", while bootloader + partitions + app (1.4 MB) succeeds first try.

**byonk does not hot-reload `/config/config.yaml`** on the add-on. Restart it —
and change config *before* starting a recovery run, since a restart cancels
in-memory sessions.

**`cargo clean` freed 225 GiB.** `target/debug/incremental` alone was 50 GB and
never self-prunes.

---

## 7. Environment

- Device `1C:DB:D4:66:5B:50` on **`/dev/cu.usbmodem101`**, firmware 1.8.14 control.
- **Never `erase_flash` or `pio run -t erase`** — wipes NVS, WiFi and registration.
- `env:TRMNL_X` is the clean control build: if a local build fails, build that
  first to find out whether the repo or your env is at fault.
- **homeio deploy** (`root@homeio.oetiker.ch`, add-on `local_byonk`, source at
  `/addons/byonk`):
  1. `git archive --format=tar HEAD Cargo.toml Cargo.lock src crates fonts screens byonk-base static docs/src custom_components default-config.yaml | ssh root@homeio.oetiker.ch 'tar -xf - -C /addons/byonk'` — sync the **whole** set; a src-only sync failed with `cannot find module or crate crc32fast` because the host's `Cargo.toml` was older.
  2. Bump `version:` in `/addons/byonk/config.yaml` (the Dockerfile's `BUILD_VERSION` cache-bust key).
  3. `ha store reload && ha addons update local_byonk`
  4. Read failures with `ha supervisor logs` — `ha addons update` only says "unknown error".
  Host has `jq`, **no `python3`**. Admin token:
  `ha addons info local_byonk --raw-json | jq -r .data.options.admin_token` —
  keep it in a shell variable, **never print it** (project CLAUDE.md).
- Raw workflow: `dcraw` installed via brew. `dcraw -4 -T -o 0 -r 1 1 1 1 x.DNG`
  gives linear 16-bit; read it with `ffmpeg -pix_fmt gray16le`.
  **iCloud share links deliver JPEG, not DNG** — export the original from Photos.
- byonk verify: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --lib`. **`make check` has reported exit 0 while tests failed.**
- SDD ledger (git-ignored, 15 rulings):
  `.superpowers/sdd/2026-08-21-panel-clean-recovery/progress.md`

---

## 8. Still open, unrelated to this branch

1. **PR for `fix/trmnl-x-ghosting-levers`** never opened; its three commits are
   in this branch's history, including a data-loss fix (`0fb5c47`).
2. **Restore `homeio`**: `log_level` → `info`, stop `local_byonk` and start
   `43664941_byonk` (currently in `error` state), delete `local/noise-test`.
   Device is on `local/gradient-lab`, `params: {}`; a backup of the pre-session
   config is at `/addon_configs/local_byonk/config.yaml.pre-recovery`.
3. **Timestamped image filenames** — byonk's content-hash names defeat device
   caching (`filesystem.cpp:141`). Own issue.
4. **Four uncommitted files are the owner's separate docs-screenshot task** —
   `config.yaml`, `docs/generate-samples.sh`,
   `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`.
   **Never stage them.** Never `git add -A` in this repo.
