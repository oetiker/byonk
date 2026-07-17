# Phase 6 Device-page Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a TRMNL's reported model accurate, give each device an optional refresh-interval override, and let a device's Home-Assistant name sync down into byonk.

**Architecture:** Three mostly-independent slices. (#2) byonk stores the verbatim `Model` header instead of collapsing it to an `OG`/`X` enum. (#4) byonk gains a per-device `refresh` config field resolved as `Lua > device-override > screen-default`, controlled from HA by a new Number entity. (#5) HA owns the device name (native rename) and mirrors it to a new byonk `name` config field via a one-way registry listener.

**Tech Stack:** Rust (axum, serde_yaml) for byonk; Python (Home Assistant custom component, pytest-homeassistant-custom-component) for the integration.

## Global Constraints

- Never `git add -A`/`git add .` — stage explicit paths and verify `git diff --cached` before committing (pre-existing untracked files must not be swept in).
- Rust: `make check` (fmt + clippy + tests) must pass.
- HA: `.venv/bin/python -m pytest tests_ha` and `.venv/bin/ruff check custom_components/byonk tests_ha` must pass (ruff does not lint `tests_ha` via `make ha-check`; run it explicitly).
- HA Python ≤ 3.13.
- Admin API merge convention for optional device fields: `body.<field>.or(existing.<field>)`; the writer omits empty values so they can be cleared.
- byonk is pre-release with no userbase — change byonk APIs freely to serve the integration.
- Commit messages end with the `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailer.

---

### Task 1: byonk — store the verbatim device model (#2)

**Files:**
- Modify: `src/models/device.rs` (remove `DeviceModel`, change `Device.model` to `String`, update `Device::new`)
- Modify: `src/models/mod.rs` (drop `DeviceModel` from the re-export)
- Modify: `src/api/setup.rs:62-63` (store raw model string)
- Modify: `src/api/display.rs` (4 `DeviceModel::parse` sites + import)
- Modify: `src/services/device_registry.rs` (test constructors use strings)
- Test: `tests/admin_devices_test.rs` (new round-trip test)

**Interfaces:**
- Produces: `Device { model: String, .. }`; `Device::new(device_id: DeviceId, model: String, fw_version: String) -> Device`. `AdminDevice.model` JSON stays a string (now the raw header).

- [ ] **Step 1: Write the failing test**

Add to `tests/admin_devices_test.rs` (it already has `mod common;` and uses `TestApp`; reuse the existing `AUTH`/admin patterns from that file — if no admin token helper exists there, mirror `tests/admin_write_test.rs`'s `const AUTH: (&str, &str) = ("Authorization", "Bearer secret");` and `TestApp::new_admin_with_file("secret", dir.path())`):

```rust
#[tokio::test]
async fn test_custom_model_header_is_stored_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    // A reTerminal reports its own model string (not "og"/"x").
    let mac = "9C:13:9E:AB:99:D4";
    let resp = app
        .get_with_headers(
            "/api/setup",
            &[("ID", mac), ("FW-Version", "1.0.0"), ("Model", "reterminal_e1002")],
        )
        .await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);

    let listed = app.get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == mac)
        .expect("device row present");
    assert_eq!(row["model"], "reterminal_e1002");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test admin_devices_test test_custom_model_header_is_stored_verbatim -- --nocapture`
Expected: FAIL — `row["model"]` is `"og"` (header collapsed by `DeviceModel::parse`).

- [ ] **Step 3: Change the model type in `src/models/device.rs`**

Replace the `DeviceModel` enum + impls (the `pub enum DeviceModel { OG, X }` block and its `impl DeviceModel { pub fn parse ... }` and `impl fmt::Display for DeviceModel { ... }`) by deleting all three. Then change the `Device` struct field and constructor:

```rust
/// Device runtime metadata (tracked in memory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub device_id: DeviceId,
    pub api_key: ApiKey,
    /// Verbatim `Model` header the device reported (e.g. "og", "x", "reterminal_e1002").
    pub model: String,
    pub firmware_version: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
}

