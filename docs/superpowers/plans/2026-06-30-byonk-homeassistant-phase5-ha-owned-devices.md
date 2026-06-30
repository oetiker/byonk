# Phase 5 — HA-owned devices via discovery — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Home Assistant the source of truth for TRMNL devices: each device becomes its own HA config entry that appears as a native **Discovered card**, byonk ships with no devices, and HA prunes byonk mappings it doesn't own.

**Architecture:** One **hub** config entry (zero-touch add-on connection + shared polling coordinator + global settings) plus **one config entry per TRMNL device** (`unique_id = MAC`, `data = {device_key, hub_entry_id}`). The coordinator injects `integration_discovery` flows for pending devices (→ Discovered cards), reconciles the HA device-entry set against byonk's registered set (removing HA entries and pruning byonk orphans, both 2-strike debounced), and tears down stale cards. byonk persists per-device mappings only as a write-through cache.

**Tech Stack:** Rust (axum, rust-embed, serde_yaml) for byonk core; Python 3.13 + Home Assistant custom component (`pytest-homeassistant-custom-component`) for the integration.

## Global Constraints

- byonk must ship with **zero** device mappings by default (empty `devices: {}`).
- HA is authoritative: a device exists iff HA has a config entry for it. byonk mappings with no HA entry are **orphans** and are deleted by HA (2-strike debounced).
- An HA device entry whose key is no longer registered in byonk is removed by HA (2-strike debounced) — the device-removal grace path.
- Discovery dedup/teardown: never create a duplicate flow for a MAC that already has an entry or an in-progress flow; abort a discovery flow when its device leaves the pending list.
- Python target: 3.13 (HA Core does **not** support 3.14). Tests run via `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`).
- Rust: `make check` (fmt + clippy + tests) must stay green.
- Never `git add -A`/`.` in this repo — stage by explicit path and verify `git diff --cached` before committing.
- Tasks 2–5 are a coordinated refactor (config-entry replaces subentry); each runs its **targeted** tests, and full `make ha-check` is the gate at the end of Task 5.

---

### Task R1: byonk ships an empty default config

**Files:**
- Create: `default-config.yaml`
- Modify: `src/assets.rs` (embed include line + every `EmbeddedConfig::get("config.yaml")` call + the `list_embedded` config name + its test)

**Interfaces:**
- Produces: an embedded default config asset named `default-config.yaml` with `registration`, `auth_mode`, `panels`, `screens`, and `devices: {}` (no demo devices, no `default_screen`). The owner's `config.yaml` stays as a tracked local dev file, no longer embedded.

- [ ] **Step 1: Create `default-config.yaml`**

Create `default-config.yaml` as a clean copy of the non-device sections of `config.yaml`. Concretely: copy `config.yaml`, then (a) replace the entire `devices:` block with `devices: {}`, and (b) delete the trailing `default_screen: default` line. The `registration`, `auth_mode`, `panels`, and `screens` sections are copied verbatim. The file must start with:

```yaml
# Byonk default configuration (shipped / embedded).
# Home Assistant owns devices: this file intentionally has none.
# New (un-onboarded) devices show registration.screen (or the built-in code screen).
registration:
  enabled: true
auth_mode: api_key
```

…followed by the verbatim `panels:` and `screens:` sections from `config.yaml`, and ending with:

```yaml
# Devices are owned by Home Assistant; none ship by default.
devices: {}
```

- [ ] **Step 2: Point the embed at the new file**

In `src/assets.rs`, change the include:

```rust
#[derive(RustEmbed)]
#[folder = "."]
#[include = "default-config.yaml"]
struct EmbeddedConfig;
```

- [ ] **Step 3: Update every embedded-config lookup**

In `src/assets.rs`, replace all four `EmbeddedConfig::get("config.yaml")` occurrences (around lines 241, 247 error text, 334, 408) with `EmbeddedConfig::get("default-config.yaml")`, and update the not-found error message text accordingly. Update `list_embedded` (around line 424):

```rust
AssetCategory::Config => vec!["default-config.yaml".to_string()],
```

- [ ] **Step 4: Update the embedded-config list test**

In `src/assets.rs` `test_list_embedded_config` (around line 584):

```rust
let config = AssetLoader::list_embedded(AssetCategory::Config);
assert_eq!(config.len(), 1);
assert_eq!(config[0], "default-config.yaml");
```

- [ ] **Step 5: Add a test asserting the embedded default has no devices**

Append to the `#[cfg(test)]` module in `src/assets.rs`:

```rust
#[test]
fn test_embedded_default_has_no_devices() {
    // AssetLoader::new(screens_dir, fonts_dir, config_path) — all None = embedded-only.
    let loader = AssetLoader::new(None, None, None);
    let text = loader.read_config_string().expect("read embedded config");
    let cfg: serde_yaml::Value = serde_yaml::from_str(&text).expect("parse embedded config");
    let devices = cfg.get("devices").expect("devices key present");
    let map = devices.as_mapping().expect("devices is a mapping");
    assert!(map.is_empty(), "embedded default config must ship zero devices");
}
```

- [ ] **Step 6: Run the Rust tests**

Run: `cargo test --lib assets 2>&1 | tail -20`
Expected: PASS, including `test_embedded_default_has_no_devices` and `test_list_embedded_config`.

- [ ] **Step 7: Full Rust check**

Run: `make check`
Expected: fmt + clippy clean, all tests pass.

- [ ] **Step 8: Commit**

```bash
git add default-config.yaml src/assets.rs
git commit -m "feat(config): ship empty default config; HA owns devices

Embed default-config.yaml (no demo devices, no default_screen) instead
of the developer's personal config.yaml. A fresh install has zero
devices.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task R2: settings API accepts the new-device screen

**Files:**
- Modify: `src/api/admin/write.rs:211-263` (`SettingsWrite` + `patch_settings`)
- Modify: `src/api/display.rs:289` (treat an empty `registration.screen` as "use built-in")
- Test: `src/api/admin/write.rs` (add unit tests in the file's test module, or extend the existing admin write tests)

**Interfaces:**
- Produces: `PATCH /api/admin/settings` accepts `registration_screen: Option<String>`. A non-empty value is validated against known screens and written to YAML path `["registration", "screen"]` (comment-preserving). An **empty string** is the explicit "built-in" sentinel: it is written as `screen: ''` and `display.rs` treats an empty `registration.screen` exactly like an unset one (built-in code screen). Consumed by the HA new-device-screen select (Task 5), where `""` ↔ a "(built-in)" option.

- [ ] **Step 1: Write the failing test**

Locate the test module in `src/api/admin/write.rs` (or the integration test that exercises `patch_settings`). Add two tests, mirroring the structure of the existing `patch_settings`/`default_screen` test:

1. PATCH `{"registration_screen": "transit"}` against a config whose `screens` contains `transit` → persisted YAML has `screen: transit` under `registration`.
2. PATCH `{"registration_screen": ""}` → succeeds (no unknown-screen error) and persists `screen: ''`.

Example assertion core:

```rust
let body = SettingsWrite {
    registration_enabled: None,
    auth_mode: None,
    default_screen: None,
    registration_screen: Some("transit".to_string()),
};
// ... call patch_settings handler with a state whose screens contains "transit" ...
// then read back the persisted config string:
let written = state.asset_loader.read_config_string().unwrap();
assert!(written.contains("screen: transit"));

