# Handover — Tasks 1 and 2 are done; everything now waits on Gate A

**Date:** 2026-08-21 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `73b2f12`
**Base:** `fix/trmnl-x-ghosting-levers` @ `254705d` (itself off `main` @ `5c67c62`, protected)

> Supersedes the earlier 2026-08-21 handover. The diagnosis in §1 is unchanged
> and still correct. What changed is §3: Task 2 is finished, and three
> environment traps were found and fixed along the way — read §6, they will
> bite again.

---

## 1. What the video proved

The panel showed a **sharp** horizontal step at mid-panel that was never in any
burnt-in image, with a chip-on-film bond visible at exactly that row. A 240 fps
recording settled it.

Measured, not inferred:

| Real time | Step at the boundary |
|---|---|
| 0–170 ms | −2 (nothing) |
| 175 ms | +7 — panel powers up |
| **187 ms** | **+21 — first flash, full strength** |
| 217 / 250 / 280 ms | +8 / +2 / 0 — decays away |
| 400, 800, 1300, 1400 ms | **0 or ±1** — four more flashes, no split |

Boundary is **band 55 of 104 = 52.9% = row ~743 of 1404** — the gate-driver
chip boundary.

**Not a camera artifact.** It stays pinned to one band across 16 consecutive
video frames (rolling shutter would drift), and it **decays smoothly while the
panel is already static** — a camera artifact cannot fade on a still scene.

**Conclusion.** The two panel halves get different drive on the first frame
after power-up, and only then. The firmware powers the panel down after every
update (`display.cpp:531`, `:2768`), so this kick lands ~4 500 times a day.
The overnight recovery run was *adding* to the split while healing the burn-in.

**Suspected cause.** epdiy programs the TPS65185 power-up sequence before
power-up for this panel (`epd_board_v7_103.c:216-222`, `tps65185.c:122`).
FastEPD has the equivalent writes **commented out** and positioned **after**
PWRUP — where they could never take effect, because the chip reloads defaults
on every WAKEUP deassert. Still a hypothesis; Gate A tests it.

---

## 2. Where the work lives

| Thing | Path |
|---|---|
| Spec (approved) | `docs/superpowers/specs/2026-08-21-panel-clean-firmware-patch-design.md` |
| Plan (approved) | `docs/superpowers/plans/2026-08-21-panel-clean-recovery.md` |
| SDD ledger (git-ignored — **read this first**) | `.superpowers/sdd/2026-08-21-panel-clean-recovery/progress.md` |
| Gate A checklist (user-owned) | `~/scratch/panel-evidence/GATE-A.md` |
| Measurement tool + baseline | `~/scratch/panel-evidence/` |

Three deliverables, deliberately decoupled: a FastEPD power-up fix, a
trmnl-firmware `panel_clean` response field plus device-side clean loop, and
byonk admin-driven recovery sessions.

---

## 3. Exact state — verify against git, do not trust this snapshot

**byonk** (`feat/panel-clean-recovery`): four commits, all docs.
`eb3fb82` spec · `e08fd7b` spec fixup (`refresh_rate` pinned to 1) ·
`335d58a` plan · `73b2f12` previous handover.
**No byonk source has been touched yet.** Tasks 5–8 do that.

**`~/scratch/fastepd`** — both branches carry a byte-identical power-up hunk:
- `validate/upseq-carta1300` @ `2d4a40e`, off the **pinned** `855ce9a4` ← Gate A flashes this
- `feat/upseq-carta1300` @ `c46bda2`, off `main` `8dc8c74` ← the upstream PR
- Currently checked out: `validate/upseq-carta1300` (correct). Tree clean.

The only differences between the two branches are `main`'s own drift in the
panel row: clock `26666666`→`20000000`, `BB_PANEL_FLAG_DARK`→`SLOW_SPH`, line
padding `16`→`44`. Our added `| BB_PANEL_FLAG_UPSEQ_MC2` is identical on both.

**`~/scratch/trmnl-firmware`** — `local/validation` @ `d5af13e`, **tree clean**.
Three files committed: `platformio.ini`, `include/config.h`,
`sdkconfigs/sdkconfig.TRMNL_X_LOCAL`.

**Task status:** Tasks 1 and 2 complete. Task 2 review pending. Then **Gate A**.

**The built artifact is ready to flash.** `.pio/build/TRMNL_X_LOCAL/firmware.bin`
reports `1.8.15`, and its FastEPD object matches the validate-branch
fingerprint `18bb0faa51d397e57a4d2ceb85eba3dc` (see §6 for how that was
established and why the obvious check does not work).

---

## 4. How to resume

1. Read the ledger. It now carries **ten rulings**, the pre-flight conflict
   scan, and the Task 2 closeout.
2. Review Task 2 (`git log 6bff55b..d5af13e` in trmnl-firmware; the two fastepd
   commits). Note Task 2 deviated from its brief in one deliberate way —
   Ruling 8 — and the deviation is load-bearing, not cosmetic.
3. **Stop at Gate A.** It is the user's: flash, film five updates at 240 fps,
   measure. `~/scratch/panel-evidence/GATE-A.md` has every command with the
   port already filled in.
4. Gate A decides everything downstream. PASS → Tasks 3–8 and two upstream PRs.
   FAIL → the split is a hardware defect; the clean mode still has value but the
   FastEPD PR becomes a defect report.

Continue with `superpowers:subagent-driven-development`.

---

## 5. Decisions that cost real work if reversed

- **Two fastepd branches, not one.** `main` is **38 commits ahead** of what
  TRMNL ships, including ESP32-S3 parallel bit-banging and row-start-timing
  changes. Validating off `main` would leave a vanished step unattributable.
  Both clones were shallow (depth 1) and have been unshallowed.