impl Device {
    pub fn new(device_id: DeviceId, model: String, fw_version: String) -> Self {
        Self {
            device_id,
            api_key: ApiKey::generate(),
            model,
            firmware_version: fw_version,
            last_seen: chrono::Utc::now(),
            battery_voltage: None,
            rssi: None,
        }
    }
}
```

- [ ] **Step 4: Drop the re-export in `src/models/mod.rs`**

Change the `pub use device::{... DeviceModel ...}` line to remove `DeviceModel`:

```rust
pub use device::{verify_ed25519_signature, ApiKey, Device, DeviceId, Ed25519Error};
```

- [ ] **Step 5: Update `src/api/setup.rs`**

At lines ~62-63 replace:

```rust
    let model_str = headers.get_str("Model").unwrap_or("og");

    let model = DeviceModel::parse(model_str);
```

with:

```rust
    let model = headers.get_str("Model").unwrap_or("og").to_string();
```

Remove `DeviceModel` from the `use crate::models::{...}` import at the top of the file (keep `AppConfig, Device, DeviceId`). The `model = ?model` tracing field still compiles (now a `String`).

- [ ] **Step 6: Update `src/api/display.rs`**

Remove `DeviceModel` from the `use crate::models::{ ... }` import (line ~15).

Pending-device branch (~line 354): replace the `Device::new(pending_id.clone(), DeviceModel::parse(model_str), ...)` model argument with `model_str.to_string()`, and replace `pending_device.model = DeviceModel::parse(model_str);` with `pending_device.model = model_str.to_string();`.

Main branch (~line 401-423): replace

```rust
    let model = DeviceModel::parse(model_str);
```

with

```rust
    let model = model_str.to_string();
```

and at `device.model = model;` keep it (now assigns the `String`). `Device::new(device_id.clone(), model, fw_version.clone())` still compiles (model is now a `String`; it is moved — if a later use of `model` errors, change that call to `model.clone()`).

- [ ] **Step 7: Update `src/services/device_registry.rs` tests**

In the `#[cfg(test)]` module, remove `use crate::models::DeviceModel;` and replace every `DeviceModel::OG` with `"og".to_string()` and every `DeviceModel::X` with `"x".to_string()` in the `Device::new(...)` calls.

- [ ] **Step 8: Build & run the whole suite**

Run: `cargo test --test admin_devices_test test_custom_model_header_is_stored_verbatim -- --nocapture`
Expected: PASS.
Then run: `make check`
Expected: fmt + clippy clean, all tests pass. (If clippy/compile flags any remaining `DeviceModel` reference, fix it — there should be none outside the sites above.)

- [ ] **Step 9: Commit**

```bash
git add src/models/device.rs src/models/mod.rs src/api/setup.rs src/api/display.rs src/services/device_registry.rs tests/admin_devices_test.rs
git commit -m "feat(model): store verbatim device Model header instead of OG/X enum

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: byonk — per-device refresh override (#4)

**Files:**
- Modify: `src/models/config.rs` (derive `Default`, convert struct literals, add `refresh`)
- Modify: `src/services/content_pipeline.rs` (`resolve_refresh_rate` helper, `DeviceContext.refresh_override`, threading, `EMPTY_DEVICE`)
- Modify: `src/api/display.rs:585` (init new `DeviceContext` field)
- Modify: `src/api/admin/write.rs` (`DeviceWrite.refresh`, merge, `device_block`)
- Modify: `src/api/admin/read.rs` (`AdminDevice.refresh`)
- Test: `tests/admin_write_test.rs` (round-trip), unit tests in `content_pipeline.rs`

**Interfaces:**
- Consumes: `Device` (Task 1).
- Produces: `DeviceConfig.refresh: Option<u32>`; `content_pipeline::resolve_refresh_rate(lua_refresh: u32, device_override: Option<u32>, screen_default: u32) -> u32`; `DeviceContext.refresh_override: Option<u32>`; `AdminDevice.refresh: Option<u32>`; `DeviceWrite.refresh: Option<u32>`.

- [ ] **Step 1: Write the failing unit test for the resolver**

In `src/services/content_pipeline.rs`, add a `#[cfg(test)]` module at the end of the file (or extend an existing one):