// empty string = built-in sentinel, must not be rejected as an unknown screen
let body = SettingsWrite {
    registration_enabled: None, auth_mode: None, default_screen: None,
    registration_screen: Some(String::new()),
};
// ... call patch_settings ... assert it returns Ok(...)
```

- [ ] **Step 2: Run it to confirm it fails to compile / fails**

Run: `cargo test --lib admin 2>&1 | tail -20`
Expected: FAIL — `registration_screen` is not a field of `SettingsWrite`.

- [ ] **Step 3: Add the field**

In `src/api/admin/write.rs`, extend `SettingsWrite`:

```rust
#[derive(Deserialize)]
pub struct SettingsWrite {
    pub(crate) registration_enabled: Option<bool>,
    pub(crate) auth_mode: Option<String>,
    pub(crate) default_screen: Option<String>,
    pub(crate) registration_screen: Option<String>,
}
```

- [ ] **Step 4: Validate and apply it**

In `patch_settings`, add validation alongside the `default_screen` validation block (an empty string is the built-in sentinel and is allowed):

```rust
    if let Some(screen) = &body.registration_screen {
        if !screen.is_empty() && !state.config.load().screens.contains_key(screen) {
            return Err(ApiError::BadRequest(format!("unknown screen `{screen}`")));
        }
    }
