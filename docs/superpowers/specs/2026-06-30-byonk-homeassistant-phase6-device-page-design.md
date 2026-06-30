# Phase 6 — Device-page enhancements (model string + per-device refresh)

_Status: design approved 2026-06-30. Covers two of the four device-page findings
surfaced during Phase 5 live validation. (Finding #1 — Panel/Dither read-back —
and finding #3 — RSSI sensor enabled by default — are already fixed and
committed; they are out of scope here.)_

## Background

During hands-on testing of an HA-onboarded TRMNL's device page, two gaps were
found:

- **#2 — Model shows "og" for a reTerminal E1002.** byonk collapses every device
  into a two-variant `DeviceModel` enum (`OG`/`X`), so anything that is not `x`
  is reported as `og`. A reTerminal reports its own identity in the `Model`
  header, but that information is discarded.
- **#4 — Refresh interval is not per-device.** Today the refresh rate is the
  screen-level `default_refresh`, overridable only by the screen's Lua
  (`refresh_rate`). There is no way to make a single device refresh on a
  different cadence.

## Finding #2 — Surface the real model string

### Current behavior

- `Device.model: DeviceModel` where `DeviceModel` is `enum { OG, X }`.
- `DeviceModel::parse(s)` maps `"x"` → `X`, everything else → `OG`.
- The enum has **no behavioral consumers**:
  - Rendering dimensions come from the `Width`/`Height` headers and
    `DisplaySpec::from_dimensions`, not the model.
  - Lua scripts already receive the **raw** `Model` header string via
    `device.model` (set from `model_str`, not the enum).
  - Panel auto-detection matches the **raw** header via
    `find_panel_for_board(model_str)`.
  - The only places the lossy enum is observed are the stored `Device.model`
    and the HA "Model" diagnostic sensor (`AdminDevice.model`, which is already
    `d.model.to_string()`).

### Design

- Change `Device.model` from `DeviceModel` to `String`, storing the **verbatim
  `Model` header** the device sends. When the header is absent, default to
  `"og"` (preserves today's fallback).
- Remove the `DeviceModel` enum and `DeviceModel::parse` (no remaining
  consumers). Update construction sites:
  - `Device::new(...)` takes the model `String`.
  - `/api/display` and `/api/setup` store `headers.get_str("Model")` verbatim
    (default `"og"`).
  - Test helpers and `device_registry` tests updated to pass strings.
- `AdminDevice.model` continues to be the model string — the admin JSON shape is
  unchanged, the value is just accurate (e.g. the reTerminal's real model).

### HA side

No code change. The `model` sensor passes the byonk string through, so it now
shows the real reported model.

### Migration / compatibility

- The registry is in-memory only (`Arc<RwLock<HashMap<DeviceId, Device>>>`), so
  there is no persisted state to migrate.
- The admin API value for genuine OG/X devices is unchanged (`"og"`/`"x"`).
- Lua `device.model` already exposed the raw header, so no screen behavior
  changes.

## Finding #4 — Per-device refresh override

### Current behavior

In `content_pipeline::run_script_for_screen`:

```rust
let refresh_rate = if lua_result.refresh_rate > 0 {
    lua_result.refresh_rate
} else {
    screen.default_refresh
};
```

The device config is not consulted. `DeviceConfig` is, however, already resolved
at the `/api/display` call site (`get_screen_for_device` → `(ScreenConfig,
DeviceConfig)`).

### Design

**Precedence (approved): Lua-returned (>0) → per-device override (>0) → screen
`default_refresh`.** Lua-controlled screens keep full control; the per-device
override only replaces the static screen default.

- Add `refresh: Option<u32>` (seconds) to `DeviceConfig`.
- Resolution treats `Some(0)` the same as `None` (no override) — this sidesteps
  the "clear a field via PATCH" problem (a Number entity can send `0` but not
  "unset"). Effective rule:

  ```rust
  let device_override = device_refresh.filter(|&r| r > 0);
  let refresh_rate = if lua_result.refresh_rate > 0 {
      lua_result.refresh_rate
  } else if let Some(r) = device_override {
      r
  } else {
      screen.default_refresh
  };
  ```

- Thread the override into `run_script_for_screen` via `DeviceContext` (which
  already carries per-device info). Both refresh-resolution sites in
  `content_pipeline.rs` apply the same rule.

### Admin API

- `DeviceWrite` gains `refresh: Option<u32>`.
- `patch_device` merges it like the other optional fields
  (`body.refresh.or(existing.refresh)`).
- `device_block` writes `refresh:` into the device's YAML block when present.
- `AdminDevice` gains `refresh: Option<u32>`, populated from `dc.refresh` in
  both the seen-device path and the configured-but-unseen path.

### HA side

- New `ByonkRefreshNumber` device entity (`number` platform), added alongside the
  device selects/sensors when `CONF_DEVICE_KEY` is in the entry.
- Behavior:
  - `native_value` ← `device.get("refresh") or 0` (no override renders as `0`
    rather than "unknown"; `0` means no override).
  - `async_set_native_value(value)` → PATCH `{"refresh": int(value)}` then
    `async_request_refresh()`.
  - Bounds: `native_min_value = 0`, `native_max_value = 86400`,
    `native_step = 60`, `native_unit_of_measurement = "s"`,
    `entity_category = CONFIG`.
  - `0 = no override` (byonk falls back to Lua/screen default).
- Add `Platform.NUMBER` to the integration's `PLATFORMS` list (in code) so the
  new platform is set up and torn down with the others.

## Testing

**Rust**
- `DeviceModel` removal: existing device/registry tests updated to strings;
  add a test that a non-`x`/non-`og` `Model` header is stored and returned
  verbatim by `GET /api/admin/devices`.
- Refresh resolution unit tests in `content_pipeline`: assert the three-way
  precedence (Lua wins over override; override wins over screen default;
  `Some(0)` is treated as no override).
- Admin integration test: `PATCH /api/admin/devices/:key {"refresh": N}` then
  `GET` reads `refresh == N` back (seen and unseen device paths), mirroring the
  Phase-6 finding-#1 guards.

**HA (`tests_ha`)**
- `test_number.py`: the device refresh Number reflects byonk's stored value and,
  on `set_value`, PATCHes `{"refresh": N}`.

## Out of scope

- The "expose more device metadata" idea (capturing `Board`, `Colors`, `Width`,
  `Height` into the registry) is deferred; it is a larger capture-layer change
  and not required for #2 or #4.
- No change to the dev-preview UI model query parameter.