```rust
#[cfg(test)]
mod refresh_tests {
    use super::resolve_refresh_rate;

    #[test]
    fn lua_wins_over_override_and_default() {
        assert_eq!(resolve_refresh_rate(120, Some(600), 900), 120);
    }

    #[test]
    fn override_wins_over_default_when_lua_zero() {
        assert_eq!(resolve_refresh_rate(0, Some(600), 900), 600);
    }

    #[test]
    fn zero_override_is_ignored() {
        assert_eq!(resolve_refresh_rate(0, Some(0), 900), 900);
        assert_eq!(resolve_refresh_rate(0, None, 900), 900);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test resolve_refresh_rate`
Expected: FAIL — `resolve_refresh_rate` not found.

- [ ] **Step 3: Add the resolver and use it**

In `src/services/content_pipeline.rs`, add the free function near the top of the file (after the imports, before `struct ScriptResult`):

```rust
/// Resolve the effective refresh rate.
/// Precedence: Lua-returned (>0) > per-device override (>0) > screen default.
pub(crate) fn resolve_refresh_rate(
    lua_refresh: u32,
    device_override: Option<u32>,
    screen_default: u32,
) -> u32 {
    if lua_refresh > 0 {
        lua_refresh
    } else if let Some(r) = device_override.filter(|&r| r > 0) {
        r
    } else {
        screen_default
    }
}
```

In `run_script_for_screen`, replace the existing block:

```rust
        let refresh_rate = if lua_result.refresh_rate > 0 {
            lua_result.refresh_rate
        } else {
            screen.default_refresh
        };
```

with:

```rust
        let device_override = device_ctx.as_ref().and_then(|c| c.refresh_override);
        let refresh_rate =
            resolve_refresh_rate(lua_result.refresh_rate, device_override, screen.default_refresh);
```

- [ ] **Step 4: Run the resolver tests**

Run: `cargo test resolve_refresh_rate`
Expected: PASS.

- [ ] **Step 5: Add the `refresh_override` field to `DeviceContext`**

In `src/services/content_pipeline.rs`, add to the `DeviceContext` struct (after `dither_strength`):

```rust
    /// Per-device refresh override (seconds) from DeviceConfig; 0/None = no override.
    pub refresh_override: Option<u32>,
```

`DeviceContext` derives `Default`, so the spread-form literals (`src/main.rs`, `src/api/dev.rs`, `src/api/display.rs:271`) compile unchanged. Fix the one full literal at `src/api/display.rs:585` by adding before its closing `};`:

```rust
        refresh_override: None,
```

- [ ] **Step 6: Set the override in `run_script_for_device`**

In `src/services/content_pipeline.rs`, change `run_script_for_device` so it injects the override from the resolved config. Make `device_ctx` mutable and set the field in the found-config branch:

```rust
    pub fn run_script_for_device(
        &self,
        device_mac: &str,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ScriptResult, ContentError> {
        let mut device_ctx = device_ctx;
        let config = self.config.load();
        let device_config = device_ctx
            .as_ref()
            .and_then(|ctx| ctx.registration_code.as_deref())
            .and_then(|code| config.get_device_config_for_code(code))
            .or_else(|| config.get_device_config(device_mac));

        if let Some(device_config) = device_config {
            if let Some(screen_config) = self.resolve_screen(&device_config.screen) {
                if let Some(ctx) = device_ctx.as_mut() {
                    ctx.refresh_override = device_config.refresh;
                }
                return self.run_script_for_screen(
                    &screen_config,
                    &device_config.params,
                    device_ctx,
                );
            }
        }
        // ... (unchanged fallback block below) ...
```