```

…and add the mutation alongside the others (after the `default_screen` mutation):

```rust
    if let Some(screen) = &body.registration_screen {
        yaml = config_writer::set_scalar(&yaml, &["registration", "screen"], screen.as_str().into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
```

- [ ] **Step 4b: Treat an empty registration screen as built-in (`display.rs`)**

In `src/api/display.rs`, change the screen resolution (currently line 289):

```rust
            let screen_to_use = config.registration.screen.as_deref().filter(|s| !s.is_empty());
```

This makes `screen: ''` behave identically to an unset `registration.screen` — the built-in code screen — so the HA "(built-in)" option works without a config-removal helper.

- [ ] **Step 5: Run the test**

Run: `cargo test --lib admin 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Full Rust check**

Run: `make check`
Expected: green.

- [ ] **Step 7: Commit**

```bash
git add src/api/admin/write.rs src/api/display.rs
git commit -m "feat(admin): settings PATCH accepts registration_screen

Lets Home Assistant set the new-device screen (registration.screen) via
the admin API, preserving config comments. An empty value selects the
built-in code screen.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 1: integration constants + shared test fixtures

**Files:**
- Modify: `custom_components/byonk/const.py`
- Modify: `tests_ha/conftest.py`

**Interfaces:**
- Produces:
  - `CONF_DEVICE_KEY = "device_key"`, `CONF_HUB_ENTRY_ID = "hub_entry_id"` in `const.py`.
  - conftest helpers used by all later tests: `make_hub_entry(hass)`, `make_device_entry(hass, hub, key)`, and a `byonk` fixture exposing a mutable `state` (lists `devices`, `pending`; dicts `screens`, `config`; AsyncMocks `add_device`, `update_device`, `delete_device`, `update_settings`) with all `ByonkClient` methods + `async_read_token` patched.

- [ ] **Step 1: Add the new constants**

In `custom_components/byonk/const.py`, add below `CONF_BASE_URL`:

```python
CONF_DEVICE_KEY = "device_key"
CONF_HUB_ENTRY_ID = "hub_entry_id"
```

Leave `ISSUE_PENDING_PREFIX` for now (removed in Task 5 with repairs).

- [ ] **Step 2: Add shared fixtures to conftest**

Replace `tests_ha/conftest.py` with:

```python
"""Shared fixtures for Byonk integration tests."""
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import (
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DOMAIN,
)

pytest_plugins = ["pytest_homeassistant_custom_component"]

DEFAULT_SCREENS = {
    "screens": [{"name": "transit", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og"}],
    "dither_algorithms": ["atkinson"],
}


@pytest.fixture(autouse=True)
def auto_enable_custom_integrations(enable_custom_integrations):
    """Enable loading custom integrations in all tests."""
    yield


def make_hub_entry(hass):
    """Add an unloaded hub config entry to hass."""
    entry = MockConfigEntry(
        domain=DOMAIN,
        unique_id=DOMAIN,
        title="Byonk",
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"},
    )
    entry.add_to_hass(hass)
    return entry


def make_device_entry(hass, hub, key):
    """Add an unloaded device config entry to hass."""
    entry = MockConfigEntry(
        domain=DOMAIN,
        unique_id=key,
        title=f"TRMNL {key}",
        data={CONF_DEVICE_KEY: key, CONF_HUB_ENTRY_ID: hub.entry_id},
    )
    entry.add_to_hass(hass)
    return entry


@pytest.fixture
def byonk():
    """Patch ByonkClient + token reader; expose mutable state to each test."""
    state = SimpleNamespace(
        devices=[],
        pending=[],
        screens=DEFAULT_SCREENS,
        config={},
        add_device=AsyncMock(return_value={"key": "x", "screen": "transit"}),
        update_device=AsyncMock(),
        delete_device=AsyncMock(),
        update_settings=AsyncMock(),
    )
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch.multiple(
            "custom_components.byonk.coordinator.ByonkClient",
            async_get_devices=AsyncMock(side_effect=lambda *a, **k: list(state.devices)),
            async_get_pending=AsyncMock(side_effect=lambda *a, **k: list(state.pending)),
            async_get_screens=AsyncMock(side_effect=lambda *a, **k: state.screens),
            async_get_config=AsyncMock(side_effect=lambda *a, **k: state.config),
            async_add_device=state.add_device,
            async_update_device=state.update_device,
            async_delete_device=state.delete_device,
            async_update_settings=state.update_settings,
        ),
    ):
        yield state
```

- [ ] **Step 3: Run the suite to confirm fixtures import cleanly**

Run: `make ha-check 2>&1 | tail -25`
Expected: the existing subentry-based tests still pass (this task only ADDS consts + fixtures; no production behavior changed). Some tests still use their own inline patches — that's fine.

- [ ] **Step 4: Commit**

```bash
git add custom_components/byonk/const.py tests_ha/conftest.py
git commit -m "test(ha): device-entry constants + shared client/fixture helpers

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: per-device config-entry plumbing

> Part of the Tasks 2–5 coordinated swap. Run the **targeted** tests named in each step; full `make ha-check` green is gated at Task 5.

**Files:**
- Modify: `custom_components/byonk/__init__.py`
- Modify: `custom_components/byonk/sensor.py:66-78` (`async_setup_entry`)
- Modify: `custom_components/byonk/select.py:14-32` (`async_setup_entry`)
- Modify: `custom_components/byonk/switch.py:15-18` (`async_setup_entry`)
- Create: `tests_ha/test_device_entry.py`
- Delete: `tests_ha/test_runtime_subentry.py`

**Interfaces:**
- Consumes: `CONF_DEVICE_KEY`, `CONF_HUB_ENTRY_ID` (Task 1); the `byonk` fixture + `make_hub_entry`/`make_device_entry` (Task 1).
- Produces:
  - Hub setup stores the coordinator at `hass.data[DOMAIN][hub_entry_id]` and `entry.runtime_data`.
  - Device setup resolves the hub coordinator via `entry.data[CONF_HUB_ENTRY_ID]`, sets `entry.runtime_data = coordinator`, raises `ConfigEntryNotReady` if the hub isn't loaded.
  - `async_remove_entry` deletes the byonk mapping for a device entry (best-effort).
  - Every platform's `async_setup_entry` branches on `CONF_DEVICE_KEY in entry.data`.

- [ ] **Step 1: Write failing tests for the plumbing**

Create `tests_ha/test_device_entry.py`:

```python
from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState

from custom_components.byonk.const import CONF_DEVICE_KEY, DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {
    "key": "AA:BB", "registered": True, "model": "og",
    "battery_voltage": 4.1, "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00",
    "firmware_version": "1.7.1", "screen": "transit", "params": {},
    "dither": "atkinson", "panel": None,
}


async def test_device_entry_creates_device_and_entities(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    assert dev_entry.state is ConfigEntryState.LOADED
    # the device's diagnostic sensors exist
    assert hass.states.get("sensor.trmnl_aa_bb_battery_voltage") is not None


async def test_device_entry_not_ready_without_hub(hass, byonk):
    hub = make_hub_entry(hass)
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    # hub never set up -> coordinator absent -> device setup retries
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    assert dev_entry.state is ConfigEntryState.SETUP_RETRY


async def test_remove_device_entry_deletes_byonk_mapping(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    with patch(
        "custom_components.byonk.ByonkClient.async_delete_device", new=byonk.delete_device
    ):
        await hass.config_entries.async_remove(dev_entry.entry_id)
        await hass.async_block_till_done()
    assert byonk.delete_device.await_args.args[0] == "AA:BB"
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pytest tests_ha/test_device_entry.py -v 2>&1 | tail -25`
Expected: FAIL — device entries aren't supported yet (setup errors / entity not created).

- [ ] **Step 3: Rewrite `__init__.py`**

Replace `custom_components/byonk/__init__.py` with:

```python
"""The Byonk integration."""
from __future__ import annotations

from homeassistant.config_entries import ConfigEntryState
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed, ConfigEntryNotReady
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .addon import async_read_token
from .api import ByonkApiError, ByonkClient
from .const import (
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DOMAIN,
    PLATFORMS,
)
from .coordinator import ByonkConfigEntry, ByonkCoordinator


async def async_setup_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    if CONF_DEVICE_KEY in entry.data:
        return await _async_setup_device_entry(hass, entry)
    return await _async_setup_hub_entry(hass, entry)


async def _async_setup_hub_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    slug = entry.data[CONF_ADDON_SLUG]
    token = await async_read_token(hass, slug)
    if not token:
        raise ConfigEntryAuthFailed("byonk admin token not provisioned")
    client = ByonkClient(async_get_clientsession(hass), entry.data[CONF_BASE_URL], token)
    coordinator = ByonkCoordinator(hass, entry, client, slug)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator
    hass.data.setdefault(DOMAIN, {})[entry.entry_id] = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    # Device entries that loaded before the hub raised ConfigEntryNotReady; nudge them.
    for dev in hass.config_entries.async_entries(DOMAIN):
        if (
            CONF_DEVICE_KEY in dev.data
            and dev.data.get(CONF_HUB_ENTRY_ID) == entry.entry_id
            and dev.state is ConfigEntryState.SETUP_RETRY
        ):
            hass.async_create_task(hass.config_entries.async_reload(dev.entry_id))
    return True


async def _async_setup_device_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    hub_id = entry.data[CONF_HUB_ENTRY_ID]
    coordinator = hass.data.get(DOMAIN, {}).get(hub_id)
    if coordinator is None:
        raise ConfigEntryNotReady("byonk hub not ready")
    entry.runtime_data = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    unloaded = await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
    if unloaded and CONF_DEVICE_KEY not in entry.data:
        hass.data.get(DOMAIN, {}).pop(entry.entry_id, None)
    return unloaded


async def async_remove_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> None:
    """When a device entry is removed, delete its mapping from byonk (best-effort)."""
    if CONF_DEVICE_KEY not in entry.data:
        return
    hub = hass.config_entries.async_get_entry(entry.data[CONF_HUB_ENTRY_ID])
    if hub is None:
        return
    token = await async_read_token(hass, hub.data[CONF_ADDON_SLUG])
    if not token:
        return
    client = ByonkClient(
        async_get_clientsession(hass), hub.data[CONF_BASE_URL], token
    )
    try:
        await client.async_delete_device(entry.data[CONF_DEVICE_KEY])
    except ByonkApiError:
        pass
```

(Note: `_async_reload_entry` / `add_update_listener` is intentionally dropped — there are no subentries to react to anymore.)

- [ ] **Step 4: Branch `sensor.py` setup**

Replace `async_setup_entry` in `custom_components/byonk/sensor.py` with:

```python
async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        async_add_entities(
            ByonkDeviceSensor(coordinator, key, desc) for desc in DEVICE_SENSORS
        )
        return
    # hub entry: no hub sensors after the pending sensor is removed (Task 5).
    # ByonkPendingSensor still added here until Task 5 removes it.
    async_add_entities([ByonkPendingSensor(coordinator)])
```

Add the import at the top of `sensor.py`:

```python
from .const import CONF_DEVICE_KEY
```

- [ ] **Step 5: Branch `select.py` setup**

Replace `async_setup_entry` in `custom_components/byonk/select.py` with:

```python
async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        async_add_entities(
            [
                ByonkScreenSelect(coordinator, key),
                ByonkDitherSelect(coordinator, key),
                ByonkPanelSelect(coordinator, key),
            ]
        )
        return
    async_add_entities(
        [ByonkDefaultScreenSelect(coordinator), ByonkAuthModeSelect(coordinator)]
    )
```

Add to the imports in `select.py`:

```python
from .const import CONF_DEVICE_KEY
```

(`ByonkDefaultScreenSelect` is renamed to the new-device-screen select in Task 5.)

- [ ] **Step 6: Branch `switch.py` setup**

Replace `async_setup_entry` in `custom_components/byonk/switch.py` with:

```python
async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        return
    async_add_entities([ByonkRegistrationSwitch(entry.runtime_data)])
```

Add to the imports in `switch.py`:

```python
from .const import CONF_DEVICE_KEY
```

- [ ] **Step 7: Delete the obsolete subentry-runtime test**

```bash
git rm tests_ha/test_runtime_subentry.py
```

- [ ] **Step 8: Run the targeted tests**

Run: `pytest tests_ha/test_device_entry.py tests_ha/test_sensor.py tests_ha/test_select.py -v 2>&1 | tail -30`
Expected: `test_device_entry.py` PASS. `test_sensor.py` / `test_select.py` may still fail where they set up devices via subentries — update those two files to use `make_device_entry` (set `byonk.devices`, set up hub, then a device entry) following the pattern in `test_device_entry.py`. Re-run until these three files pass.

- [ ] **Step 9: Commit**

```bash
git add custom_components/byonk/__init__.py custom_components/byonk/sensor.py \
        custom_components/byonk/select.py custom_components/byonk/switch.py \
        tests_ha/test_device_entry.py tests_ha/test_sensor.py tests_ha/test_select.py
git rm tests_ha/test_runtime_subentry.py
git commit -m "feat(ha): per-device config entries replace subentries (plumbing)

Hub stores a shared coordinator in hass.data; device entries resolve it
via hub_entry_id (ConfigEntryNotReady until the hub is up). Platforms
branch on device vs hub entries. Removing a device entry deletes its
byonk mapping.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: discovery config flow (Discovered cards)

> Part of the Tasks 2–5 coordinated swap.

**Files:**
- Modify: `custom_components/byonk/config_flow.py`
- Create: `tests_ha/test_device_flow.py`
- Delete: `tests_ha/test_subentry_flow.py`

**Interfaces:**
- Consumes: `CONF_DEVICE_KEY`, `CONF_HUB_ENTRY_ID`; the hub coordinator at `hub.runtime_data` with `.data` (`screen_names()`, `screen_params()`, `panel_names()`, `dither`) and `.client.async_add_device` / `.async_update_device`.
- Produces: a `ByonkConfigFlow` that (a) creates the hub via `async_step_user` (single hub), (b) creates **device** config entries via `async_step_integration_discovery` → `async_step_configure` → `async_step_dev_params`, and (c) edits params via `async_step_reconfigure`. Discovery data shape: `{"key": mac, "code": registration_code, "model": model}`. Device entry data: `{CONF_DEVICE_KEY: mac, CONF_HUB_ENTRY_ID: hub.entry_id}`.

- [ ] **Step 1: Write failing tests**

Create `tests_ha/test_device_flow.py`:

```python
from homeassistant.config_entries import SOURCE_INTEGRATION_DISCOVERY

from custom_components.byonk.const import CONF_DEVICE_KEY, DOMAIN
from tests_ha.conftest import make_hub_entry

SCREENS_NO_PARAMS = {
    "screens": [{"name": "transit", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"],
}
SCREENS_PARAMS = {
    "screens": [{"name": "transit",
                 "params": [{"name": "limit", "type": "int", "default": 8}],
                 "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"],
}


async def _setup_hub(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_discovery_creates_device_entry_and_posts(hass, byonk):
    byonk.screens = SCREENS_NO_PARAMS
    await _setup_hub(hass)

    result = await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": SOURCE_INTEGRATION_DISCOVERY},
        data={"key": "CC:DD", "code": "ABCD-1234", "model": "og"},
    )
    assert result["type"] == "form"
    assert result["step_id"] == "configure"

    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"screen": "transit"}
    )
    await hass.async_block_till_done()

    assert result["type"] == "create_entry"
    assert byonk.add_device.await_args.args[0]["key"] == "CC:DD"
    assert byonk.add_device.await_args.args[0]["screen"] == "transit"
    entries = [e for e in hass.config_entries.async_entries(DOMAIN)
               if e.data.get(CONF_DEVICE_KEY) == "CC:DD"]
    assert len(entries) == 1


async def test_discovery_with_params_shows_second_form(hass, byonk):
    byonk.screens = SCREENS_PARAMS
    await _setup_hub(hass)
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_INTEGRATION_DISCOVERY},
        data={"key": "CC:DD", "code": "ABCD-1234", "model": "og"},
    )
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"screen": "transit"}
    )
    assert result["type"] == "form"
    assert result["step_id"] == "dev_params"
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"limit": 5}
    )
    await hass.async_block_till_done()
    assert result["type"] == "create_entry"
    assert byonk.add_device.await_args.args[0]["params"] == {"limit": 5}


