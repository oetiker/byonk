# Handover — the mid-panel split is a power-up transient, and we are patching it upstream

**Date:** 2026-08-21 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `335d58a`
**Base:** `fix/trmnl-x-ghosting-levers` @ `254705d` (itself off `main` @ `5c67c62`, protected)

> Supersedes the 2026-08-20 handover. That one's diagnosis (burn-in) still
> holds and is not repeated here; §1 below is the **new** finding that came out
> of a slow-motion recording, and it changes what we are building.

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

**byonk** (`feat/panel-clean-recovery`): three commits, all docs.
`eb3fb82` spec · `e08fd7b` spec fixup (`refresh_rate` pinned to 1) · `335d58a` plan.
**No byonk source has been touched yet.** Tasks 5–8 do that.

**`~/scratch/fastepd`** — both branches done, both carry the patch:
- `validate/upseq-carta1300` @ `2d4a40e`, off the **pinned** `855ce9a4` ← Gate A flashes this
- `feat/upseq-carta1300` @ `c46bda2`, off `main` `8dc8c74` ← the upstream PR
- Currently checked out: `validate/upseq-carta1300` (correct)

**`~/scratch/trmnl-firmware`** — `local/validation`, **2 files dirty and uncommitted**
(`include/config.h`, `platformio.ini`). Task 2 was mid-flight when this session
ended: the local build env and version bump are written but not committed, and
the two verification builds had not landed.

**Task status:** Task 1 complete and reviewed clean. Task 2 **in flight** —
resume it, do not restart it.

Task 2's implementer stopped at a precise point: both fastepd branches are
committed, the two trmnl-firmware files are edited but **not** committed, and
the first verification build was still running when the session ended:

```
cd ~/scratch/trmnl-firmware && pio run -e TRMNL_X_LOCAL
# log: ~/scratch/panel-evidence-build-validate.log
```

First check whether it finished and what it said:

```bash
tail -20 ~/scratch/panel-evidence-build-validate.log
pgrep -fl "pio run"          # empty means it is done
ls ~/scratch/trmnl-firmware/.pio/build/TRMNL_X_LOCAL/*.bin
```

If it succeeded, the remaining Task 2 work is: commit the two dirty files
(Step 10), then run the second build with `feat/upseq-carta1300` checked out
in fastepd, then leave `validate/upseq-carta1300` checked out.

---

## 4. How to resume

1. Read the ledger. It carries seven rulings and the pre-flight conflict scan.
2. Finish Task 2 from its brief (`.superpowers/sdd/.../task-2-brief.md`):
   commit the two dirty trmnl-firmware files, then run the **amended** Step 9 —
   build `pio run -e TRMNL_X_LOCAL` once with each fastepd branch checked out,
   leaving `validate/upseq-carta1300` checked out at the end.
3. Review Task 2, then **stop at Gate A**. It is the user's: flash, film five
   updates at 240 fps, measure. `~/scratch/panel-evidence/GATE-A.md` has every
   command with the port already filled in.
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

## 6. Environment

- TRMNL X on **`/dev/cu.usbmodem101`**. PlatformIO Core 6.1.19 (`symlink://` OK).
  esptool via `pio pkg exec -- esptool.py`.
- **Never `erase_flash` or `pio run -t erase`** — wipes NVS, taking WiFi
  credentials and the byonk registration with it. Upload the app only.
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