Leave the rest of the function (the default-screen fallback) unchanged — it passes `device_ctx` through with `refresh_override` defaulted to `None`.

- [ ] **Step 7: Derive `Default` for `DeviceConfig` and add the field**

In `src/models/config.rs`, change the derive on `DeviceConfig` to include `Default`:

```rust
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DeviceConfig {
```

Add the new field after `strength`:

```rust
    /// Optional per-device refresh override in seconds (0/absent = use Lua/screen default)
    #[serde(default)]
    pub refresh: Option<u32>,
```

- [ ] **Step 8: Convert the `DeviceConfig` struct literals to spread form**

Now that `DeviceConfig: Default`, replace each full literal with the spread form (keeps the screen, defaults the rest, so this field and Task 3's `name` need no further literal edits).

`src/services/content_pipeline.rs:189` (`EMPTY_DEVICE`):

```rust
        let dc = EMPTY_DEVICE.get_or_init(|| crate::models::DeviceConfig {
            screen: "default".to_string(),
            ..Default::default()
        });
```

`src/models/config.rs` — replace each of the six test literals (currently at ~lines 521, 557, 590, 742, 784, 800) with the spread form, preserving that literal's `screen` value:

```rust
            DeviceConfig {
                screen: "custom".to_string(),   // line 521
                ..Default::default()
            },
```
```rust
            DeviceConfig {
                screen: "test".to_string(),     // line 557
                ..Default::default()
            },
```
```rust
            DeviceConfig {
                screen: "nonexistent".to_string(), // line 590
                ..Default::default()
            },
```
```rust
            DeviceConfig {
                screen: "custom".to_string(),   // line 742
                ..Default::default()
            },
```
```rust
            DeviceConfig {
                screen: "test".to_string(),     // line 784
                ..Default::default()
            },
```
```rust
            DeviceConfig {
                screen: "test".to_string(),     // line 800
                ..Default::default()
            },
```

(If `cargo` reports any other `DeviceConfig { ... }` full literal, convert it the same way.)

- [ ] **Step 9: Add `refresh` to the admin write path**

In `src/api/admin/write.rs`, add to `DeviceWrite`:

```rust
    pub refresh: Option<u32>,
```

In `patch_device`'s `merged` construction, add (alongside `panel`/`dither`):

```rust
        refresh: body.refresh.or(existing.refresh),
```

In `device_block`, after the `colors` block and before `params`, add (omit when 0 so a `0` from the HA Number clears the override):

```rust
    if let Some(r) = w.refresh {
        if r > 0 {
            m.insert("refresh".into(), serde_yaml::Value::from(r));
        }
    }
```

- [ ] **Step 10: Expose `refresh` in the admin read path**

In `src/api/admin/read.rs`, add to `AdminDevice`:

```rust
    pub refresh: Option<u32>,
```

In the first loop (seen devices), add to the pushed `AdminDevice`:

```rust
            refresh: dc.and_then(|c| c.refresh),
```

In the second loop (configured-but-unseen), add:

```rust
            refresh: dc.refresh,
```

- [ ] **Step 11: Write the admin round-trip test**

Add to `tests/admin_write_test.rs`:

```rust
#[tokio::test]
async fn test_patch_refresh_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        r#"{"key":"AA:BB","screen":"hello"}"#,
    )
    .await;
    let resp = app
        .patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"refresh":600}"#)
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json.as_array().unwrap().iter().find(|d| d["key"] == "AA:BB").unwrap();
    assert_eq!(row["refresh"], 600);

    // 0 clears the override.
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"refresh":0}"#)
        .await;
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json.as_array().unwrap().iter().find(|d| d["key"] == "AA:BB").unwrap();
    assert_eq!(row["refresh"], serde_json::Value::Null);
}
```

- [ ] **Step 12: Run & verify**

Run: `cargo test --test admin_write_test test_patch_refresh_reads_back -- --nocapture`
Expected: PASS.
Then: `make check`
Expected: clean.

- [ ] **Step 13: Commit**

```bash
git add src/models/config.rs src/services/content_pipeline.rs src/api/display.rs src/api/admin/write.rs src/api/admin/read.rs tests/admin_write_test.rs
git commit -m "feat(refresh): per-device refresh override (Lua > device > screen default)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: byonk — per-device name field (#5 byonk side)

**Files:**
- Modify: `src/models/config.rs` (add `name`)
- Modify: `src/api/admin/write.rs` (`DeviceWrite.name`, merge, `device_block`)
- Modify: `src/api/admin/read.rs` (`AdminDevice.name`)
- Test: `tests/admin_write_test.rs` (round-trip)

**Interfaces:**
- Produces: `DeviceConfig.name: Option<String>`; `AdminDevice.name: Option<String>`; `DeviceWrite.name: Option<String>`.

- [ ] **Step 1: Write the failing round-trip test**

Add to `tests/admin_write_test.rs`:

```rust
#[tokio::test]
async fn test_patch_name_reads_back_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        r#"{"key":"AA:BB","screen":"hello"}"#,
    )
    .await;
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"name":"Kitchen"}"#)
        .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json.as_array().unwrap().iter().find(|d| d["key"] == "AA:BB").unwrap();
    assert_eq!(row["name"], "Kitchen");

    // Empty string clears it.
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"name":""}"#)
        .await;
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json.as_array().unwrap().iter().find(|d| d["key"] == "AA:BB").unwrap();
    assert_eq!(row["name"], serde_json::Value::Null);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test admin_write_test test_patch_name_reads_back_and_clears -- --nocapture`
Expected: FAIL — `name` is not a field (deserializes/serializes as absent → assertion mismatch / compile of the JSON key returns null on first assert).

- [ ] **Step 3: Add the `name` field to `DeviceConfig`**

In `src/models/config.rs`, add after `refresh`:

```rust
    /// Optional friendly name (mirrored from Home Assistant; absent = identify by MAC)
    #[serde(default)]
    pub name: Option<String>,