async def test_discovery_aborts_if_already_configured(hass, byonk):
    byonk.screens = SCREENS_NO_PARAMS
    hub = await _setup_hub(hass)
    from tests_ha.conftest import make_device_entry
    make_device_entry(hass, hub, "CC:DD")
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_INTEGRATION_DISCOVERY},
        data={"key": "CC:DD", "code": "ABCD-1234", "model": "og"},
    )
    assert result["type"] == "abort"
    assert result["reason"] == "already_configured"


async def test_hub_single_instance(hass, byonk):
    await _setup_hub(hass)
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": "user"}
    )
    assert result["type"] == "abort"
    assert result["reason"] == "single_instance_allowed"
```

- [ ] **Step 2: Run it to verify failure**

Run: `pytest tests_ha/test_device_flow.py -v 2>&1 | tail -30`
Expected: FAIL — no `async_step_integration_discovery`.

- [ ] **Step 3: Rewrite the config flow**

In `custom_components/byonk/config_flow.py`: update imports, drop the subentry types, fix the hub single-instance check, and add the discovery + configure + dev_params + reconfigure steps.

Replace the import block (lines 9–30) with:

```python
import voluptuous as vol
from homeassistant.components.hassio import AddonError
from homeassistant.config_entries import (
    ConfigFlow,
    ConfigFlowResult,
)
from homeassistant.core import callback
from homeassistant.helpers import selector
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.hassio import is_hassio

from .addon import (
    async_ensure_addon_installed,
    async_get_base_url,
    async_provision_token,
    async_read_token,
)
from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import (
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DOMAIN,
)
from .param_form import build_params_schema
```

Keep `_async_probe_ready` and `_token_authenticates` unchanged.

Replace the `ByonkConfigFlow` class body. Remove `async_get_supported_subentry_types`. Change the single-instance check in `async_step_user` from `if self._async_current_entries():` to:

```python
        if any(e.unique_id == DOMAIN for e in self._async_current_entries(include_ignore=False)):
            return self.async_abort(reason="single_instance_allowed")
