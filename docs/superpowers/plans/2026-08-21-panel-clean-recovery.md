# Panel Clean & Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the per-power-up gate-driver kick on a TRMNL X, and give byonk a bounded, efficient panel-recovery burst it can drive on demand.

**Architecture:** Three independent deliverables in three repos. `bitbank2/FastEPD` gets a power-up-sequence fix only. `usetrmnl/trmnl-firmware` gets a `panel_clean` response field and a device-side clean loop built on FastEPD's existing public API. Byonk gets in-memory recovery sessions driven by two admin endpoints. Two user-owned hardware gates sit between the phases; nothing downstream of a gate may start until the gate reports.

**Tech Stack:** C++ / Arduino / PlatformIO / Unity (firmware), Rust / axum / serde / utoipa (byonk), Python 3 + ffmpeg (measurement).

**Spec:** `docs/superpowers/specs/2026-08-21-panel-clean-firmware-patch-design.md`

## Global Constraints

- Panel is an E Ink ED103MC2 (Carta 1300), 1872x1404, 16-bit parallel, VCOM -11.00 V.
- Firmware baseline is 1.8.14 (`usetrmnl/trmnl-firmware` @ `6bff55b`); FastEPD is pinned at `855ce9a42c0be5cd8ce2f2425483a8271a04668d` in `[env:TRMNL_X]`.
- `panel_clean` is clamped to **1000** in BOTH the firmware parser and byonk. The two limits must not drift.
- `cycles` is clamped to **1000000**. `cycles_per_burst` defaults to **200**.
- Byonk sends **`refresh_rate: 1`** for every response carrying a `panel_clean` burst.
- Byonk's minimum firmware version for the feature is **1.8.15**, provisional until TRMNL tags a release.
- **Never `git add -A` or `git add .`** in the byonk repo. Add by explicit path and check `git diff --cached` before committing. Four unrelated files (`config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`) are modified in the working tree and belong to a separate task. Never stage them.
- Byonk verification is `cargo fmt`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --lib`. **Do not trust `make check`'s exit code** — it has reported 0 while tests failed. Read the output.
- The `[env:TRMNL_X_LOCAL]` build env and the `FW_PATCH_VERSION` bump exist ONLY on the throwaway `local/validation` branch. They must never appear on a PR branch.
- Branches: `feat/upseq-carta1300` in `~/scratch/fastepd`; `feat/panel-clean` and `local/validation` in `~/scratch/trmnl-firmware`; `feat/panel-clean-recovery` in `/Users/oetiker/checkouts/byonk` (already checked out).

---

### Task 1: Split-measurement script

Build the measurement tool first, and prove it against the recording we already have. The baseline video is a known input with a known answer, so this task is fully testable before any firmware changes exist.

**Files:**
- Create: `~/scratch/panel-evidence/measure_split.py`
- Fixture (read-only, do not modify): `~/Downloads/2026-08-21 09.07.34.mp4`

**Interfaces:**
- Consumes: nothing.
- Produces: a CLI `python3 ~/scratch/panel-evidence/measure_split.py --video <path>` printing `band=<int> peak_step=<int> peak_real_ms=<float>` on its last line. Gate A and Gate B both call it.

- [ ] **Step 1: Create the directory**

```bash
mkdir -p ~/scratch/panel-evidence
```

- [ ] **Step 2: Write the script**

Create `~/scratch/panel-evidence/measure_split.py`:

```python
#!/usr/bin/env python3
"""Measure the mid-panel gate-driver step in a slow-motion e-paper recording.

Extracts a per-frame vertical luminance profile from a panel recording, finds
the band whose row-to-row step is largest across the clip, and reports that
step's peak magnitude and the real (de-slowed) time at which it occurs.
"""
import argparse
import subprocess
import sys