```

(No struct-literal churn — they use `..Default::default()` after Task 2.)

- [ ] **Step 4: Add `name` to the admin write path**

In `src/api/admin/write.rs`, add to `DeviceWrite`:

```rust
    pub name: Option<String>,
```

In `patch_device`'s `merged`, add:

```rust
        name: body.name.clone().or(existing.name.clone()),
```

In `device_block`, after the `refresh` block, add (omit when empty so `""` clears it):

```rust
    if let Some(n) = &w.name {
        if !n.is_empty() {
            m.insert("name".into(), n.as_str().into());
        }
    }
```

Note: `DeviceWrite` now has both `refresh` and `name`; the `merged` literal in `patch_device` must set every field. Ensure the `merged = DeviceWrite { key, screen, panel, dither, colors, params, refresh, name }` block lists `refresh` and `name`.

- [ ] **Step 5: Expose `name` in the admin read path**

In `src/api/admin/read.rs`, add to `AdminDevice`:

```rust
    pub name: Option<String>,
```

First loop (seen): `name: dc.and_then(|c| c.name.clone()),`
Second loop (unseen): `name: dc.name.clone(),`

- [ ] **Step 6: Run & verify**

Run: `cargo test --test admin_write_test test_patch_name_reads_back_and_clears -- --nocapture`
Expected: PASS.
Then: `make check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/models/config.rs src/api/admin/write.rs src/api/admin/read.rs tests/admin_write_test.rs
git commit -m "feat(name): per-device name field in config + admin API

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: HA — refresh-interval Number entity (#4 HA side)

**Files:**
- Create: `custom_components/byonk/number.py`
- Modify: `custom_components/byonk/const.py` (add `Platform.NUMBER`)
- Modify: `custom_components/byonk/strings.json` and `custom_components/byonk/translations/en.json` (entity name)
- Test: `tests_ha/test_number.py`

**Interfaces:**
- Consumes: `ByonkDeviceEntity`, `ByonkCoordinator`, `ByonkConfigEntry`, `CONF_DEVICE_KEY`. byonk device row exposes `refresh` (Task 2).

- [ ] **Step 1: Write the failing test**

Create `tests_ha/test_number.py`:

```python
from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {"key": "AA:BB", "registered": True, "screen": "transit", "refresh": 600}


async def _setup(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()


async def test_refresh_number_reflects_value(hass, byonk):
    await _setup(hass, byonk)
    ent = next(
        s for s in hass.states.async_all("number")
        if "trmnl" in s.entity_id and "refresh" in s.entity_id
    )
    assert int(float(ent.state)) == 600


async def test_refresh_number_sets_value(hass, byonk):
    await _setup(hass, byonk)
    ent = next(
        s for s in hass.states.async_all("number")
        if "trmnl" in s.entity_id and "refresh" in s.entity_id
    )
    await hass.services.async_call(
        "number", "set_value",
        {"entity_id": ent.entity_id, "value": 300}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"refresh": 300}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `.venv/bin/python -m pytest tests_ha/test_number.py -q`
Expected: FAIL — no `number` entities (platform not set up).

- [ ] **Step 3: Add the number platform to `const.py`**

In `custom_components/byonk/const.py`, change:

```python
PLATFORMS: list[Platform] = [Platform.SENSOR, Platform.SELECT, Platform.SWITCH]
```

to:

```python
PLATFORMS: list[Platform] = [
    Platform.SENSOR,
    Platform.SELECT,
    Platform.SWITCH,
    Platform.NUMBER,
]
```

- [ ] **Step 4: Create `custom_components/byonk/number.py`**

```python
"""Byonk number entities."""
from __future__ import annotations

from homeassistant.components.number import NumberEntity
from homeassistant.const import EntityCategory, UnitOfTime
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        async_add_entities([ByonkRefreshNumber(coordinator, entry.data[CONF_DEVICE_KEY])])


class ByonkRefreshNumber(ByonkDeviceEntity, NumberEntity):
    _attr_translation_key = "refresh"
    _attr_entity_category = EntityCategory.CONFIG
    _attr_native_min_value = 0
    _attr_native_max_value = 86400
    _attr_native_step = 60
    _attr_native_unit_of_measurement = UnitOfTime.SECONDS

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator, key)
        self._attr_unique_id = f"{key}_refresh"

    @property
    def native_value(self) -> float | None:
        device = self.device
        # 0 = no override (rather than "unknown").
        return float(device.get("refresh") or 0) if device else None

    async def async_set_native_value(self, value: float) -> None:
        await self.coordinator.client.async_update_device(self._key, {"refresh": int(value)})
        await self.coordinator.async_request_refresh()