```

Add an `__init__` and the new steps to `ByonkConfigFlow`:

```python
    def __init__(self) -> None:
        self._discovery: dict[str, Any] = {}
        self._key: str | None = None
        self._screen: str | None = None
        self._extra: dict[str, Any] = {}

    @callback
    def _hub_entry(self):
        for entry in self._async_current_entries(include_ignore=False):
            if entry.unique_id == DOMAIN:
                return entry
        return None

    async def async_step_integration_discovery(
        self, discovery_info: dict[str, Any]
    ) -> ConfigFlowResult:
        mac = discovery_info["key"]
        await self.async_set_unique_id(mac)
        self._abort_if_unique_id_configured()
        self._discovery = discovery_info
        self.context["title_placeholders"] = {
            "name": f"TRMNL {mac}",
            "code": discovery_info.get("code") or mac,
        }
        return await self.async_step_configure()

    async def async_step_configure(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        hub = self._hub_entry()
        if hub is None:
            return self.async_abort(reason="no_hub")
        data = hub.runtime_data.data
        if user_input is not None:
            self._key = self._discovery["key"]
            self._screen = user_input["screen"]
            self._extra = {
                k: user_input[k] for k in ("panel", "dither") if user_input.get(k)
            }
            return await self.async_step_dev_params()

        schema = vol.Schema(
            {
                vol.Required("screen"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=data.screen_names(),
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Optional("dither"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=data.dither, mode=selector.SelectSelectorMode.DROPDOWN
                    )
                ),
                vol.Optional("panel"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=data.panel_names(),
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
            }
        )
        return self.async_show_form(
            step_id="configure",
            data_schema=schema,
            description_placeholders={
                "code": self._discovery.get("code") or self._discovery["key"]
            },
        )

    async def async_step_dev_params(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        hub = self._hub_entry()
        coordinator = hub.runtime_data
        fields = coordinator.data.screen_params(self._screen)
        if user_input is not None or not fields:
            params = user_input or {}
            payload = {
                "key": self._key, "screen": self._screen, "params": params, **self._extra
            }
            await coordinator.client.async_add_device(payload)
            return self.async_create_entry(
                title=f"TRMNL {self._key}",
                data={CONF_DEVICE_KEY: self._key, CONF_HUB_ENTRY_ID: hub.entry_id},
            )
        return self.async_show_form(
            step_id="dev_params", data_schema=build_params_schema(fields)
        )

    async def async_step_reconfigure(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        entry = self._get_reconfigure_entry()
        self._key = entry.data[CONF_DEVICE_KEY]
        hub = self._hub_entry()
        coordinator = hub.runtime_data
        device = next(
            (d for d in coordinator.data.devices if d["key"] == self._key), {}
        )
        self._screen = device.get("screen")
        fields = coordinator.data.screen_params(self._screen)
        if user_input is not None or not fields:
            params = user_input or {}
            await coordinator.client.async_update_device(
                self._key, {"screen": self._screen, "params": params}
            )
            await coordinator.async_request_refresh()
            return self.async_abort(reason="reconfigure_successful")
        return self.async_show_form(
            step_id="reconfigure",
            data_schema=build_params_schema(fields, current=device.get("params") or {}),
        )
```

Delete the entire `ByonkDeviceSubentryFlow` class. Add `from typing import Any` if not already imported (it is, via the existing `from typing import Any`).

- [ ] **Step 4: Delete the subentry-flow test**

```bash
git rm tests_ha/test_subentry_flow.py
```

- [ ] **Step 5: Run the targeted tests**

Run: `pytest tests_ha/test_device_flow.py tests_ha/test_config_flow.py -v 2>&1 | tail -30`
Expected: `test_device_flow.py` PASS. Update `test_config_flow.py` where it asserts old single-instance behavior (it should still abort `single_instance_allowed` when a hub exists; remove any assertions referencing `async_get_supported_subentry_types` or subentry flows). Re-run until both pass.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/config_flow.py tests_ha/test_device_flow.py \
        tests_ha/test_config_flow.py
git rm tests_ha/test_subentry_flow.py
git commit -m "feat(ha): discovery config flow creates per-device entries

Pending TRMNLs onboard via async_step_integration_discovery -> configure
-> params, producing a device config entry and POSTing the mapping to
byonk. Hub stays single-instance; reconfigure edits params.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: coordinator reconcile — discovery, removal grace, orphan prune

> Part of the Tasks 2–5 coordinated swap.

**Files:**
- Modify: `custom_components/byonk/coordinator.py`
- Create: `tests_ha/test_reconcile.py` (replaces the old subentry version)

**Interfaces:**
- Consumes: `data.devices` (each `{"key", "registered", ...}`), `data.pending` (each `{"mac", "registration_code", "model"}`); `self.client.async_delete_device(key)`; HA flow manager (`flow.async_init`, `flow.async_progress_by_handler`, `flow.async_abort`) and `config_entries.async_remove`.
- Produces: a coordinator whose every update (a) injects `integration_discovery` flows for new pending devices, (b) removes device config entries no longer registered in byonk (2-strike), (c) deletes byonk orphan mappings with no HA entry (2-strike), (d) aborts discovery flows whose device left the pending list. No more Repairs sync, no subentry creation.

**Note on the dropped "write-missing" safety net:** the spec listed "HA entry exists, byonk mapping missing → write it back" as a safety net. byonk persists device mappings to `config.yaml`, so this state does not arise in normal operation; and the HA device entry stores only the key (not the screen), so there is nothing to write back. We therefore implement the symmetric, achievable rule instead: an HA device entry whose key is not in byonk's registered set is **removed** after 2 strikes (the device-removal grace path). This keeps the two sets converging without inventing a screen assignment HA never had.

- [ ] **Step 1: Write failing tests**

Create `tests_ha/test_reconcile.py`:

```python
from homeassistant.config_entries import SOURCE_INTEGRATION_DISCOVERY

from custom_components.byonk.const import CONF_DEVICE_KEY, DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {
    "key": "AA:BB", "registered": True, "model": "og",
    "battery_voltage": 4.1, "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00",
    "firmware_version": "1.7.1", "screen": "transit", "dither": "atkinson", "panel": None,
}
PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og",
            "last_seen": "2026-06-29T09:00:00+00:00"}]


async def _setup_hub(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_pending_injects_discovery_flow(hass, byonk):
    byonk.pending = PENDING
    await _setup_hub(hass)
    await hass.async_block_till_done()
    flows = hass.config_entries.flow.async_progress_by_handler(DOMAIN)
    disc = [f for f in flows if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY]
    assert len(disc) == 1


async def test_no_duplicate_discovery_flow(hass, byonk):
    byonk.pending = PENDING
    hub = await _setup_hub(hass)
    coordinator = hub.runtime_data
    await coordinator.async_refresh()
    await hass.async_block_till_done()
    disc = [f for f in hass.config_entries.flow.async_progress_by_handler(DOMAIN)
            if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY]
    assert len(disc) == 1


async def test_discovery_flow_torn_down_when_no_longer_pending(hass, byonk):
    byonk.pending = PENDING
    hub = await _setup_hub(hass)
    assert hass.config_entries.flow.async_progress_by_handler(DOMAIN)
    byonk.pending = []
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    disc = [f for f in hass.config_entries.flow.async_progress_by_handler(DOMAIN)
            if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY]
    assert not disc


async def test_device_removed_after_two_strikes(hass, byonk):
    byonk.devices = [DEV]
    hub = await _setup_hub(hass)
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    coordinator = hub.runtime_data
    byonk.devices = []  # device deregistered in byonk

    await coordinator.async_refresh()  # strike 1
    await hass.async_block_till_done()
    assert hass.config_entries.async_get_entry(dev_entry.entry_id) is not None

    await coordinator.async_refresh()  # strike 2
    await hass.async_block_till_done()
    assert hass.config_entries.async_get_entry(dev_entry.entry_id) is None


async def test_orphan_byonk_mapping_pruned_after_two_strikes(hass, byonk):
    # byonk reports a registered device that HA has no entry for
    byonk.devices = [DEV]
    hub = await _setup_hub(hass)
    coordinator = hub.runtime_data

    await coordinator.async_refresh()  # strike 1 — not deleted yet
    await hass.async_block_till_done()
    assert not byonk.delete_device.called

    await coordinator.async_refresh()  # strike 2 — pruned
    await hass.async_block_till_done()
    assert byonk.delete_device.await_args.args[0] == "AA:BB"
```

- [ ] **Step 2: Run it to verify failure**

Run: `pytest tests_ha/test_reconcile.py -v 2>&1 | tail -30`
Expected: FAIL — the coordinator still does subentry reconcile + repairs.

- [ ] **Step 3: Rewrite `coordinator.py`**

Replace `custom_components/byonk/coordinator.py` with:

```python
"""Data coordinator for byonk."""
from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import timedelta
import logging

from homeassistant.config_entries import SOURCE_INTEGRATION_DISCOVERY, ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import CONF_DEVICE_KEY, DOMAIN, UPDATE_INTERVAL_SECONDS

_LOGGER = logging.getLogger(__name__)

type ByonkConfigEntry = ConfigEntry["ByonkCoordinator"]

REMOVE_STRIKES = 2


@dataclass(frozen=True)
class ByonkData:
    devices: list[dict]
    pending: list[dict]
    screens: list[dict]
    panels: list[dict]
    dither: list[str]
    config: dict

    def screen_names(self) -> list[str]:
        return [s["name"] for s in self.screens]

    def panel_names(self) -> list[str]:
        return [p["name"] for p in self.panels]

    def screen_params(self, name: str) -> list[dict]:
        for s in self.screens:
            if s["name"] == name:
                return s.get("params") or []
        return []

    def registration_screen(self) -> str | None:
        return self.config.get("registration", {}).get("screen")

    def default_screen(self) -> str | None:
        # Retained only so the not-yet-renamed hub select keeps working between
        # this task and Task 5, where the select is repurposed and this is removed.
        return self.config.get("default_screen")

    def registration_enabled(self) -> bool:
        return bool(self.config.get("registration", {}).get("enabled", False))

    def auth_mode(self) -> str | None:
        return self.config.get("auth_mode")


class ByonkCoordinator(DataUpdateCoordinator[ByonkData]):
    def __init__(
        self, hass: HomeAssistant, entry: ByonkConfigEntry, client: ByonkClient, slug: str
    ) -> None:
        super().__init__(
            hass,
            _LOGGER,
            name=DOMAIN,
            update_interval=timedelta(seconds=UPDATE_INTERVAL_SECONDS),
            always_update=False,
        )
        self.client = client
        self.entry = entry
        self.slug = slug
        self._remove_strikes: dict[str, int] = {}
        self._orphan_strikes: dict[str, int] = {}

    async def _async_update_data(self) -> ByonkData:
        try:
            devices, pending, screens, config = await asyncio.gather(
                self.client.async_get_devices(),
                self.client.async_get_pending(),
                self.client.async_get_screens(),
                self.client.async_get_config(),
            )
        except ByonkAuthError as err:
            raise ConfigEntryAuthFailed(str(err)) from err
        except ByonkApiError as err:
            raise UpdateFailed(str(err)) from err
        data = ByonkData(
            devices=devices,
            pending=pending,
            screens=screens.get("screens", []),
            panels=screens.get("panels", []),
            dither=screens.get("dither_algorithms", []),
            config=config,
        )
        await self._async_reconcile(data)
        self._async_sync_discovery(data)
        return data

    def _device_entries(self) -> dict[str, ConfigEntry]:
        return {
            e.data[CONF_DEVICE_KEY]: e
            for e in self.hass.config_entries.async_entries(DOMAIN)
            if CONF_DEVICE_KEY in e.data
        }

    async def _async_reconcile(self, data: ByonkData) -> None:
        device_entries = self._device_entries()
        ha_keys = set(device_entries)
        byonk_registered = {d["key"] for d in data.devices if d.get("registered")}

        for key in ha_keys & byonk_registered:
            self._remove_strikes.pop(key, None)
            self._orphan_strikes.pop(key, None)

        # HA entry exists, byonk no longer registers it -> remove HA entry (grace).
        for key in ha_keys - byonk_registered:
            self._remove_strikes[key] = self._remove_strikes.get(key, 0) + 1
            if self._remove_strikes[key] >= REMOVE_STRIKES:
                self._remove_strikes.pop(key, None)
                self.hass.async_create_task(
                    self.hass.config_entries.async_remove(device_entries[key].entry_id)
                )

        # byonk registers a device HA has no entry for -> orphan; delete from byonk (grace).
        for key in byonk_registered - ha_keys:
            self._orphan_strikes[key] = self._orphan_strikes.get(key, 0) + 1
            if self._orphan_strikes[key] >= REMOVE_STRIKES:
                self._orphan_strikes.pop(key, None)
                try:
                    await self.client.async_delete_device(key)
                except ByonkApiError as err:
                    _LOGGER.warning("orphan prune failed for %s: %s", key, err)

    def _async_sync_discovery(self, data: ByonkData) -> None:
        pending_macs = {p["mac"] for p in data.pending}
        configured = set(self._device_entries())
        flows = self.hass.config_entries.flow.async_progress_by_handler(
            DOMAIN, include_uninitialized=True
        )
        discovery_flows = [
            f for f in flows
            if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY
        ]
        in_progress = {f["context"].get("unique_id") for f in discovery_flows}

        for p in data.pending:
            mac = p["mac"]
            if mac in configured or mac in in_progress:
                continue
            self.hass.async_create_task(
                self.hass.config_entries.flow.async_init(
                    DOMAIN,
                    context={"source": SOURCE_INTEGRATION_DISCOVERY},
                    data={
                        "key": mac,
                        "code": p.get("registration_code"),
                        "model": p.get("model"),
                    },
                )
            )

        for f in discovery_flows:
            uid = f["context"].get("unique_id")
            if uid and uid not in pending_macs:
                self.hass.config_entries.flow.async_abort(f["flow_id"])
```

- [ ] **Step 4: Run the targeted tests**

Run: `pytest tests_ha/test_reconcile.py -v 2>&1 | tail -30`
Expected: PASS. If `test_no_duplicate_discovery_flow` is flaky on the `async_init` task timing, ensure `await hass.async_block_till_done()` follows each refresh (it does in the test).

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/coordinator.py tests_ha/test_reconcile.py
git commit -m "feat(ha): coordinator drives discovery, removal grace, orphan prune

Inject integration_discovery flows for pending devices; remove device
entries no longer registered in byonk (2-strike); prune byonk mappings
with no HA entry (2-strike); tear down stale discovery cards. Drops
subentry reconcile and Repairs sync.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: hub new-device-screen select; remove pending sensor + Repairs

> Final task of the Tasks 2–5 coordinated swap — full `make ha-check` must be green at the end.

**Files:**
- Modify: `custom_components/byonk/select.py` (rename `ByonkDefaultScreenSelect` → `ByonkNewDeviceScreenSelect`, drive `registration_screen` with a "(built-in)" option)
- Modify: `custom_components/byonk/sensor.py` (remove `ByonkPendingSensor`)
- Modify: `custom_components/byonk/coordinator.py` (remove the now-dead `default_screen()` method from `ByonkData`)
- Modify: `custom_components/byonk/const.py` (remove `ISSUE_PENDING_PREFIX`; add `BUILTIN_SCREEN_LABEL`)
- Delete: `custom_components/byonk/repairs.py`, `tests_ha/test_repairs.py`, `tests_ha/test_pending_sensor.py`
- Modify: `tests_ha/test_settings_entities.py`

**Interfaces:**
- Consumes: `ByonkData.registration_screen()`; `client.async_update_settings({"registration_screen": option})` (backed by Task R2, where `""` selects the built-in screen).
- Produces: a hub `select` entity (`translation_key="new_device_screen"`) whose options are `[BUILTIN_SCREEN_LABEL, *screen_names()]`; the current value is the configured `registration.screen`, or `BUILTIN_SCREEN_LABEL` when it is unset/empty. Selecting `BUILTIN_SCREEN_LABEL` writes `registration_screen=""`. No pending sensor, no Repairs issues.

- [ ] **Step 1: Write/adjust the failing test**

In `tests_ha/test_settings_entities.py`, replace references to the default-screen select with the new-device-screen select. Add/replace these tests:

```python
from custom_components.byonk.const import BUILTIN_SCREEN_LABEL
from tests_ha.conftest import make_hub_entry


async def test_new_device_screen_select(hass, byonk):
    byonk.config = {"registration": {"enabled": True, "screen": "transit"}}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    state = hass.states.get("select.byonk_new_device_screen")
    assert state is not None
    assert state.state == "transit"

    await hass.services.async_call(
        "select", "select_option",
        {"entity_id": "select.byonk_new_device_screen", "option": "transit"},
        blocking=True,
    )
    assert byonk.update_settings.await_args.args[0] == {"registration_screen": "transit"}


async def test_new_device_screen_builtin(hass, byonk):
    # no registration.screen configured -> shows the built-in label
    byonk.config = {"registration": {"enabled": True}}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    state = hass.states.get("select.byonk_new_device_screen")
    assert state.state == BUILTIN_SCREEN_LABEL

    await hass.services.async_call(
        "select", "select_option",
        {"entity_id": "select.byonk_new_device_screen", "option": BUILTIN_SCREEN_LABEL},
        blocking=True,
    )
    assert byonk.update_settings.await_args.args[0] == {"registration_screen": ""}
```

(Match the existing `test_settings_entities.py` setup style; use the `byonk` fixture. Remove any prior `default_screen` select test.)

- [ ] **Step 2: Run it to verify failure**

Run: `pytest tests_ha/test_settings_entities.py -v 2>&1 | tail -20`
Expected: FAIL — entity `select.byonk_new_device_screen` doesn't exist.

- [ ] **Step 3a: Add the built-in label constant + drop the dead method**

In `custom_components/byonk/const.py`, add:

```python
BUILTIN_SCREEN_LABEL = "(built-in)"
```

In `custom_components/byonk/coordinator.py`, delete the `default_screen()` method from `ByonkData` (added temporarily in Task 4).

- [ ] **Step 3b: Rename + repurpose the hub select**

In `custom_components/byonk/select.py`, replace the `ByonkDefaultScreenSelect` class with:

```python
class ByonkNewDeviceScreenSelect(ByonkHubEntity, SelectEntity):
    _attr_translation_key = "new_device_screen"
    _attr_entity_category = EntityCategory.CONFIG

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_new_device_screen"

    @property
    def options(self) -> list[str]:
        return [BUILTIN_SCREEN_LABEL, *self.coordinator.data.screen_names()]

    @property
    def current_option(self) -> str | None:
        screen = self.coordinator.data.registration_screen()
        return screen or BUILTIN_SCREEN_LABEL

    async def async_select_option(self, option: str) -> None:
        value = "" if option == BUILTIN_SCREEN_LABEL else option
        await self.coordinator.client.async_update_settings(
            {"registration_screen": value}
        )
        await self.coordinator.async_request_refresh()
```

Add to the imports in `select.py`:

```python
from .const import BUILTIN_SCREEN_LABEL, CONF_DEVICE_KEY
```

(Replace the existing `from .const import CONF_DEVICE_KEY` line added in Task 2.)

Update the hub branch in `async_setup_entry` (from Task 2 Step 5) to use the new class name:

```python
    async_add_entities(
        [ByonkNewDeviceScreenSelect(coordinator), ByonkAuthModeSelect(coordinator)]
    )
```

- [ ] **Step 4: Remove the pending sensor**

In `custom_components/byonk/sensor.py`: delete the `ByonkPendingSensor` class entirely, and change the hub branch of `async_setup_entry` (from Task 2 Step 4) to add no hub sensors:

```python
    # hub entry: no diagnostic hub sensors (pending devices surface as Discovered cards)
    return
```

Remove the now-unused `ByonkHubEntity` import from `sensor.py` if nothing else uses it.

- [ ] **Step 5: Remove Repairs**

```bash
git rm custom_components/byonk/repairs.py tests_ha/test_repairs.py tests_ha/test_pending_sensor.py
```

In `custom_components/byonk/const.py`, delete the lines:

```python
# Repairs
ISSUE_PENDING_PREFIX = "device_pending_"
```

Confirm no remaining imports of `repairs` or `ISSUE_PENDING_PREFIX`:

Run: `grep -rn "repairs\|ISSUE_PENDING_PREFIX" custom_components/byonk tests_ha`
Expected: no matches.

- [ ] **Step 6: Run the full HA suite**

Run: `make ha-check 2>&1 | tail -30`
Expected: ruff clean; **all** `tests_ha` pass. Fix any stragglers (e.g. `test_init.py` references to the dropped update listener, or any leftover subentry assertions) by converting them to the device-entry / `byonk`-fixture pattern.

- [ ] **Step 7: Commit**

```bash
git add custom_components/byonk/select.py custom_components/byonk/sensor.py \
        custom_components/byonk/coordinator.py custom_components/byonk/const.py \
        tests_ha/test_settings_entities.py
git rm custom_components/byonk/repairs.py tests_ha/test_repairs.py tests_ha/test_pending_sensor.py
git commit -m "feat(ha): new-device-screen select; drop pending sensor + Repairs

Hub gains a select for registration.screen (the new-device screen).
Pending devices now surface only as Discovered cards, so the pending
sensor and Repairs issues are removed.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: translations / strings

**Files:**
- Modify: `custom_components/byonk/strings.json`
- Modify: `custom_components/byonk/translations/en.json`

**Interfaces:**
- Produces: user-facing copy for the discovery flow steps (`configure`, `dev_params`, `reconfigure`), the `no_hub` abort, the discovered-card title, the renamed `new_device_screen` select; removal of the subentry, `device_pending` issue, and `pending_devices` sensor strings.

- [ ] **Step 1: Update `strings.json`**

Replace `custom_components/byonk/strings.json` with:

```json
{
  "config": {
    "flow_title": "{name}",
    "step": {
      "configure": {
        "title": "Set up TRMNL device",
        "description": "Device showing code {code}. Choose the screen it should display.",
        "data": {
          "screen": "Screen",
          "dither": "Dither algorithm",
          "panel": "Panel type"
        }
      },
      "dev_params": {
        "title": "Screen parameters",
        "description": "Configure the screen's dynamic parameters."
      },
      "reconfigure": {
        "title": "Reconfigure device",
        "description": "Update the screen parameters for this device."
      }
    },
    "abort": {
      "not_hassio": "Byonk requires the Byonk add-on, which needs a Home Assistant Supervised or HAOS installation.",
      "single_instance_allowed": "Byonk is already configured.",
      "already_configured": "This device is already set up.",
      "no_hub": "The Byonk hub is not set up yet. Set up the Byonk integration first.",
      "addon_error": "Could not install or start the Byonk add-on automatically. Add the repository https://github.com/oetiker/byonk to the add-on store, install the Byonk add-on, then retry.",
      "addon_unhealthy": "The Byonk add-on started, but its management API did not respond as expected. Make sure the Byonk add-on is up to date (a version that supports the admin API), then retry.",
      "reconfigure_successful": "Device updated."
    }
  },
  "entity": {
    "switch": {
      "registration_enabled": {
        "name": "Registration enabled"
      }
    },
    "select": {
      "screen": {
        "name": "Screen"
      },
      "dither": {
        "name": "Dither"
      },
      "panel": {
        "name": "Panel"
      },
      "new_device_screen": {
        "name": "New device screen"
      },
      "auth_mode": {
        "name": "Auth mode"
      }
    },
    "sensor": {
      "battery_voltage": {
        "name": "Battery voltage"
      },
      "rssi": {
        "name": "Signal strength"
      },
      "last_seen": {
        "name": "Last seen"
      },
      "firmware_version": {
        "name": "Firmware version"
      },
      "model": {
        "name": "Model"
      }
    }
  }
}
```

- [ ] **Step 2: Mirror into `translations/en.json`**

Copy the exact same content into `custom_components/byonk/translations/en.json`.

- [ ] **Step 3: Verify they parse and match**

Run: `python -c "import json,sys; a=json.load(open('custom_components/byonk/strings.json')); b=json.load(open('custom_components/byonk/translations/en.json')); sys.exit(0 if a==b else 1)" && echo OK`
Expected: `OK`.

- [ ] **Step 4: Run the manifest/translation test**

Run: `pytest tests_ha/test_manifest.py -v 2>&1 | tail -15`
Expected: PASS (update `test_manifest.py` if it asserted any removed keys).

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/strings.json custom_components/byonk/translations/en.json
git commit -m "i18n(ha): discovery flow copy; drop subentry/pending strings

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: documentation + CHANGES

**Files:**
- Modify: `docs/src/guide/ha-integration.md`
- Modify: `CHANGES.md`

**Interfaces:**
- Produces: user docs describing the discovery-card onboarding model and the HA-owned-devices philosophy; a CHANGES.md Unreleased entry.

- [ ] **Step 1: Update the HA integration guide**

In `docs/src/guide/ha-integration.md`, replace the onboarding section so it describes the new model: byonk ships with no devices; a new TRMNL appears as a **Discovered** card under _Settings → Devices & Services_; click **Configure**, pick a screen, and it becomes an HA device. Note that removing the HA device removes it from byonk, and that byonk mappings with no HA device are pruned automatically. Remove any text describing the "Add device" subentry flow, the pending-devices sensor, and the Repairs warnings.

- [ ] **Step 2: Add a CHANGES.md entry**

Under the `## [Unreleased]` section in `CHANGES.md`, add:

```markdown
### Changed
- Home Assistant integration: TRMNL devices are now Home Assistant–owned. A new
  device appears as a native **Discovered** card; configuring it creates a
  per-device config entry and writes the mapping to byonk. Home Assistant is the
  source of truth — byonk ships with no devices and mappings without a matching
  HA device are pruned automatically. Replaces the previous subentry +
  Repairs-issue onboarding.

### Added
- Admin settings API accepts `registration_screen` to set the screen shown to
  new (un-onboarded) devices.
```

- [ ] **Step 3: Build the docs**

Run: `make docs 2>&1 | tail -15`
Expected: mdBook build succeeds, no broken-link errors.

- [ ] **Step 4: Commit**

```bash
git add docs/src/guide/ha-integration.md CHANGES.md
git commit -m "docs(ha): document HA-owned discovery onboarding + CHANGES

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: full verification

**Files:** none (verification only).

- [ ] **Step 1: Rust**

Run: `make check`
Expected: fmt + clippy clean; all tests pass (incl. `test_embedded_default_has_no_devices`, the `registration_screen` test).

- [ ] **Step 2: Python integration**

Run: `make ha-check`
Expected: ruff clean; all `tests_ha` pass. Confirm the deleted test files (`test_subentry_flow.py`, `test_runtime_subentry.py`, `test_repairs.py`, `test_pending_sensor.py`) are gone and replaced by `test_device_entry.py`, `test_device_flow.py`, and the rewritten `test_reconcile.py`.

- [ ] **Step 3: Docs**

Run: `make docs`
Expected: clean build.

- [ ] **Step 4: Grep for leftovers**

Run: `grep -rn "subentry\|ISSUE_PENDING\|ByonkPendingSensor\|ByonkDefaultScreenSelect\|default_screen" custom_components/byonk`
Expected: no matches (all subentry/pending/default-screen references removed; `registration_screen` is the replacement).

- [ ] **Step 5: Manual VM validation (from the Phase 4 to-do, now updated)**

On the HAOS VM with a from-source byonk add-on (see `tools/ha-vm/README.md`), remove and re-add the Byonk integration, then verify:
- a fresh install shows **no** demo devices;
- a real pending TRMNL (e.g. `94:A9:90:8C:6D:18`, code `GRFQSRWNSQ`) appears as a **Discovered** card;
- configuring it creates an HA device whose telemetry populates and whose screen renders;
- deleting the HA device removes the mapping from byonk;
- editing byonk's `config.yaml` to add an unknown mapping results in that orphan being pruned within two polls.

This step is manual and not gated by `make`; record results in `.superpowers/sdd/progress.md`.

---

## Notes on deviations from the spec

byonk is ours and has no userbase yet, so byonk's API is changed freely wherever HA's
needs call for it (no backward-compat constraints): Task R2 adds `registration_screen` to
the settings API, and Task R1 changes the embedded default config.

- **New-device-screen "built-in default" option:** implemented in full (Tasks R2 + 5). The
  select offers a `(built-in)` option that writes `registration_screen=""`; `display.rs`
  treats an empty `registration.screen` as the built-in code screen. No config-removal
  helper was needed.
- **"Write-missing" reconcile bullet:** replaced with HA-entry removal grace (see Task 4
  note). This is *not* an API limitation — byonk persists mappings to `config.yaml`, so the
  state doesn't arise, and the HA device entry deliberately stores only the key (byonk holds
  the live screen assignment), so there is no screen to write back. If we later want HA to
  be able to restore a lost mapping, the clean change is to store the screen in the device
  entry's data and push it on reconcile — call that out before implementing.
