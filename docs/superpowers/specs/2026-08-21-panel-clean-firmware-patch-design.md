# Design — TRMNL X panel-clean firmware patch and byonk recovery sessions

**Date:** 2026-08-21
**Status:** design approved in chat, awaiting spec review
**Scope:** two upstream firmware PRs plus a byonk feature

---

## 1. Problem

A TRMNL X (E Ink ED103MC2, Carta 1300, 1872x1404, 16-bit parallel, VCOM -11.00 V)
running firmware 1.8.14 developed two distinct defects:

1. **Burn-in** from weeks of near-static content. Largely treated by hours of hard
   cycling, but with a residue that has plateaued.
2. **A sharp horizontal split** at mid-panel that was *not* present in any
   burnt-in image, with a visible chip-on-film bond at exactly that row.

This design addresses the split's cause and gives byonk a proper tool for the
burn-in, replacing the current ad-hoc "noise screen on a short refresh" approach.

---

## 2. Evidence

A 240 fps slow-motion recording of one update (12.433 s of video in a 30 fps
container = **1.554 s of real panel time**) was analysed by extracting a
104-band vertical luminance profile per frame.

| Real time | Step at band 55 | State |
|---|---|---|
| 0 - 170 ms | -2 | settled, previous image |
| 175 ms | +7 | panel powers up |
| **187 ms** | **+21** | **first flash, split at full strength** |
| 217 ms | +8 | decaying |
| 250 ms | +2 | nearly gone |
| 280 ms | 0 | gone |
| 400, 800, 1300, 1400 ms | 0 or +/-1 | four further flashes, no split |

The boundary sits at band 55 of 104 = **52.9% down the panel = row ~743 of 1404**,
which is where the second gate-driver chip takes over, and where the user sees a
bond entering the panel from the side edge.

**Not a camera artifact.** The step stays pinned to one band across 16 consecutive
video frames (a rolling-shutter split would drift), and it decays smoothly from
+21 to 0 while the panel is already static (a camera artifact cannot fade on a
still scene). The decay is ink relaxing.

**Conclusion.** The two halves of the panel receive different drive on the first
frame after power-up, and only then. The firmware calls `bbep.einkPower(0)` after
every update (`src/display.cpp:531`, `:2768`), so this differential kick occurs
once per refresh — roughly 4 500 times a day at the measured ~189 cycles/hour.

**Suspected cause.** epdiy programs the TPS65185 power-up sequence registers
before power-up for this exact panel:

```c
// epdiy/src/board/epd_board_v7_103.c:216-222
if (display->display_type & DISPLAY_UPSEQ_MC2) {
    vTaskDelay(3);
    tps_set_upseq_carta1300();   // UPSEQ0 (0x09) = 0xE1, UPSEQ1 (0x0A) = 0xAA
}
config_reg.pwrup = true;         // only THEN power up
```

FastEPD has the equivalent writes **commented out**, and positioned *after* PWRUP
and after the PWRGOOD wait (`FastEPD.inl:806-830`). Because the TPS65185 reloads
its defaults whenever WAKEUP is deasserted — which happens on every power-down —
those writes would take effect on no power-up at all even if uncommented in place.

Wrong rail ordering leaves the gate lines undefined while the source rails are
live. The two gate-driver chips exit that state at slightly different moments, so
the first frame lands unevenly, with a hard edge at the chip boundary.

This remains a **hypothesis**. Section 7 defines the experiment that settles it.

---

## 3. Goals

- Stop the per-power-up differential kick on Carta 1300 panels.
- Give byonk a way to run an efficient, bounded panel-recovery burst on a device.
- Land both changes upstream as merge-ready PRs, not bug reports.

## 4. Non-goals

- Removing the existing accumulated split. No firmware change can; only drive can.
- A custom recovery waveform (`setCustomMatrix`). Deferred until we can measure
  whether it beats repeated clears. See section 10.
- Byonk deciding on its own when a device needs recovery. Deferred; needs
  content-similarity tracking and a policy.
- Changing panel drive voltage. FastEPD has a commented-out block adjusting
  +/-15 V to +/-14.25 V; it stays commented. No evidence supports touching it.