```

- [ ] **Step 5: Add the entity name to strings/translations**

In `custom_components/byonk/strings.json`, add a `number` section inside `entity` (sibling of `select`/`sensor`):

```json
  "number": {
   "refresh": {
    "name": "Refresh interval"
   }
  }
```

Add the identical `number` block to the `entity` object in `custom_components/byonk/translations/en.json`.

- [ ] **Step 6: Run & verify**

Run: `.venv/bin/python -m pytest tests_ha/test_number.py -q`
Expected: PASS.
Then: `.venv/bin/python -m pytest tests_ha -q && .venv/bin/ruff check custom_components/byonk tests_ha`
Expected: all pass, ruff clean.

- [ ] **Step 7: Commit**

```bash
git add custom_components/byonk/number.py custom_components/byonk/const.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_number.py
git commit -m "feat(ha): per-device refresh-interval Number entity

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: HA — sync the HA device name to byonk (#5 HA side)

**Files:**
- Create: `custom_components/byonk/name_sync.py`
- Modify: `custom_components/byonk/__init__.py` (call the sync setup from `_async_setup_device_entry`)
- Test: `tests_ha/test_name_sync.py`

**Interfaces:**
- Consumes: `ByonkCoordinator` (has `.client.async_update_device`, `.data.devices`), `CONF_DEVICE_KEY`, `DOMAIN`. byonk device row exposes `name` (Task 3).
- Produces: `async_setup_name_sync(hass, entry, coordinator) -> None`.

- [ ] **Step 1: Write the failing test**

Create `tests_ha/test_name_sync.py`:

```python
from homeassistant.helpers import device_registry as dr

from custom_components.byonk.const import DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry


async def _setup(hass, byonk):
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": "transit"}]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    return dev_entry


async def test_rename_syncs_to_byonk(hass, byonk):
    await _setup(hass, byonk)
    registry = dr.async_get(hass)
    device = registry.async_get_device(identifiers={(DOMAIN, "AA:BB")})
    assert device is not None

    byonk.update_device.reset_mock()
    registry.async_update_device(device.id, name_by_user="Kitchen")
    await hass.async_block_till_done()

    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"name": "Kitchen"}


async def test_clear_name_syncs_empty(hass, byonk):
    await _setup(hass, byonk)
    registry = dr.async_get(hass)
    device = registry.async_get_device(identifiers={(DOMAIN, "AA:BB")})
    registry.async_update_device(device.id, name_by_user="Kitchen")
    await hass.async_block_till_done()

    byonk.update_device.reset_mock()
    registry.async_update_device(device.id, name_by_user=None)
    await hass.async_block_till_done()

    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"name": ""}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `.venv/bin/python -m pytest tests_ha/test_name_sync.py -q`
Expected: FAIL — renaming does not call `update_device` (no sync wired up).

- [ ] **Step 3: Create `custom_components/byonk/name_sync.py`**

```python
"""Mirror a device's Home Assistant name down to byonk (one-way, HA -> byonk)."""
from __future__ import annotations

import logging

from homeassistant.core import Event, HomeAssistant, callback
from homeassistant.helpers import device_registry as dr
from homeassistant.helpers.event import async_track_device_registry_updated_event

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY, DOMAIN
from .coordinator import ByonkConfigEntry, ByonkCoordinator

_LOGGER = logging.getLogger(__name__)


def _effective_name(device: dr.DeviceEntry | None) -> str:
    """The deliberately-chosen name only; '' means no user name (identify by MAC)."""
    if device is None:
        return ""
    return device.name_by_user or ""


async def async_setup_name_sync(
    hass: HomeAssistant, entry: ByonkConfigEntry, coordinator: ByonkCoordinator
) -> None:
    key = entry.data[CONF_DEVICE_KEY]
    registry = dr.async_get(hass)
    device = registry.async_get_device(identifiers={(DOMAIN, key)})
    if device is None:
        # Entities create the device during platform setup; if it is somehow not
        # present yet, skip — nothing to track or seed.
        return

    async def _push(name: str) -> None:
        try:
            await coordinator.client.async_update_device(key, {"name": name})
        except ByonkApiError as err:
            _LOGGER.debug("name sync failed for %s: %s", key, err)
            return
        await coordinator.async_request_refresh()

    # Seed once if byonk's stored name differs from HA's chosen name.
    desired = _effective_name(device)
    current = ""
    for d in coordinator.data.devices:
        if d.get("key") == key:
            current = d.get("name") or ""
            break
    if desired != current:
        await _push(desired)

    @callback
    def _handle_update(event: Event) -> None:
        if event.data.get("action") != "update":
            return
        if "name_by_user" not in event.data.get("changes", {}):
            return
        updated = registry.async_get_device(identifiers={(DOMAIN, key)})
        hass.async_create_task(_push(_effective_name(updated)))

    entry.async_on_unload(
        async_track_device_registry_updated_event(hass, device.id, _handle_update)
    )
```

- [ ] **Step 4: Wire it into `__init__.py`**

In `custom_components/byonk/__init__.py`, add the import near the others:

```python
from .name_sync import async_setup_name_sync
```

In `_async_setup_device_entry`, after `await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)` and before `return True`, add:

```python
    await async_setup_name_sync(hass, entry, coordinator)