def extract_profiles(video, crop, bands):
    """Return a list of per-frame profiles, each a list of `bands` ints 0-255.

    Scaling the cropped region to 1 pixel wide with the `area` filter makes each
    output row the mean luminance of that band, which is exactly the profile we
    want and costs one ffmpeg pass.
    """
    cmd = [
        "ffmpeg", "-v", "error", "-i", video,
        "-vf", f"crop={crop},format=gray,scale=1:{bands}:flags=area",
        "-f", "rawvideo", "-pix_fmt", "gray", "-",
    ]
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    return [list(raw[i * bands:(i + 1) * bands])
            for i in range(len(raw) // bands)]


def find_step(profiles, bands, edge_margin):
    """Find the band with the largest single-frame step, ignoring panel edges.

    Returns (band, peak_step, peak_frame). The sign of peak_step is kept: a
    positive value means the band below the boundary is brighter.
    """
    best = (0, 0, 0)
    for frame, p in enumerate(profiles):
        for b in range(edge_margin, bands - edge_margin):
            step = p[b] - p[b - 1]
            if abs(step) > abs(best[1]):
                best = (b, step, frame)
    return best


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--video", required=True)
    ap.add_argument("--crop", default="180:520:70:92",
                    help="ffmpeg crop w:h:x:y selecting panel area only")
    ap.add_argument("--bands", type=int, default=104)
    ap.add_argument("--edge-margin", type=int, default=5,
                    help="bands to ignore at top and bottom (bezel, glare)")
    ap.add_argument("--capture-fps", type=float, default=240.0)
    ap.add_argument("--container-fps", type=float, default=30.0)
    ap.add_argument("--trace", action="store_true",
                    help="print the step at the detected band for every frame")
    args = ap.parse_args()

    profiles = extract_profiles(args.video, args.crop, args.bands)
    if not profiles:
        print("no frames decoded", file=sys.stderr)
        return 2

    band, peak, peak_frame = find_step(profiles, args.bands, args.edge_margin)
    slow = args.capture_fps / args.container_fps
    real_ms = peak_frame / args.container_fps / slow * 1000.0

    if args.trace:
        for f, p in enumerate(profiles):
            t = f / args.container_fps / slow * 1000.0
            print(f"{t:8.1f}ms {p[band] - p[band - 1]:+5d}")

    pct = band / args.bands * 100.0
    print(f"frames={len(profiles)} slowmo={slow:g}x "
          f"real_duration_ms={len(profiles) / args.container_fps / slow * 1000:.1f}")
    print(f"band_pct={pct:.1f}%")
    print(f"band={band} peak_step={peak:+d} peak_real_ms={real_ms:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 3: Run it against the known baseline**

```bash
python3 ~/scratch/panel-evidence/measure_split.py --video ~/Downloads/2026-08-21\ 09.07.34.mp4
```

Expected, reproducing the analysis already done by hand:
- `slowmo=8x`, `real_duration_ms` about `1554`
- `band=55` (accept 54-56), `band_pct` about `52.9%`
- `peak_step=+21` (accept +19 to +23)
- `peak_real_ms` about `187` (accept 175-200)

If the band or the step magnitude is outside those ranges the script is wrong, not the panel. Fix the script.

- [ ] **Step 4: Copy the baseline video next to the script**

The recording is evidence and must not live only in `~/Downloads`.

```bash
cp ~/Downloads/2026-08-21\ 09.07.34.mp4 ~/scratch/panel-evidence/before-upseq-01.mp4
python3 ~/scratch/panel-evidence/measure_split.py --video ~/scratch/panel-evidence/before-upseq-01.mp4 | tee ~/scratch/panel-evidence/before-upseq-01.txt
```

Confirm the tee'd output matches Step 3.

- [ ] **Step 5: Report**

This task produces no commit — `~/scratch/panel-evidence/` is evidence, not source, and is deliberately outside every repo. Report the three measured numbers.

---

### Task 2: FastEPD power-up sequence fix, plus a local build env that proves it compiles

**Files:**
- Modify: `~/scratch/fastepd/src/FastEPD.h:43` (add the flag define)
- Modify: `~/scratch/fastepd/src/FastEPD.inl:230` (add the flag to the panel entry)
- Modify: `~/scratch/fastepd/src/FastEPD.inl:817` (insert the guarded writes before PWRUP)
- Modify: `~/scratch/trmnl-firmware/platformio.ini` (append a local build env — `local/validation` branch ONLY)
- Modify: `~/scratch/trmnl-firmware/include/config.h` (version bump — `local/validation` branch ONLY)

**Interfaces:**
- Consumes: nothing.
- Produces: `BB_PANEL_FLAG_UPSEQ_MC2` (value `0x10`) in `FastEPD.h`; a PlatformIO env named `TRMNL_X_LOCAL` that builds trmnl-firmware against the local FastEPD checkout and reports firmware version `1.8.15`.

- [ ] **Step 1: Branch FastEPD**

```bash
cd ~/scratch/fastepd && git checkout -b feat/upseq-carta1300
```

- [ ] **Step 2: Add the panel flag**

In `src/FastEPD.h`, after the `BB_PANEL_FLAG_DARK` line, add:

```c
#define BB_PANEL_FLAG_DARK     0x08
#define BB_PANEL_FLAG_UPSEQ_MC2 0x10
```

`0x10` is the lowest free bit; `flags` is a `uint32_t` in `BBPANELDEF` (`FastEPD.h:123`), so there is plenty of room.

- [ ] **Step 3: Set the flag on the TRMNL X panel entry**

In `src/FastEPD.inl`, the `BB_PANEL_TRMNL_X` row currently begins:

```c
{1872, 1404, 20000000, BB_PANEL_FLAG_MIRROR_X | BB_PANEL_FLAG_SLOW_SPH, {8,18,17,16,15,7,6,5,47,21,14,13,12,11,10,9}, 16, 11, 48, 45, 41, 8, 42,
```

Change only the flags expression to:

```c
{1872, 1404, 20000000, BB_PANEL_FLAG_MIRROR_X | BB_PANEL_FLAG_SLOW_SPH | BB_PANEL_FLAG_UPSEQ_MC2, {8,18,17,16,15,7,6,5,47,21,14,13,12,11,10,9}, 16, 11, 48, 45, 41, 8, 42,
```

Leave every other value on that row untouched, including `-1100` (VCOM) and `44` (line padding).

- [ ] **Step 4: Move the UPSEQ writes before PWRUP**

In `src/FastEPD.inl`, inside `EPDiyV7EinkPower()`, the current sequence is:

```c
        bbepPCA9535DigitalWrite(13, 1); // WAKEUP on
        bbepPCA9535DigitalWrite(11, 1); // PWRUP on
        bbepPCA9535DigitalWrite(12, 1); // VCOM CTRL on
```

Replace those three lines with:

```c
        bbepPCA9535DigitalWrite(13, 1); // WAKEUP on
        if (pState->panelDef.flags & BB_PANEL_FLAG_UPSEQ_MC2) {
            // Carta 1300 (e.g. E Ink ED103MC2) needs a non-default rail power-up
            // order. The TPS65185 reloads its defaults whenever WAKEUP is
            // deasserted, so these must be programmed after WAKEUP and BEFORE
            // PWRUP or they take effect on no power-up at all.
            // Matches epdiy's tps_set_upseq_carta1300(), called from
            // epd_board_v7_103.c before config_reg.pwrup is asserted.
            vTaskDelay(3); // let the TPS65185 finish waking
            ucTemp[0] = TPS_REG_UPSEQ0;
            ucTemp[1] = 0xe1;
            bbepI2CWrite(0x68, ucTemp, 2);
            ucTemp[0] = TPS_REG_UPSEQ1;
            ucTemp[1] = 0xaa;
            bbepI2CWrite(0x68, ucTemp, 2);
        }
        bbepPCA9535DigitalWrite(11, 1); // PWRUP on
        bbepPCA9535DigitalWrite(12, 1); // VCOM CTRL on
```

Then delete the now-redundant commented-out `TPS_REG_UPSEQ0` and `TPS_REG_UPSEQ1` blocks that sit after the `PWRGOOD` wait. **Leave the commented voltage-adjust block (register `0x02`, `0x6`) exactly as it is** — changing drive voltage is out of scope and unsupported by evidence.

- [ ] **Step 5: Commit FastEPD**

```bash
cd ~/scratch/fastepd
git add src/FastEPD.h src/FastEPD.inl
git commit -m "fix: program TPS65185 power-up sequence for Carta 1300 panels

The ED103MC2 and other Carta 1300 panels need a non-default rail power-up
order (UPSEQ0=0xE1, UPSEQ1=0xAA). The writes existed but were commented out,
and were positioned after PWRUP -- where they could never take effect, since
the TPS65185 reloads its defaults on every WAKEUP deassert.

Gated behind a new BB_PANEL_FLAG_UPSEQ_MC2 so no other panel changes
behaviour. Mirrors epdiy's tps_set_upseq_carta1300(), which is called from
epd_board_v7_103.c before pwrup is asserted."
```

- [ ] **Step 6: Branch trmnl-firmware for local validation**

```bash
cd ~/scratch/trmnl-firmware && git checkout -b local/validation
```

- [ ] **Step 7: Add the local build env**

Append to `~/scratch/trmnl-firmware/platformio.ini`:

```ini
; LOCAL VALIDATION ONLY -- never include this env in an upstream PR.
; Builds against the working copy of FastEPD so local edits are picked up.
[env:TRMNL_X_LOCAL]
extends = env:TRMNL_X
lib_deps =
	${deps_app.lib_deps}
	symlink:///Users/oetiker/scratch/fastepd
```

- [ ] **Step 8: Bump the firmware version so byonk's gate will accept this build**

In `~/scratch/trmnl-firmware/include/config.h`, change `FW_PATCH_VERSION` from `14` to `15`. `FW_VERSION_STRING` is composed from the three version macros at `config.h:17`, so the device will report `1.8.15`, which is the minimum byonk requires (see Task 7).

- [ ] **Step 9: Build**

```bash
cd ~/scratch/trmnl-firmware && pio run -e TRMNL_X_LOCAL
```

Expected: `SUCCESS`. A compile error mentioning `BB_PANEL_FLAG_UPSEQ_MC2`, `panelDef`, `ucTemp` or `bbepI2CWrite` means Task 2's FastEPD edit is wrong — fix it and rebuild. This build is the only verification the FastEPD change gets before hardware; FastEPD is a header/`.inl` Arduino library with no standalone test harness.

- [ ] **Step 10: Commit the local-validation scaffolding**

```bash
cd ~/scratch/trmnl-firmware
git add platformio.ini include/config.h
git commit -m "local: build env against working-copy FastEPD, version 1.8.15

NOT FOR UPSTREAM. Lives only on local/validation."
```

---

## GATE A — hardware: does the power-up fix remove the split?

**This gate is the user's. No subagent may perform it, and no task after it may start until it reports.**

The panel is flashed over USB-C.

1. **Back up the existing flash first.**
   ```bash
   esptool.py --port <PORT> flash_id
   esptool.py --port <PORT> read_flash 0 ALL ~/scratch/panel-evidence/flash-backup-2026-08-21.bin
   ```
2. **Upload the app only. Do NOT erase the chip** — `pio run -t erase` or `esptool.py erase_flash` would wipe NVS, losing WiFi credentials and the device's byonk registration and forcing a re-onboard.
   ```bash
   cd ~/scratch/trmnl-firmware && pio run -e TRMNL_X_LOCAL -t upload --upload-port <PORT>
   ```
3. **Film at least 5 update starts** at 240 fps. Same camera position, distance and lighting as the baseline; phone mounted, not handheld; same content transition; similar panel warm-up. Save as `~/scratch/panel-evidence/after-upseq-0N.mp4`.
4. **Measure each one:**
   ```bash
   for f in ~/scratch/panel-evidence/after-upseq-*.mp4; do
     echo "== $f"; python3 ~/scratch/panel-evidence/measure_split.py --video "$f" | tail -1
   done
   ```

**PASS:** `peak_step` at band ~55 falls to the settled-state noise floor, `|step| <= 2`, across all captures. Proceed to Task 3.

**FAIL:** the step persists near +21. The hypothesis is wrong and the split is most likely a hardware defect in one gate-driver bond. **Stop and re-plan** — Tasks 3-8 still have standalone value (the clean mode helps burn-in regardless), but the FastEPD PR becomes a hardware-defect report instead, and its framing changes completely.

---

### Task 3: `panel_clean` response field and parser tests

**Files:**
- Modify: `~/scratch/trmnl-firmware/lib/trmnl/include/api_types.h:34-49` (`ApiDisplayResponse`)
- Modify: `~/scratch/trmnl-firmware/lib/trmnl/src/parse_response_api_display.cpp`
- Test: `~/scratch/trmnl-firmware/test/test_parse_api_display/api_display.test.cpp`

**Interfaces:**
- Consumes: nothing.
- Produces: `uint32_t ApiDisplayResponse::panel_clean`, 0 when absent, clamped to 1000. Task 4 reads it.

- [ ] **Step 1: Branch for the feature, off the validated local branch**

```bash
cd ~/scratch/trmnl-firmware && git checkout -b feat/panel-clean local/validation
```

The `local/validation` commit rides along locally and is dropped when the PR branch is prepared (Task 4, Step 8).

- [ ] **Step 2: Write the failing tests**

In `test/test_parse_api_display/api_display.test.cpp`, add `panel_clean` to the shared comparator, immediately after the `refresh_rate` assertion inside `assert_response_equal`:

```c
  TEST_ASSERT_EQUAL_UINT32(expected.panel_clean, actual.panel_clean);
```

Then add three tests, above `void setUp(void)`:

```c
void test_parseResponse_apiDisplay_panel_clean(void) {
  String input = "{\"status\":0,\"panel_clean\":200}";
  TEST_ASSERT_EQUAL_UINT32(200, parseResponse_apiDisplay(input).panel_clean);
}

void test_parseResponse_apiDisplay_panel_clean_absent_is_zero(void) {
  String input = "{\"status\":0}";
  TEST_ASSERT_EQUAL_UINT32(0, parseResponse_apiDisplay(input).panel_clean);
}

void test_parseResponse_apiDisplay_panel_clean_clamped(void) {
  String input = "{\"status\":0,\"panel_clean\":999999}";
  TEST_ASSERT_EQUAL_UINT32(1000, parseResponse_apiDisplay(input).panel_clean);
}
```

Register all three inside `process()`, after the existing `RUN_TEST` lines:

```c
  RUN_TEST(test_parseResponse_apiDisplay_panel_clean);
  RUN_TEST(test_parseResponse_apiDisplay_panel_clean_absent_is_zero);
  RUN_TEST(test_parseResponse_apiDisplay_panel_clean_clamped);
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cd ~/scratch/trmnl-firmware && pio test -e native -f test_parse_api_display
```

Expected: compile error, `'struct ApiDisplayResponse' has no member named 'panel_clean'`.

- [ ] **Step 4: Add the struct field**

In `lib/trmnl/include/api_types.h`, add to `ApiDisplayResponse`, after `String touchbar_mode;`:

```c
  uint32_t panel_clean;
```

Appending keeps every existing designated-initializer literal valid.

- [ ] **Step 5: Parse and clamp it**

In `lib/trmnl/src/parse_response_api_display.cpp`, add to the error-return literal, after `.touchbar_mode = ""`:

```c
        .touchbar_mode = "",
        .panel_clean = 0};
```

In the success path, after computing `u32TP`, add:

```c
  // Number of device-side black/white recovery cycles to run instead of
  // displaying an image. Clamped so a malformed or hostile response cannot
  // strand the device in a multi-hour loop.
  uint32_t u32PanelClean = doc["panel_clean"] | 0;
  if (u32PanelClean > 1000) u32PanelClean = 1000;
```

and add to the success return literal, after `.touchbar_mode = ...`:

```c
      .touchbar_mode = doc["touchbar_mode"] | "",
      .panel_clean = u32PanelClean};
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd ~/scratch/trmnl-firmware && pio test -e native -f test_parse_api_display
```

Expected: all tests PASS, including the four pre-existing ones.

- [ ] **Step 7: Commit**

```bash
cd ~/scratch/trmnl-firmware
git add lib/trmnl/include/api_types.h lib/trmnl/src/parse_response_api_display.cpp test/test_parse_api_display/api_display.test.cpp
git commit -m "feat: parse panel_clean from the display response

New optional integer field. Absent or 0 means normal operation; a positive
value asks the device to run that many panel-recovery cycles instead of
displaying an image. Clamped to 1000 at parse time."
```

---

### Task 4: Device-side clean routine and dispatch

**Files:**
- Modify: `~/scratch/trmnl-firmware/src/display.cpp` (add `display_panel_clean`, in the `BOARD_X_CLASS` section)
- Modify: `~/scratch/trmnl-firmware/include/display.h` (declare it)
- Modify: `~/scratch/trmnl-firmware/src/bl.cpp:2008` (dispatch before the normal image path)

**Interfaces:**
- Consumes: `ApiDisplayResponse::panel_clean` from Task 3.
- Produces: `void display_panel_clean(int cycles);` — X-class only.

- [ ] **Step 1: Declare the function**

In `~/scratch/trmnl-firmware/include/display.h`, add near the other `display_*` declarations:

```c
#ifdef BOARD_X_CLASS
/**
 * @brief Run a bounded panel-recovery burst: alternating full black and full
 *        white clears with panel power held for the whole burst.
 * @param cycles number of black/white pairs to run
 * @return none
 */
void display_panel_clean(int cycles);
#endif // BOARD_X_CLASS
```

- [ ] **Step 2: Implement it**

In `~/scratch/trmnl-firmware/src/display.cpp`, inside the `#else // BOARD_X_CLASS` region that already contains the FastEPD code (the region defining `u8_graytable`), add:

```c
#ifdef BOARD_X_CLASS
void display_panel_clean(int cycles)
{
    if (cycles <= 0) return;
    Log_info("panel_clean: starting %d cycles", cycles);

    // Power the panel up once and hold it for the whole burst. Each power-up
    // costs a transient, and on this panel that transient is not uniform across
    // the gate-driver boundary -- so one power-up per burst rather than one per
    // cycle is the point of doing this device-side rather than server-side.
    bbep.einkPower(1);

    for (int i = 0; i < cycles; i++) {
        bbep.clearBlack(true);  // bKeepOn: do not drop power between passes
        bbep.clearWhite(true);
        // Equal black and white counts keep the burst DC-balanced.
        vTaskDelay(1); // yield so the task watchdog is fed
    }

    // Leave the panel at white, then release power.
    bbep.einkPower(0);
    Log_info("panel_clean: finished %d cycles", cycles);
}
#endif // BOARD_X_CLASS
```

- [ ] **Step 3: Dispatch it**

In `~/scratch/trmnl-firmware/src/bl.cpp`, the response handling currently begins:

```c
  if (special_function == SF_NONE)
  {
    uint64_t request_status = apiResponse.status;
```

Insert immediately before that `if`:

```c
#ifdef BOARD_X_CLASS
  // A panel-clean burst replaces the image path entirely for this poll: no
  // download, no repaint of content. Nothing is persisted, so a reboot, a lost
  // network, or a server that stops asking all return the device to normal
  // operation by themselves.
  if (apiResponse.panel_clean > 0)
  {
    Log.info("%s [%d]: panel_clean requested: %d cycles\r\n", __FILE__, __LINE__,
             (int)apiResponse.panel_clean);
    display_panel_clean((int)apiResponse.panel_clean);
    refreshInterval.applyServerRate(apiResponse.refresh_rate);
    return HTTPS_NO_ERR;
  }
#endif // BOARD_X_CLASS
```

The enclosing function is `https_request_err_e handleApiDisplayResponse(ApiDisplayResponse &apiResponse)` (`src/bl.cpp:1992`). Its own default is `https_request_err_e result = HTTPS_NO_ERR;`, and `HTTPS_NO_ERR` is already in the not-an-error list at `src/bl.cpp:1256`, so it is the correct "handled, nothing went wrong" return. `applyServerRate` is the same call the normal path uses at `src/bl.cpp:2095`.

- [ ] **Step 4: Build**

```bash
cd ~/scratch/trmnl-firmware && pio run -e TRMNL_X_LOCAL
```

Expected: `SUCCESS`.

- [ ] **Step 5: Re-run the parser tests to confirm nothing regressed**

```bash
cd ~/scratch/trmnl-firmware && pio test -e native -f test_parse_api_display
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
cd ~/scratch/trmnl-firmware
git add include/display.h src/display.cpp src/bl.cpp
git commit -m "feat: server-triggerable panel-clean burst on X-class boards

display_panel_clean() powers the panel up once, alternates full black and
full white clears with power held, and powers down. Holding power across the
burst avoids the per-update radio overhead and the download entirely, and
takes one power-up transient for the whole burst instead of one per cycle.

Nothing is persisted, so a device cannot get stuck in clean mode."
```

- [ ] **Step 7: Prepare the clean PR branch**

The `local/validation` commit must not reach upstream.

```bash
cd ~/scratch/trmnl-firmware
git checkout -b pr/panel-clean 6bff55b
git cherry-pick <task-3-commit-sha> <task-4-commit-sha>
git log --oneline 6bff55b..HEAD
git diff 6bff55b..HEAD -- platformio.ini include/config.h
```

The last command must print **nothing**. If it prints anything, the local build env or the version bump has leaked into the PR branch — remove it before proceeding.

---

## GATE B — hardware: does the clean mode work, and how fast is it?

**This gate is the user's. No subagent may perform it.**

1. Flash the feature build (app only, no erase):
   ```bash
   cd ~/scratch/trmnl-firmware && git checkout feat/panel-clean
   pio run -e TRMNL_X_LOCAL -t upload --upload-port <PORT>
   ```
2. Hand-serve one response carrying `panel_clean` to confirm the path works end to end, before byonk knows how to send it. Confirm from the serial log that `panel_clean: starting N cycles` and `panel_clean: finished N cycles` both appear, that no watchdog reset occurs, and that the device polls normally afterwards.
3. **Measure the rate.** Time a burst of 200 cycles from the two log lines and record cycles per minute in `~/scratch/panel-evidence/clean-rate.txt`. This number sets whether 200 is the right `cycles_per_burst` default, and it is the comparison point against the old noise-screen approach.
4. Keep the device on USB power.

**PASS:** burst completes, device resumes, rate recorded. Proceed to Task 5.
**FAIL:** report the serial log. Do not start Task 5.

---

### Task 5: Byonk recovery session model and registry operations

**Files:**
- Modify: `/Users/oetiker/checkouts/byonk/src/models/device.rs:73-95`
- Modify: `/Users/oetiker/checkouts/byonk/src/services/device_registry.rs`
- Test: inline `#[cfg(test)] mod tests` in `src/services/device_registry.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct RecoverySession { pub cycles_total: u32, pub cycles_remaining: u32, pub cycles_per_burst: u32, pub started_at: chrono::DateTime<chrono::Utc> }`
  - `Device::recovery: Option<RecoverySession>`
  - Three async trait methods on `DeviceRegistry`: `start_recovery`, `cancel_recovery`, `take_recovery_burst`.

- [ ] **Step 1: Write the failing tests**

Append to the existing `#[cfg(test)] mod tests` in `src/services/device_registry.rs`:

```rust
    async fn seeded() -> (InMemoryRegistry, DeviceId) {
        let reg = InMemoryRegistry::new();
        let dev = Device::new(DeviceId::new("AA:BB:CC:DD:EE:FF"), "x".into(), "1.8.15".into());
        let id = dev.device_id.clone();
        reg.upsert(dev).await.unwrap();
        (reg, id)
    }

    #[tokio::test]
    async fn take_recovery_burst_is_none_without_a_session() {
        let (reg, id) = seeded().await;
        assert_eq!(reg.take_recovery_burst(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn burst_is_capped_by_cycles_per_burst_and_decrements() {
        let (reg, id) = seeded().await;
        reg.start_recovery(&id, 500, 200).await.unwrap();
        assert_eq!(reg.take_recovery_burst(&id).await.unwrap(), Some(200));
        assert_eq!(reg.take_recovery_burst(&id).await.unwrap(), Some(200));
        // Final burst is the remainder, not a full burst.
        assert_eq!(reg.take_recovery_burst(&id).await.unwrap(), Some(100));
        // Session is dropped once it is spent.
        assert_eq!(reg.take_recovery_burst(&id).await.unwrap(), None);
        let dev = reg.find_by_id(&id).await.unwrap().unwrap();
        assert!(dev.recovery.is_none());
    }

    #[tokio::test]
    async fn start_recovery_clamps_both_arguments() {
        let (reg, id) = seeded().await;
        reg.start_recovery(&id, 9_999_999, 9_999).await.unwrap();
        let dev = reg.find_by_id(&id).await.unwrap().unwrap();
        let s = dev.recovery.unwrap();
        assert_eq!(s.cycles_total, 1_000_000);
        assert_eq!(s.cycles_per_burst, 1_000);
    }

    #[tokio::test]
    async fn start_recovery_replaces_an_existing_session() {
        let (reg, id) = seeded().await;
        reg.start_recovery(&id, 500, 200).await.unwrap();
        reg.take_recovery_burst(&id).await.unwrap();
        reg.start_recovery(&id, 800, 100).await.unwrap();
        let s = reg.find_by_id(&id).await.unwrap().unwrap().recovery.unwrap();
        assert_eq!(s.cycles_remaining, 800);
        assert_eq!(s.cycles_per_burst, 100);
    }

    #[tokio::test]
    async fn cancel_recovery_reports_whether_a_session_was_active() {
        let (reg, id) = seeded().await;
        assert!(!reg.cancel_recovery(&id).await.unwrap());
        reg.start_recovery(&id, 500, 200).await.unwrap();
        assert!(reg.cancel_recovery(&id).await.unwrap());
        assert_eq!(reg.take_recovery_burst(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn start_recovery_on_unknown_device_is_an_error() {
        let reg = InMemoryRegistry::new();
        let missing = DeviceId::new("11:22:33:44:55:66");
        assert!(reg.start_recovery(&missing, 500, 200).await.is_err());
    }
```

`DeviceId::new` is **infallible** (`src/models/device.rs:11`): it takes `impl Into<String>` and returns `Self`, so there is nothing to unwrap and no error branch to write.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test --lib device_registry
```

Expected: compile errors — no method `start_recovery`, no field `recovery`.

- [ ] **Step 3: Add the session type and the device field**

In `src/models/device.rs`, above `pub struct Device`:

```rust
/// An in-flight panel-recovery run for one device.
///
/// Deliberately in-memory, like the rest of `Device`'s runtime state: a byonk
/// restart cancels the run rather than silently continuing one the operator has
/// forgotten about. Resuming costs a single API call.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RecoverySession {
    /// Cycles requested when the session started.
    pub cycles_total: u32,
    /// Cycles still owed to the device.
    pub cycles_remaining: u32,
    /// Maximum cycles handed out in a single burst.
    pub cycles_per_burst: u32,
    pub started_at: chrono::DateTime<chrono::Utc>,
}
```

Add to `Device`, after `pub rssi: Option<i32>,`:

```rust
    /// Active panel-recovery session, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoverySession>,
```

and to `Device::new`, after `rssi: None,`:

```rust
            recovery: None,
```

Export `RecoverySession` from `src/models/mod.rs` alongside `Device`.

- [ ] **Step 4: Add the registry operations**

In `src/services/device_registry.rs`, add to the `DeviceRegistry` trait:

```rust
    /// Start or replace a recovery session. `cycles` is clamped to 1_000_000 and
    /// `cycles_per_burst` to 1_000, matching the firmware-side clamp.
    async fn start_recovery(
        &self,
        device_id: &DeviceId,
        cycles: u32,
        cycles_per_burst: u32,
    ) -> Result<(), ApiError>;

    /// Cancel any active session. Returns true if one was active.
    async fn cancel_recovery(&self, device_id: &DeviceId) -> Result<bool, ApiError>;

    /// Atomically take the next burst, decrementing the session and dropping it
    /// when spent. Returns None when no session is active.
    async fn take_recovery_burst(&self, device_id: &DeviceId) -> Result<Option<u32>, ApiError>;
```

and to `impl DeviceRegistry for InMemoryRegistry`:

```rust
    async fn start_recovery(
        &self,
        device_id: &DeviceId,
        cycles: u32,
        cycles_per_burst: u32,
    ) -> Result<(), ApiError> {
        let cycles = cycles.min(1_000_000);
        let cycles_per_burst = cycles_per_burst.clamp(1, 1_000);
        let mut devices = self.devices.write().await;
        let device = devices
            .get_mut(device_id)
            .ok_or(ApiError::DeviceNotFound)?;
        device.recovery = Some(RecoverySession {
            cycles_total: cycles,
            cycles_remaining: cycles,
            cycles_per_burst,
            started_at: chrono::Utc::now(),
        });
        Ok(())
    }

    async fn cancel_recovery(&self, device_id: &DeviceId) -> Result<bool, ApiError> {
        let mut devices = self.devices.write().await;
        let device = devices
            .get_mut(device_id)
            .ok_or(ApiError::DeviceNotFound)?;
        Ok(device.recovery.take().is_some())
    }

    async fn take_recovery_burst(&self, device_id: &DeviceId) -> Result<Option<u32>, ApiError> {
        // Held under the write lock for the whole read-modify-write so two
        // concurrent polls from the same device cannot both take the last burst.
        let mut devices = self.devices.write().await;
        let Some(device) = devices.get_mut(device_id) else {
            return Ok(None);
        };
        let Some(session) = device.recovery.as_mut() else {
            return Ok(None);
        };
        let burst = session.cycles_per_burst.min(session.cycles_remaining);
        session.cycles_remaining -= burst;
        if session.cycles_remaining == 0 {
            device.recovery = None;
        }
        Ok(Some(burst))
    }
```

Import `RecoverySession` at the top of the file. `ApiError::DeviceNotFound` is a **unit** variant (`src/error.rs:15`) mapping to HTTP 404 — it takes no payload, so do not pass it a string.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd /Users/oetiker/checkouts/byonk && cargo test --lib device_registry
```

Expected: all six new tests PASS.

- [ ] **Step 6: Full verification**

```bash
cd /Users/oetiker/checkouts/byonk
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --lib
```

Expected: clippy clean; all lib tests pass. Other implementors of `DeviceRegistry` (if any exist, including test doubles) must be updated for the three new trait methods — the compiler will name them.

- [ ] **Step 7: Commit**

```bash
cd /Users/oetiker/checkouts/byonk
git add src/models/device.rs src/models/mod.rs src/services/device_registry.rs
git diff --cached --stat
git commit -m "feat(recovery): per-device panel-recovery sessions in the registry

Sessions are in-memory like the rest of Device's runtime state, so a restart
cancels a run rather than silently continuing one nobody remembers starting.

take_recovery_burst does its read-modify-write under the write lock so two
concurrent polls cannot both claim the last burst."
```

`git diff --cached --stat` must list exactly the three files above. If it lists `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md` or `tools/capture-config.yaml`, unstage them — they belong to an unrelated task.

---

### Task 6: Admin endpoints to start and cancel a session

**Files:**
- Modify: `/Users/oetiker/checkouts/byonk/src/api/admin/write.rs`
- Modify: `/Users/oetiker/checkouts/byonk/src/api/admin/mod.rs:52`
- Test: inline `#[cfg(test)] mod tests` in `src/api/admin/write.rs`

**Interfaces:**
- Consumes: `start_recovery`, `cancel_recovery` from Task 5.
- Produces: `POST /api/admin/devices/{key}/recover`, `DELETE /api/admin/devices/{key}/recover`, and `pub struct RecoverStart { cycles: u32, cycles_per_burst: Option<u32> }`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `src/api/admin/write.rs`:

```rust
    #[test]
    fn recover_start_defaults_cycles_per_burst_to_200() {
        let body: RecoverStart = serde_json::from_str(r#"{"cycles":5000}"#).unwrap();
        assert_eq!(body.cycles_per_burst.unwrap_or(200), 200);
    }

    #[test]
    fn recover_start_accepts_an_explicit_burst_size() {
        let body: RecoverStart = serde_json::from_str(r#"{"cycles":5000,"cycles_per_burst":50}"#).unwrap();
        assert_eq!(body.cycles_per_burst, Some(50));
    }

    #[test]
    fn firmware_supports_panel_clean_requires_1_8_15() {
        assert!(!firmware_supports_panel_clean("1.8.14"));
        assert!(firmware_supports_panel_clean("1.8.15"));
        assert!(firmware_supports_panel_clean("1.9.0"));
        assert!(firmware_supports_panel_clean("2.0.0"));
        // Unparseable versions are refused rather than assumed capable.
        assert!(!firmware_supports_panel_clean(""));
        assert!(!firmware_supports_panel_clean("unknown"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /Users/oetiker/checkouts/byonk && cargo test --lib admin::write
```

Expected: compile errors — `RecoverStart` and `firmware_supports_panel_clean` are not defined.

- [ ] **Step 3: Implement the request body, the version gate and the handlers**

Add to `src/api/admin/write.rs`:

```rust
/// Minimum firmware version that understands `panel_clean`.
///
/// Provisional until TRMNL tags a release containing the field. Below this,
/// a device would parse `panel_clean` as 0 and then receive a response with no
/// image, leaving it idle — so byonk refuses to start a session instead.
const MIN_PANEL_CLEAN_FIRMWARE: (u32, u32, u32) = (1, 8, 15);

/// True if `version` is at least `MIN_PANEL_CLEAN_FIRMWARE`.
/// Anything unparseable is treated as unsupported.
pub(crate) fn firmware_supports_panel_clean(version: &str) -> bool {
    let mut parts = version.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
    });
    let (Some(Ok(major)), Some(Ok(minor)), Some(Ok(patch))) =
        (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    (major, minor, patch) >= MIN_PANEL_CLEAN_FIRMWARE
}

/// Body of `POST /api/admin/devices/{key}/recover`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecoverStart {
    /// Total recovery cycles to run. Clamped to 1_000_000.
    pub cycles: u32,
    /// Cycles per burst. Clamped to 1_000. Defaults to 200.
    #[serde(default)]
    pub cycles_per_burst: Option<u32>,
}

pub async fn start_device_recovery(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RecoverStart>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let device_id = DeviceId::new(&key);
    let device = state
        .registry
        .find_by_id(&device_id)
        .await?
        .ok_or(ApiError::DeviceNotFound)?;
    if !firmware_supports_panel_clean(&device.firmware_version) {
        return Err(ApiError::Conflict(format!(
            "device reports firmware {}, but panel recovery needs {}.{}.{} or later",
            device.firmware_version,
            MIN_PANEL_CLEAN_FIRMWARE.0,
            MIN_PANEL_CLEAN_FIRMWARE.1,
            MIN_PANEL_CLEAN_FIRMWARE.2
        )));
    }
    state
        .registry
        .start_recovery(&device_id, body.cycles, body.cycles_per_burst.unwrap_or(200))
        .await?;
    let device = state.registry.find_by_id(&device_id).await?;
    Ok(Json(serde_json::json!({
        "recovery": device.and_then(|d| d.recovery)
    })))
}

pub async fn cancel_device_recovery(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let device_id = DeviceId::new(&key);
    let was_active = state.registry.cancel_recovery(&device_id).await?;
    Ok(Json(serde_json::json!({ "cancelled": was_active })))
}
```

Adjust imports as the compiler requires. `AppState.registry` is `Arc<InMemoryRegistry>` (`src/server.rs:76`) — a concrete type, so the new trait methods are callable directly with no dispatch changes.

- [ ] **Step 4: Register the routes**

In `src/api/admin/mod.rs`, after the `/devices/{key}/preview` route:

```rust
        .route(
            "/devices/{key}/recover",
            post(write::start_device_recovery).delete(write::cancel_device_recovery),
        )
```

- [ ] **Step 5: Expose session status on the existing device read**

The spec requires session status to appear in the device read rather than in a
third endpoint. `list_devices` (`src/api/admin/read.rs:130`) builds an explicit
`AdminDevice` projection, so the new field does **not** appear automatically.

Add to `struct AdminDevice` in `src/api/admin/read.rs`:

```rust
    /// Active panel-recovery session, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<crate::models::RecoverySession>,
```

and populate it in the push inside `list_devices`, alongside the other fields:

```rust
            recovery: d.recovery.clone(),
```

If a second `AdminDevice` is constructed further down the same function for
config-only devices that the registry has never seen, give it `recovery: None`
— such a device cannot have a session, because sessions are keyed on a device
the registry knows.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd /Users/oetiker/checkouts/byonk && cargo test --lib admin::write
```

Expected: the three new tests PASS.

- [ ] **Step 7: Full verification**

```bash
cd /Users/oetiker/checkouts/byonk
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --lib
```

- [ ] **Step 8: Commit**

```bash
cd /Users/oetiker/checkouts/byonk
git add src/api/admin/write.rs src/api/admin/mod.rs src/api/admin/read.rs
git diff --cached --stat
git commit -m "feat(recovery): admin endpoints to start and cancel a session

POST and DELETE on /api/admin/devices/{key}/recover. Starting is refused for
firmware below 1.8.15, which would parse panel_clean as 0 and then sit idle on
an imageless response."
```

---

### Task 7: Send bursts in the display response

**Files:**
- Modify: `/Users/oetiker/checkouts/byonk/src/api/display.rs:1276-1307` (`DisplayJsonResponse`)
- Modify: `/Users/oetiker/checkouts/byonk/src/api/display.rs:661` and `:1138` (both construction sites)
- Modify: `/Users/oetiker/checkouts/byonk/src/api/display.rs` (burst short-circuit, before rendering)
- Test: inline `#[cfg(test)] mod tests` in `src/api/display.rs`

**Interfaces:**
- Consumes: `take_recovery_burst` from Task 5.
- Produces: `DisplayJsonResponse::panel_clean: Option<u32>`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `src/api/display.rs`:

```rust
    #[test]
    fn panel_clean_is_omitted_when_no_session_is_active() {
        let r = DisplayJsonResponse {
            status: 0,
            image_url: Some("http://example/x.png".into()),
            filename: "abc".into(),
            update_firmware: false,
            firmware_url: None,
            refresh_rate: 300,
            reset_firmware: false,
            temperature_profile: Some("default".into()),
            special_function: None,
            maximum_compatibility: None,
            panel_clean: None,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("panel_clean").is_none());
        assert!(v.get("image_url").is_some());
    }

    #[test]
    fn a_burst_response_carries_panel_clean_and_no_image() {
        let r = DisplayJsonResponse {
            status: 0,
            image_url: None,
            filename: "recovery".into(),
            update_firmware: false,
            firmware_url: None,
            refresh_rate: 1,
            reset_firmware: false,
            temperature_profile: None,
            special_function: None,
            maximum_compatibility: None,
            panel_clean: Some(200),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["panel_clean"], 200);
        assert_eq!(v["refresh_rate"], 1);
        assert!(v.get("image_url").is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /Users/oetiker/checkouts/byonk && cargo test --lib display
```

Expected: compile error — `DisplayJsonResponse` has no field `panel_clean`.

- [ ] **Step 3: Add the field**

In `src/api/display.rs`, add to `DisplayJsonResponse` after `maximum_compatibility`:

```rust
    /// Number of device-side black/white recovery cycles to run **instead of**
    /// displaying an image.
    ///
    /// Only sent while an admin-started recovery session is active for this
    /// device. When present, `image_url` is omitted and byonk skips rendering
    /// entirely: the device runs the burst with panel power held, which costs
    /// one power-up transient for the whole burst rather than one per cycle.
    ///
    /// Requires firmware >= 1.8.15. Clamped to 1000, matching the firmware's
    /// own parse-time clamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel_clean: Option<u32>,
```

Add `panel_clean: None` to both existing construction sites (around lines 661 and 1138) and to the two existing test literals in the file.

- [ ] **Step 4: Short-circuit the render when a burst is due**

In the main display handler, immediately after `registry.upsert(device.clone()).await?;` (`src/api/display.rs:714`) and **before** the screen lookup that begins at `src/api/display.rs:729`, insert:

```rust
    // A recovery burst replaces content for this poll. Take it before rendering:
    // the device ignores the image entirely, so rendering one would be wasted
    // work. refresh_rate is 1 because every poll costs ~6-7 s of fixed radio
    // overhead and any gap beyond that is dead time during recovery.
    if let Some(cycles) = state.registry.take_recovery_burst(&device_id).await? {
        tracing::info!(device = %device_id_str, cycles, "Sending panel-clean burst");
        return Ok(Json(DisplayJsonResponse {
            status: 0,
            image_url: None,
            filename: "recovery".to_string(),
            update_firmware: false,
            firmware_url: None,
            refresh_rate: 1,
            reset_firmware: false,
            temperature_profile: None,
            special_function: None,
            maximum_compatibility: None,
            panel_clean: Some(cycles),
        })
        .into_response());
    }
```

The construction site at `src/api/display.rs:1138` is in this same function and uses `.into_response()`, so the snippet above matches it. The site at line 661 is a different branch, for devices not yet registered, and returns `Ok(Json(...))` directly — leave it alone apart from adding `panel_clean: None` to its literal.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd /Users/oetiker/checkouts/byonk && cargo test --lib display
```

- [ ] **Step 6: Full verification**

```bash
cd /Users/oetiker/checkouts/byonk
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --lib
```

- [ ] **Step 7: Commit**

```bash
cd /Users/oetiker/checkouts/byonk
git add src/api/display.rs
git diff --cached --stat
git commit -m "feat(recovery): send panel_clean bursts in the display response

While a session is active the response carries panel_clean, omits image_url,
and skips rendering entirely -- the device ignores content during a burst.
refresh_rate is pinned to 1: each poll costs ~6-7 s of fixed radio overhead,
so any gap beyond that is dead time."
```

---

### Task 8: Documentation and changelog

**Files:**
- Create: `/Users/oetiker/checkouts/byonk/docs/src/guide/panel-recovery.md`
- Modify: `/Users/oetiker/checkouts/byonk/docs/src/SUMMARY.md`
- Modify: `/Users/oetiker/checkouts/byonk/CHANGES.md`

**Interfaces:**
- Consumes: the endpoints from Task 6 and the response field from Task 7.
- Produces: nothing code depends on.

- [ ] **Step 1: Read the surrounding docs conventions**

```bash
cd /Users/oetiker/checkouts/byonk && cat docs/src/SUMMARY.md && head -40 CHANGES.md && ls docs/src/guide/
```

Match the existing heading style, tone and nav placement. Place the new page in whichever section neighbours device configuration.

- [ ] **Step 2: Write the guide page**

`docs/src/guide/panel-recovery.md` must cover, in prose matching the surrounding docs:

- What burn-in is on an e-paper panel, and why refreshing often is not protection when the pixels do not change.
- That recovery is asymptotic: fast at first, then slow, and deep cases may never clear completely.
- Starting a session: `POST /api/admin/devices/{key}/recover` with `{"cycles": 20000, "cycles_per_burst": 200}`, and what each field does.
- Cancelling: `DELETE` on the same path.
- That sessions are in-memory, so **restarting byonk cancels a running session**.
- That the device shows nothing but black/white flashing for the whole session and serves no content.
- The firmware requirement (1.8.15 or later) and the error returned below it.
- That the device should be on USB power for a long session.
- That warming the panel to roughly 30-35 degrees C accelerates recovery, because the panel's waveform has no temperature compensation and warm ink moves further per pass. Note the panel datasheet's operating maximum should be respected.

- [ ] **Step 3: Add it to the nav**

Add the page to `docs/src/SUMMARY.md` in the matching section.

- [ ] **Step 4: Add the changelog entry**

Under the `Unreleased` heading in `CHANGES.md`, describing only what a user sees:

```markdown
- **Panel recovery sessions** — an admin can now run a bounded burn-in recovery
  run on a device: `POST /api/admin/devices/{key}/recover`. The device flashes
  full black and full white with panel power held for the whole burst, which is
  far more effective than serving alternating images, and stops on its own when
  the requested cycles are spent. Requires device firmware 1.8.15 or later.
```

Keep tooling, CI and internal refactoring out of `CHANGES.md`.

- [ ] **Step 5: Build the docs**

```bash
cd /Users/oetiker/checkouts/byonk && make docs
```

Expected: builds without warnings about the new page. Read the output; do not rely on the exit code.

- [ ] **Step 6: Commit**

```bash
cd /Users/oetiker/checkouts/byonk
git add docs/src/guide/panel-recovery.md docs/src/SUMMARY.md CHANGES.md
git diff --cached --stat
git commit -m "docs(recovery): guide for panel recovery sessions"
```

---

## After the plan

1. Open PR 1 against `bitbank2/FastEPD` from `feat/upseq-carta1300`, citing epdiy `epd_board_v7_103.c:216-222` and `tps65185.c:122`, and including the Gate A before/after numbers.
2. Open PR 2 against `usetrmnl/trmnl-firmware` from `pr/panel-clean`, including the Gate B rate measurement and the parser tests.
3. Start a real recovery session and begin the healing curve described in spec section 7.4.
4. Open the byonk PR from `feat/panel-clean-recovery`.