- Inverse-image (ghost-negative) recovery content. That is byonk-side content
  design, not a firmware feature, and is tracked separately.

---

## 5. Approach

**Two fully decoupled PRs.** FastEPD receives only the power-up fix. All feature
work lands in `usetrmnl/trmnl-firmware` using FastEPD's existing public API
(`clearBlack`, `clearWhite`, both taking a `bKeepOn` flag).

Rejected alternatives:

- *Put `panelClean()` in FastEPD.* Would benefit every FastEPD board, but asks
  bitbank2 to accept and maintain new API surface, and blocks TRMNL until FastEPD
  cuts a release and the pin at `platformio.ini:497` is bumped. Two maintainers in
  series.
- *Ship a custom recovery waveform.* Strictly harder to justify upstream with no
  comparative measurement behind it.

The two PRs have very different risk profiles: the FastEPD change is a small,
obviously-correct fix mirroring proven upstream code, while the TRMNL change is a
feature with a protocol addition. Coupling them makes the fix wait on the feature.

---

## 6. Detailed design

### 6.1 PR 1 — `bitbank2/FastEPD`: correct the Carta 1300 power-up sequence

Reference: currently pinned by TRMNL at commit
`855ce9a42c0be5cd8ce2f2425483a8271a04668d` (`platformio.ini:497`).

1. Add `#define BB_PANEL_FLAG_UPSEQ_MC2 0x10` in `src/FastEPD.h`. Existing flags
   are `NONE 0x00`, `MIRROR_X 0x01`, `MIRROR_Y 0x02`, `SLOW_SPH 0x04`,
   `DARK 0x08`; 0x10 is free.
2. Add the flag to the `BB_PANEL_TRMNL_X` entry in the panel table
   (`src/FastEPD.inl`, the 1872x1404 row), alongside its existing
   `BB_PANEL_FLAG_MIRROR_X | BB_PANEL_FLAG_SLOW_SPH`.
3. In `EPDiyV7EinkPower()` (`src/FastEPD.inl:806`), **move** the two UPSEQ writes
   from their commented position to *before* `bbepPCA9535DigitalWrite(11, 1)`
   (PWRUP), guarded by the new flag:

   ```c
   if (pState->panelDef.flags & BB_PANEL_FLAG_UPSEQ_MC2) {
       ucTemp[0] = TPS_REG_UPSEQ0; ucTemp[1] = 0xe1; bbepI2CWrite(0x68, ucTemp, 2);
       ucTemp[0] = TPS_REG_UPSEQ1; ucTemp[1] = 0xaa; bbepI2CWrite(0x68, ucTemp, 2);
   }
   bbepPCA9535DigitalWrite(11, 1);   // PWRUP on
   ```

   The WAKEUP assert must already have happened, matching epdiy's ordering
   (wakeup, short delay, UPSEQ writes, then pwrup).
4. Leave the voltage-adjust block commented out.

Behaviour is unchanged for every panel that does not set the flag. No public API
change.

**PR text** cites epdiy `epd_board_v7_103.c:216-222` and `tps65185.c:122` as the
reference implementation, explains the reload-on-WAKEUP-deassert reason the writes
must precede PWRUP, and includes the before/after measurement from section 7.

### 6.2 PR 2 — `usetrmnl/trmnl-firmware`: server-triggerable panel clean

**Why a new response field rather than a `special_function`.** The special-function
enum is a closed list of seven values (`lib/trmnl/include/special_function.h`), and
the entire block that loads and acts on a stored special function sits inside
`#else // BOARD_TRMNL_X` (`src/bl.cpp:874`). On an X the global stays `SF_NONE`
forever and the handler at `src/bl.cpp:2130` never runs. A top-level field is the
only path that reaches X-class hardware.

**Protocol.** One new integer field in the `/api/display` response:

```json
{ "status": 0, "panel_clean": 200, "refresh_rate": 5 }
```

- Absent or `0` means normal operation. Backward compatible: existing servers are
  unaffected, and ArduinoJson yields 0 for a missing key.