```

- [ ] **Step 5: Run & verify**

Run: `.venv/bin/python -m pytest tests_ha/test_name_sync.py -q`
Expected: PASS (both tests).
Then: `.venv/bin/python -m pytest tests_ha -q && .venv/bin/ruff check custom_components/byonk tests_ha`
Expected: all pass, ruff clean.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/name_sync.py custom_components/byonk/__init__.py tests_ha/test_name_sync.py
git commit -m "feat(ha): mirror device name to byonk on rename (one-way HA->byonk)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Documentation + CHANGES

**Files:**
- Modify: `CHANGES.md` (Unreleased)
- Modify: `docs/src/guide/ha-integration.md` (model, refresh Number, naming)

**Interfaces:** none (docs only).

- [ ] **Step 1: Update CHANGES.md**

In the `## Unreleased` → `### New` (or a suitable subsection), add:

```markdown
- **Per-device refresh interval (Home Assistant)**: each TRMNL device now has a
  *Refresh interval* Number entity. The value (seconds) overrides the screen's
  static default; a screen's own Lua `refresh_rate` still takes precedence, and
  `0` means "no override". Stored per device in byonk's config (`refresh:`).
- **Device naming (Home Assistant)**: renaming a TRMNL device in Home Assistant
  now mirrors the name down to byonk (stored as `name:` on the device), so byonk
  no longer identifies the device only by MAC. The sync is one-way (HA owns the
  name).
```

