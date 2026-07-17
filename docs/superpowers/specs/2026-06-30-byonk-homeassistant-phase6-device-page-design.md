# Phase 6 — Device-page enhancements (model string + per-device refresh + device name)

_Status: design approved 2026-06-30. Covers three device-page improvements (the
two remaining Phase-5 findings plus a device-naming follow-up). Finding #1 —
Panel/Dither read-back — and finding #3 — RSSI sensor enabled by default — are
already fixed and committed; they are out of scope here._

## Background

During hands-on testing of an HA-onboarded TRMNL's device page, three gaps were
found:

- **#2 — Model shows "og" for a reTerminal E1002.** byonk collapses every device
  into a two-variant `DeviceModel` enum (`OG`/`X`), so anything that is not `x`
  is reported as `og`. A reTerminal reports its own identity in the `Model`
  header, but that information is discarded.
- **#4 — Refresh interval is not per-device.** Today the refresh rate is the
  screen-level `default_refresh`, overridable only by the screen's Lua
  (`refresh_rate`). There is no way to make a single device refresh on a
  different cadence.
- **#5 — Devices are identified only by MAC.** A TRMNL appears as
  `TRMNL <mac>` in HA and is keyed by MAC in byonk. There is no friendly name,
  which is awkward in real life. Home Assistant already lets a user rename a
  device, but that name lives only in HA and is not reflected in byonk's config,
  logs, or dev UI.

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
    `native_step = 60`, `native_unit_of_measurement = "s"`.
  - No `entity_category`: it is a primary control, so it sits in the device's
    "Controls" card alongside the screen/dither/panel selects (not the separate
    Configuration card).
  - `0 = no override` (byonk falls back to Lua/screen default).
- Add `Platform.NUMBER` to the integration's `PLATFORMS` list (in code) so the
  new platform is set up and torn down with the others.

## Finding #5 — Device name (HA-owned, synced to byonk)

### Approach

Home Assistant owns the name (consistent with the Phase-5 "HA owns devices"
model). The user renames the device with HA's native device rename (the ✏️ on
the device page, which sets `name_by_user`); the integration mirrors that name
down to byonk. There is **no** separate "Name" entity and **no** name field in
the onboarding/reconfigure dialogs — those would duplicate HA's built-in rename.

The sync is **one-way, HA → byonk**. byonk never pushes a name upward; HA is
authoritative. A hand-edited `name:` in `config.yaml` is overwritten on the next
HA rename (and on the initial seeding push).

### byonk side

- `DeviceConfig` gains `name: Option<String>`.
- Admin API: `DeviceWrite` gains `name`; `patch_device` merges it
  (`body.name.or(existing.name)`); `device_block` writes `name:` when present;
  `AdminDevice` exposes `name` (both seen and unseen paths).
- The friendly name is surfaced in the admin API (`AdminDevice` exposes `name`)
  and the relevant device log lines. (MAC remains the config key and the stable
  identifier; `name` is purely a label.)

### HA side

- The synced value is **`device.name_by_user` only** — the name the user
  deliberately chose. The auto-generated default (`TRMNL <mac>`, i.e.
  `device.name`) is **not** synced, so byonk's config is not polluted with a
  redundant MAC-based name before the user renames. `effective_name =
  device.name_by_user or ""` (empty string = "no user name").
- byonk treats an empty `name` as no name (`None`); with no name set, byonk
  displays the MAC as before.
- On device-entry setup, resolve the HA device for this entry
  (`identifiers={(DOMAIN, key)}`) and:
  - **Seed once:** if `effective_name` differs from byonk's stored name, PATCH
    byonk `{"name": effective_name}`.
  - **Track changes:** register `async_track_device_registry_updated_event` for
    that device id; on an `update` action whose name changed, PATCH byonk with
    the new `effective_name`, then `async_request_refresh()`. Register via
    `entry.async_on_unload(...)` so the listener is torn down with the entry.
- The HA device id is assigned after the device is created (first entity
  platform setup). Resolve it after `async_config_entry_first_refresh`/platform
  setup; if not yet present, the next registry-updated event will carry it.
- No new entity and no new platform for this feature.

### Edge cases

- **Name cleared in HA** (`name_by_user` reset to `None`): `effective_name`
  becomes `""`; byonk clears its stored name and reverts to displaying the MAC.
- **byonk device not yet present** (orphan/removal grace window): a failed PATCH
  is logged and ignored; the next registry event or refresh re-attempts.
- **Loops:** sync is one-way, so there is no echo. Seeding only PATCHes when the
  value actually differs, avoiding a write on every setup.

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
- Admin integration test: `PATCH /api/admin/devices/:key {"name": "Kitchen"}`
  then `GET` reads `name == "Kitchen"` back.

**HA (`tests_ha`)**
- `test_number.py`: the device refresh Number reflects byonk's stored value and,
  on `set_value`, PATCHes `{"refresh": N}`.
- `test_name_sync.py`: after the device entry is set up, renaming the HA device
  (set `name_by_user`) PATCHes byonk `{"name": ...}`; the initial seeding push
  fires when byonk's stored name differs from HA's.

## Out of scope

- The "expose more device metadata" idea (capturing `Board`, `Colors`, `Width`,
  `Height` into the registry) is deferred; it is a larger capture-layer change
  and not required for #2 or #4.
- No change to the dev-preview UI model query parameter.
- Surfacing the device name in the dev-preview UI device list is deferred to a
  fast-follow (the dev UI is a developer preview tool; the admin API and device
  log line already expose the name).