- When `panel_clean > 0`, the device runs the burst **instead of** the image path
  and ignores `image_url` / `filename`.

**Parsing.** `lib/trmnl/src/parse_response_api_display.cpp` and
`lib/trmnl/include/api_types.h` gain `uint32_t panel_clean`, defaulted to 0 in the
error-return struct literal as well as the success path, and clamped to a maximum
(1000) at parse time.

**Clean routine.** `display_panel_clean(int cycles)` in `src/display.cpp`, guarded
to X-class (`BOARD_X_CLASS`), where `clearBlack`/`clearWhite` exist and there is no
partial-refresh path to conflict with:

- Power the panel up once.
- Loop `cycles` times: `bbep.clearBlack(true); bbep.clearWhite(true);` — equal
  counts, so the sequence is DC-balanced by construction.
- End on white, then `bbep.einkPower(0)`.
- Yield to the scheduler between cycles so the task watchdog is fed.

Holding power across the burst is the point: it avoids the ~6 s of radio overhead
per cycle measured on this device, skips the download entirely, and takes **one**
power-up transient for the whole burst instead of one per cycle.

**Integration.** In `src/bl.cpp`, when a display response carries
`panel_clean > 0`, call `display_panel_clean()` and then resume normal polling.
Nothing is persisted to preferences, so a device can never get stuck in clean mode
— a lost server, a reboot, or a dropped network all return it to normal operation.

**Tests.** Extend `test/test_parse_api_display/api_display.test.cpp` (runs in
`[env:native]`, no hardware) for: field present, field absent, field above the
clamp, field non-numeric.

### 6.3 byonk — admin-initiated recovery sessions

**State.** `src/models/device.rs` gains:

```rust
pub struct RecoverySession {
    pub cycles_total: u32,
    pub cycles_remaining: u32,
    pub cycles_per_burst: u32,
    pub started_at: DateTime<Utc>,
}
```

held as `pub recovery: Option<RecoverySession>` on `Device`, in the existing
`InMemoryRegistry` (`src/services/device_registry.rs:22`).

**In-memory, deliberately.** It matches how byonk already treats per-device runtime
state (`last_seen`, `battery_voltage`, `rssi` are all in-memory and unpersisted),
and it fails safe: a restart cancels the session rather than silently continuing
one the operator has forgotten about. Accepted cost: an add-on restart mid-run
requires one API call to resume.

**Nothing goes into `config.yaml`.** A session is runtime state, not configuration.
This also keeps it clear of the settings-preservation logic in
`apply_device_patch` (`src/api/admin/write.rs`).

**Admin API**, token-gated under `/api/admin/*`:

| Endpoint | Behaviour |
|---|---|
| `POST /api/admin/devices/{key}/recover` | Start or replace a session. Body: `{ "cycles": 20000, "cycles_per_burst": 200 }`. Returns the session. |
| `DELETE /api/admin/devices/{key}/recover` | Cancel immediately. |

Session status is folded into the **existing** device read response rather than a
third endpoint.

`cycles_per_burst` is clamped to **1000**, matching the firmware-side clamp in
6.2, so the two limits cannot drift apart. `cycles` is clamped to 1 000 000.
Both default sensibly when omitted: `cycles_per_burst` to 200.

**Display response.** `DisplayJsonResponse` (`src/api/display.rs:1276`) gains:

```rust
/// Number of black/white recovery cycles the device should run instead of
/// displaying an image. Omitted when no recovery session is active.
#[serde(skip_serializing_if = "Option::is_none")]
pub panel_clean: Option<u32>,
```

documented in the same style as the existing `min_png_bytes` field. When a device
with an active session polls, byonk:

- sets `panel_clean = min(cycles_per_burst, cycles_remaining)`,
- decrements `cycles_remaining` and drops the session when it reaches 0,
- **skips rendering entirely** and omits `image_url` (already `Option` with
  `skip_serializing_if`, so no struct surgery),
- sends a short `refresh_rate` so bursts follow each other closely.