- **`sdkconfigs/sdkconfig.TRMNL_X_LOCAL` must exist and must stay a byte copy
  of `sdkconfig.TRMNL_X`.** See §6 — without it the validation firmware is not
  configuration-identical to the shipped build, and Gate A proves nothing.
- **`refresh_rate: 1` during a burst.** Each poll costs ~6–7 s of fixed radio
  overhead; any gap beyond it is dead time. `cycles_per_burst` is the only
  pacing knob.
- **Padding is not optional.** Unpadded solid white gets **1 active drive pass
  of 9**; padded, level 15 in the 38-pass table gets **36 of 38**, with a full
  black→white swing. That is the ratio of healing to power-up kicks.
- **Recovery sessions are in-memory.** A byonk restart cancels a run. Matches
  how `Device` already treats runtime state, and fails safe.
- **Task 6's plan test is tautological** (asserts `Option::unwrap_or`) — Ruling 2
  in the ledger says replace it when Task 6 runs.

---

## 6. Environment — three traps found on 2026-08-21, all still live

**Trap 1: esptool's dependencies live in the Homebrew venv, and `brew upgrade`
will wipe them.** `pio` here is the Homebrew build and runs
`/opt/homebrew/Cellar/platformio/6.1.19_2/libexec/bin/python`, **not**
`~/.platformio/penv`. PlatformIO upgraded `tool-esptoolpy` to 5.1.2, whose
dependencies were missing there, so every post-build `merge_bin` failed with
`ModuleNotFoundError: No module named 'rich_click'`. Fix — reinstall the list
from the package's own `pyproject.toml`:

```bash
/opt/homebrew/Cellar/platformio/*/libexec/bin/python -m pip install \
  "bitstring>=3.1.6,!=4.2.0" "cryptography>=43.0.0" "pyserial>=3.3" \
  "reedsolo>=1.5.3,<1.8" "PyYAML>=5.1" intelhex "rich_click<2" "click<9"
```

Verify with `pio pkg exec -p tool-esptoolpy -- esptool.py version`.

**Trap 2: a new PlatformIO env silently gets a default ESP-IDF config.**
`platformio.ini:503` sets the config path from the env *name*:

```ini
board_build.esp-idf.sdkconfig_path = sdkconfigs/sdkconfig.${this.__env__}
```

`extends = env:TRMNL_X` inherits that line verbatim, so `TRMNL_X_LOCAL`
resolved to a file that did not exist and PlatformIO generated one from ESP-IDF
defaults — **330 lines** away from the shipped config, including bootloader
optimisation level, log levels and `CONFIG_BOOTLOADER_RESERVE_RTC_SIZE` — and
**still reported SUCCESS**. Any new env needs its sdkconfig seeded from the
shipped one and committed.

**Trap 3: changing an sdkconfig does not regenerate `memory.ld`.** PlatformIO
recompiles the sources but keeps the stale linker script, which then fails with
`region 'rtc_reserved_seg' overflowed by 16 bytes` (stale `(0 + 24)` vs correct
`((0x10 aligned to 8) + 24)` = 40). After any sdkconfig change run
`pio run -e <env> -t clean` first. Branch switches in the symlinked FastEPD do
**not** need a clean — incremental builds track them correctly (proven below).

**How to check which FastEPD branch is baked into a build.** The obvious test —
searching `firmware.bin` for the panel clock constant `26666666` vs `20000000` —
is **invalid**; the `env:TRMNL_X` control built from pinned `855ce9a4` shows
identical counts. Fingerprint the object instead:

```bash
# the libNNN/ dir is a PlatformIO hash and can change — find it, don't hardcode it
md5 -q $(find ~/scratch/trmnl-firmware/.pio/build/TRMNL_X_LOCAL -name FastEPD.cpp.o)
# 18bb0faa51d397e57a4d2ceb85eba3dc = validate/upseq-carta1300
# 0a1c395c8d393c272180b2ae6b86f93d = feat/upseq-carta1300
```

**Other environment notes**

- TRMNL X on **`/dev/cu.usbmodem101`**. PlatformIO Core 6.1.19 (`symlink://` OK).
- **Never `erase_flash` or `pio run -t erase`** — wipes NVS, taking WiFi
  credentials and the byonk registration with it. Upload the app only.
- `env:TRMNL_X` is the clean control build: if a local build fails, build that
  one first to find out whether the repo or your env is at fault.
- Build logs: `~/scratch/panel-evidence-build-validate.log`,
  `-build-pr.log`, `-build-shipped.log`.
- `homeio.oetiker.ch` still runs `local_byonk` 0.19.0-dev3 on `:3000` with
  `log_level: debug`. Sandbox blocks outbound TCP — `ssh`/`curl` need
  `dangerouslyDisableSandbox: true`.
- byonk verify: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --lib`. **`make check` has reported exit 0 while tests failed** —
  read the output.

---

## 7. Still open from before, unrelated to this branch

1. **PR for `fix/trmnl-x-ghosting-levers`** was never opened. Its three commits
   are in this branch's history, including a real data-loss fix (`0fb5c47` —
   assigning a screen wiped every other device setting).
2. **Restore `homeio`**: `log_level` → `info`, stop `local_byonk` and start
   `43664941_byonk`, reassign `local/gradient-lab`, delete `local/noise-test`.
3. **Timestamped image filenames** — byonk's content-hash names defeat device
   caching, so every download wipes every other byonk image
   (`filesystem.cpp:141`). Own issue.
4. **Four uncommitted files are the owner's separate docs-screenshot task** —
   `config.yaml`, `docs/generate-samples.sh`,
   `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`.
   **Never stage them.** Never `git add -A` in this repo.