In `### Changed` add:

```markdown
- **Reported device model** is now the verbatim `Model` header the device sends
  (e.g. a reTerminal reports its real model) instead of being collapsed to
  `og`/`x`. Genuine TRMNL OG/X devices are unaffected.
```

- [ ] **Step 2: Update the HA integration guide**

In `docs/src/guide/ha-integration.md`, find the section that lists the per-device entities (screen/dither/panel selects, battery/signal/last-seen/firmware/model sensors) and:
- add the **Refresh interval** Number entity to that list with a one-line explanation of the `Lua > device override > screen default` precedence and the `0 = no override` convention;
- add a short note that the device's **name** is taken from Home Assistant (rename the device in HA the usual way; the name is mirrored to byonk);
- if the text claims the model is `og`/`x`, update it to say byonk now reports the device's real model string.

(Keep edits to existing prose; do not restructure the page.)

- [ ] **Step 3: Build the docs**

Run: `make docs`
Expected: mdBook build succeeds.

- [ ] **Step 4: Commit**

```bash
git add CHANGES.md docs/src/guide/ha-integration.md
git commit -m "docs: Phase 6 device-page enhancements (model, refresh, naming)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- #2 real model string → Task 1 (byonk) + Task 6 docs. HA needs no code (sensor passes through) ✓
- #4 refresh: byonk field/resolution/admin API → Task 2; HA Number → Task 4 ✓
- #5 name: byonk field/admin API → Task 3; HA sync → Task 5 ✓
- Spec note "both refresh sites": only `run_script_for_screen` (real device path) gets the override; `run_script_direct` is dev-preview with no device config and is intentionally left as-is. Documented here as a deliberate narrowing of the spec wording.
- Out-of-scope items (extra metadata capture, dev-UI/log name surfacing) are not implemented, per spec.

**Placeholder scan:** No TBD/TODO; every code step shows real code; the only prose-only step is the docs guide edit (Task 6 Step 2), which is inherently descriptive.

**Type consistency:** `resolve_refresh_rate(u32, Option<u32>, u32) -> u32`, `DeviceContext.refresh_override: Option<u32>`, `DeviceConfig.refresh: Option<u32>` / `name: Option<String>`, `AdminDevice.refresh`/`name`, `DeviceWrite.refresh`/`name` all consistent across tasks. `Device::new(DeviceId, String, String)` used consistently after Task 1. HA payloads `{"refresh": int}` and `{"name": str}` match byonk's `DeviceWrite`.