**Firmware-version gate.** Old firmware parses `panel_clean` as 0 and would then
receive a response with no image, leaving it idle. Byonk therefore refuses to start
a session for a device below the minimum firmware version, using the
`firmware_version` it already tracks on `Device`, and `POST` returns a clear error
explaining why. The exact minimum version is fixed once PR 2 is tagged; until then
the validation build is identified by its reported version string.

**Deferred:** MCP tools for start/cancel. The admin API is sufficient to prove the
mechanism; tools follow once the shape has settled.

**Also in scope:** a `CHANGES.md` entry under Unreleased and a docs page under
`docs/src/`, as this is a user-visible feature.

---

## 7. Validation

### 7.1 The decisive measurement

Baseline is already captured: **+21 grey levels at band 55, real t ~187 ms**.
After flashing the patched firmware, film again and re-run the identical analysis.

**Pass:** the step falls to the settled-state noise floor, |step| <= 2.
**Fail:** the step persists, which points at a hardware defect in one gate-driver
bond rather than a power-sequencing fault.

The analysis is packaged as a standalone script kept **with the PR evidence, not in
byonk's source tree** — it is a firmware diagnostic, and byonk should not grow a
video-analysis tool.

### 7.2 Controls

- Same camera position, distance and lighting; the phone mounted, not handheld, so
  reflections stay constant.
- Same content transition and similar panel warm-up.
- **At least 5 update starts before and 5 after**, reporting mean and spread.

The panel continues healing between measurements, but the quantity measured is a
power-up transient rather than the ghost, so that confound is small.

### 7.3 Clean-mode functional checks

- A burst completes without a watchdog reset.
- The device resumes normal polling afterwards.
- Real cycles-per-minute rate is measured, to compare healing rate against the
  current noise-screen approach and to size bursts sensibly.

### 7.4 Healing curve

A standard readout photo — flat mid-grey, fixed mount, fixed light — captured at
intervals, measuring ghost contrast in the region where the BYONK setup-screen text
sits. Produces a curve rather than an impression, and gives TRMNL a real
before/after.

---

## 8. Testing

**byonk** (TDD, per project rules):

- Session state machine: start, clamp to maximum, decrement, auto-end at zero,
  cancel, replace an existing session.
- Firmware-version gate rejects an old device with a clear error.
- Response shape: `panel_clean` present and `image_url` omitted during a burst;
  both back to normal once the session ends.

**firmware:** parser tests as described in 6.2. The display loop has no test
harness and is validated on the bench.

---

## 9. Risks

| Risk | Handling |
|---|---|
| Flashing wipes NVS, losing WiFi credentials and byonk registration | Flash the app partition only; do not erase the chip. Back up existing flash with esptool first. Device is USB-C flashed. |
| Long clean session drains the battery | Run sessions on USB power. |
| UPSEQ hypothesis is wrong | 7.1 gives a binary answer. A fail redirects to a hardware-defect report with equally good evidence. |
| Upstream declines the feature PR | The FastEPD fix is independent and still lands. Byonk keeps a private build for this device. |
| Uniform cleaning cannot fix a differential split | Acknowledged in section 4. The complement is inverse-image content, tracked separately. |

---

## 10. Follow-ups, explicitly out of scope here

- Custom recovery waveform via `setCustomMatrix()`, proposed with comparative
  numbers once the clean mode gives us a baseline.
- Inverse-image recovery content in byonk.
- Automatic byonk-side detection of at-risk devices.
- Timestamped image filenames, so devices can skip downloads (unrelated to
  ghosting; see `docs/HANDOVER.md` section 3.7).

---

## 11. Sequencing

1. Back up device flash; confirm app-partition-only flashing works.
2. Implement PR 1 locally; build; flash; run 7.1. **Gate: does the step vanish?**
3. Implement PR 2 locally; build; flash; run 7.3.
4. Implement the byonk side; run 8.
5. Start a real recovery session; begin the 7.4 healing curve.
6. Open PR 1 against `bitbank2/FastEPD` with measurements.
7. Open PR 2 against `usetrmnl/trmnl-firmware` with measurements and parser tests.
8. Merge the byonk feature once the protocol has settled upstream.
