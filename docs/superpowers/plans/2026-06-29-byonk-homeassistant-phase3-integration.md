# Byonk HA Integration (Phase 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Python Home Assistant custom integration (`custom_components/byonk/`) that installs+manages the byonk add-on with zero user-entered credentials and exposes byonk devices/settings as HA entities.

**Architecture:** Supervised/HAOS-only integration. A config flow auto-installs the byonk add-on (adds its community store repo, installs, starts) and provisions the admin token into the add-on option, then reads it back at runtime. A single `DataUpdateCoordinator` polls the Phase 1 admin API and reconciles byonk's `config.yaml` into one hub HA Device + one config-subentry/HA-Device per TRMNL device. Entities (sensors + selects + switch) read coordinator data and write back through the admin API; a subentry form edits dynamic per-screen `@params`; Repairs nudge onboarding.

**Tech Stack:** Python 3.12+, Home Assistant Core (custom integration), `aiohasupervisor` (via `homeassistant.components.hassio`), `voluptuous` + HA selectors, `pytest` + `pytest-homeassistant-custom-component`, `ruff`.

## Global Constraints

- **Supervised/HAOS-only.** All Supervisor use guarded by `homeassistant.components.hassio.is_hassio(hass)`; on non-Supervisor the config flow aborts with reason `not_hassio`. Manifest uses `after_dependencies: ["hassio"]` (NOT hard `dependencies`).
- **Zero-touch trust.** The user never enters/copies a byonk token. The admin token's single source of truth is the add-on option (`/data/options.json`); the integration provisions it and reads it back at runtime. **The config entry stores NO token.** No token is ever logged.
- **No redundancy.** byonk's `config.yaml` is authoritative for all device mappings/settings; the integration mirrors via the admin API and never keeps its own copy of byonk-owned state.
- **Add-on identity:** `BYONK_ADDON_REPO_URL = "https://github.com/oetiker/byonk"`; add-on config-slug = `"byonk"`; installable slug = `"<repo_hash>_byonk"` where `repo_hash = sha1(url.lower())[:8]` — **discover it via `store.addons_list()`, never hard-code the hash**.
- **Admin API contract (Phase 1):** Bearer token; 404 = admin dormant/unprovisioned, 401 = wrong token, 400 = validation (body carries message), 409 = config read-only. On `PATCH /api/admin/devices/:key`, **`params` is a FULL replacement** — always send the complete param set.
- **manifest values:** `integration_type: "hub"`, `iot_class: "local_polling"`, `config_flow: true`, `codeowners: ["@oetiker"]`, `documentation`/`issue_tracker` = the byonk repo. No external pip `requirements`.
- **Selector mapping** (`@params` → `homeassistant.helpers.selector`): `string`→`TextSelector(TEXT)` (`sensitive`→`PASSWORD`, `multiline`→multiline), `url`→`TextSelector(URL)`, `int`→`NumberSelector(step=1, BOX)`, `float`→`NumberSelector(step="any")`, `bool`→`BooleanSelector`, `enum`→`SelectSelector(DROPDOWN)`, `color`→`TextSelector(COLOR)` (byonk colors are hex strings). `required`→`vol.Required`, optional→`vol.Optional` + `suggested_value` on edit.
- **Commit style:** end commit messages with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- **Don't break the Rust build:** never touch `src/`, `tests/*.rs`, or `Cargo.*`. Python lives only in `custom_components/`, `tests_ha/`, root `pyproject.toml`, `requirements_test.txt`, and new Makefile targets.

---

## File Structure

```
custom_components/byonk/
  __init__.py        # async_setup_entry/async_unload_entry, PLATFORMS, runtime wiring
  manifest.json      # integration manifest
  const.py           # DOMAIN, repo URL, slug, intervals, platform list, issue ids
  api.py             # ByonkClient (admin API) + typed errors
  addon.py           # Supervisor: ensure/install/start add-on, provision + read-back token, base URL
  coordinator.py     # ByonkData, ByonkCoordinator (poll + reconcile), ByonkConfigEntry type
  config_flow.py     # ByonkConfigFlow (trust) + ByonkDeviceSubentryFlow (device add/edit)
  param_form.py      # build_params_schema(), default_params() — @params -> voluptuous/selectors
  entity.py          # ByonkHubEntity, ByonkDeviceEntity base classes (DeviceInfo)
  sensor.py          # per-device telemetry sensors + hub pending_devices sensor
  select.py          # per-device screen/dither/panel + hub default_screen/auth_mode
  switch.py          # hub registration_enabled
  repairs.py         # async_sync_pending_issues() (pending -> issue registry)
  strings.json       # config-flow + entity + issue strings
  translations/en.json
hacs.json            # repo root: minimal HACS custom-repo metadata
pyproject.toml       # repo root: [tool.pytest.ini_options] ONLY (no build system)
requirements_test.txt# pytest-homeassistant-custom-component pin
tests_ha/            # Python tests (isolated from Rust tests/)
  conftest.py
  test_*.py
```

Each task below ends with an independently testable deliverable and a commit.

---

## Stage 1 — Trust + skeleton

### Task 1: Project scaffolding, manifest, test harness

**Files:**
- Create: `custom_components/byonk/__init__.py` (empty-ish), `custom_components/byonk/const.py`, `custom_components/byonk/manifest.json`
- Create: `hacs.json`, `pyproject.toml`, `requirements_test.txt`
- Create: `tests_ha/conftest.py`, `tests_ha/test_manifest.py`
- Modify: `Makefile` (add `ha-check` target)

**Interfaces:**
- Produces: `custom_components.byonk.const.DOMAIN = "byonk"`, `BYONK_ADDON_REPO_URL`, `ADDON_CONFIG_SLUG = "byonk"`, `DEFAULT_PORT = 3000`, `UPDATE_INTERVAL_SECONDS = 60`, `PLATFORMS: list[Platform]`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_manifest.py`:
```python
import json
from pathlib import Path

MANIFEST = Path("custom_components/byonk/manifest.json")


def test_manifest_has_required_keys():
    data = json.loads(MANIFEST.read_text())
    assert data["domain"] == "byonk"
    assert data["integration_type"] == "hub"
    assert data["iot_class"] == "local_polling"
    assert data["config_flow"] is True
    assert data["after_dependencies"] == ["hassio"]
    assert "hassio" not in data.get("dependencies", [])
    assert data["codeowners"] == ["@oetiker"]
    assert "version" in data and data["version"]
    for key in ("documentation", "issue_tracker"):
        assert data[key].startswith("https://github.com/oetiker/byonk")


def test_hacs_json_parses():
    data = json.loads(Path("hacs.json").read_text())
    assert data["name"]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_manifest.py -v`
Expected: FAIL (FileNotFoundError / files missing).

- [ ] **Step 3: Create the files**

`custom_components/byonk/manifest.json`:
```json
{
  "domain": "byonk",
  "name": "Byonk",
  "version": "0.1.0",
  "documentation": "https://github.com/oetiker/byonk",
  "issue_tracker": "https://github.com/oetiker/byonk/issues",
  "codeowners": ["@oetiker"],
  "config_flow": true,
  "integration_type": "hub",
  "iot_class": "local_polling",
  "dependencies": [],
  "after_dependencies": ["hassio"],
  "requirements": []
}
```

`custom_components/byonk/const.py`:
```python
"""Constants for the Byonk integration."""
from __future__ import annotations

from homeassistant.const import Platform

DOMAIN = "byonk"

BYONK_ADDON_REPO_URL = "https://github.com/oetiker/byonk"
ADDON_CONFIG_SLUG = "byonk"  # the add-on's config.yaml slug; full slug is "<repo_hash>_byonk"
ADDON_NAME = "Byonk"
DEFAULT_PORT = 3000

UPDATE_INTERVAL_SECONDS = 60

CONF_ADDON_SLUG = "addon_slug"
CONF_BASE_URL = "base_url"

PLATFORMS: list[Platform] = [Platform.SENSOR, Platform.SELECT, Platform.SWITCH]

# Repairs
ISSUE_PENDING_PREFIX = "device_pending_"
```

`custom_components/byonk/__init__.py`:
```python
"""The Byonk integration."""
from __future__ import annotations
```

`hacs.json`:
```json
{
  "name": "Byonk",
  "homeassistant": "2025.4.0",
  "render_readme": false
}
```

`requirements_test.txt`:
```
pytest-homeassistant-custom-component==0.13.*
```

`pyproject.toml`:
```toml
[tool.pytest.ini_options]
testpaths = ["tests_ha"]
pythonpath = ["."]
asyncio_mode = "auto"

[tool.ruff]
target-version = "py312"
src = ["custom_components"]
```

`tests_ha/conftest.py`:
```python
"""Shared fixtures for Byonk integration tests."""
import pytest

pytest_plugins = ["pytest_homeassistant_custom_component"]


@pytest.fixture(autouse=True)
def auto_enable_custom_integrations(enable_custom_integrations):
    """Enable loading custom integrations in all tests."""
    yield
```

- [ ] **Step 4: Add Makefile target**

Append to `Makefile` (use TAB indentation, matching the file):
```makefile
ha-check:
	pip install -q -r requirements_test.txt
	ruff check custom_components/byonk
	python -m pytest tests_ha -q
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_manifest.py -v`
Expected: PASS (2 passed).

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk hacs.json pyproject.toml requirements_test.txt tests_ha Makefile
git commit -m "feat(ha): scaffold Byonk integration package + test harness

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Admin API client (`api.py`)

**Files:**
- Create: `custom_components/byonk/api.py`
- Test: `tests_ha/test_api.py`

**Interfaces:**
- Produces:
  - Errors: `ByonkApiError`, `ByonkConnectionError(ByonkApiError)`, `ByonkAuthError(ByonkApiError)`, `ByonkValidationError(ByonkApiError)` (has `.message: str`), `ByonkReadOnlyError(ByonkApiError)`.
  - `class ByonkClient(session: aiohttp.ClientSession, base_url: str, token: str)` with async methods returning parsed JSON:
    `async_get_devices() -> list[dict]`, `async_get_pending() -> list[dict]`,
    `async_get_screens() -> dict`, `async_get_config() -> dict`,
    `async_add_device(payload: dict) -> dict`, `async_update_device(key: str, payload: dict) -> dict`,
    `async_delete_device(key: str) -> dict`, `async_update_settings(payload: dict) -> dict`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_api.py`:
```python
import aiohttp
import pytest
from aioresponses import aioresponses

from custom_components.byonk.api import (
    ByonkAuthError,
    ByonkClient,
    ByonkReadOnlyError,
    ByonkValidationError,
)

BASE = "http://addon:3000"


@pytest.fixture
def mock_aioresponse():
    with aioresponses() as m:
        yield m


async def test_get_devices_sends_bearer(mock_aioresponse):
    mock_aioresponse.get(
        f"{BASE}/api/admin/devices",
        payload=[{"key": "AA:BB", "screen": "transit"}],
    )
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        result = await client.async_get_devices()
    assert result[0]["key"] == "AA:BB"
    req = next(iter(mock_aioresponse.requests.values()))[0]
    assert req.kwargs["headers"]["Authorization"] == "Bearer secret"


async def test_404_raises_auth_error(mock_aioresponse):
    mock_aioresponse.get(f"{BASE}/api/admin/devices", status=404)
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "")
        with pytest.raises(ByonkAuthError):
            await client.async_get_devices()


async def test_400_raises_validation_with_message(mock_aioresponse):
    mock_aioresponse.post(
        f"{BASE}/api/admin/devices", status=400, payload={"error": "unknown screen"}
    )
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        with pytest.raises(ByonkValidationError) as exc:
            await client.async_add_device({"key": "AA:BB", "screen": "nope"})
    assert "unknown screen" in exc.value.message


async def test_409_raises_readonly(mock_aioresponse):
    mock_aioresponse.patch(f"{BASE}/api/admin/settings", status=409, payload={})
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        with pytest.raises(ByonkReadOnlyError):
            await client.async_update_settings({"registration_enabled": True})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_api.py -v`
Expected: FAIL (ModuleNotFoundError: custom_components.byonk.api).

- [ ] **Step 3: Implement `api.py`**

```python
"""Async client for the byonk admin API (Phase 1)."""
from __future__ import annotations

from typing import Any

import aiohttp


class ByonkApiError(Exception):
    """Base error for admin API calls."""


class ByonkConnectionError(ByonkApiError):
    """Network/transport failure."""


class ByonkAuthError(ByonkApiError):
    """Admin API dormant (404) or wrong token (401)."""


class ByonkValidationError(ByonkApiError):
    """byonk rejected a write (400)."""

    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message = message


class ByonkReadOnlyError(ByonkApiError):
    """Config is embedded/read-only (409)."""


class ByonkClient:
    """Thin wrapper over /api/admin/* using a shared aiohttp session."""

    def __init__(
        self, session: aiohttp.ClientSession, base_url: str, token: str
    ) -> None:
        self._session = session
        self._base = base_url.rstrip("/")
        self._token = token

    async def _request(
        self, method: str, path: str, json: dict | None = None
    ) -> Any:
        url = f"{self._base}{path}"
        headers = {"Authorization": f"Bearer {self._token}"}
        try:
            async with self._session.request(
                method, url, json=json, headers=headers
            ) as resp:
                if resp.status in (401, 404):
                    raise ByonkAuthError(f"{method} {path} -> {resp.status}")
                if resp.status == 409:
                    raise ByonkReadOnlyError(f"{method} {path} -> 409")
                if resp.status == 400:
                    body = await _safe_json(resp)
                    raise ByonkValidationError(
                        body.get("error") or body.get("message") or "validation error"
                    )
                resp.raise_for_status()
                if resp.status == 204:
                    return None
                return await resp.json()
        except aiohttp.ClientError as err:
            raise ByonkConnectionError(str(err)) from err

    async def async_get_devices(self) -> list[dict]:
        return await self._request("GET", "/api/admin/devices")

    async def async_get_pending(self) -> list[dict]:
        return await self._request("GET", "/api/admin/pending")

    async def async_get_screens(self) -> dict:
        return await self._request("GET", "/api/admin/screens")

    async def async_get_config(self) -> dict:
        return await self._request("GET", "/api/admin/config")

    async def async_add_device(self, payload: dict) -> dict:
        return await self._request("POST", "/api/admin/devices", json=payload)

    async def async_update_device(self, key: str, payload: dict) -> dict:
        return await self._request("PATCH", f"/api/admin/devices/{key}", json=payload)

    async def async_delete_device(self, key: str) -> dict:
        return await self._request("DELETE", f"/api/admin/devices/{key}")

    async def async_update_settings(self, payload: dict) -> dict:
        return await self._request("PATCH", "/api/admin/settings", json=payload)


async def _safe_json(resp: aiohttp.ClientResponse) -> dict:
    try:
        return await resp.json()
    except (aiohttp.ContentTypeError, ValueError):
        return {}
```

- [ ] **Step 4: Add `aioresponses` to test deps**

Append to `requirements_test.txt`:
```
aioresponses
```
Run: `pip install -q -r requirements_test.txt`

- [ ] **Step 5: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_api.py -v`
Expected: PASS (4 passed).

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/api.py tests_ha/test_api.py requirements_test.txt
git commit -m "feat(ha): admin API client with typed errors

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Add-on lifecycle + token provisioning (`addon.py`)

**Files:**
- Create: `custom_components/byonk/addon.py`
- Test: `tests_ha/test_addon.py`

**Interfaces:**
- Produces (all `async`, take `hass`):
  - `async_find_addon_slug(hass) -> str | None` — match installed/store add-on by config-slug `byonk`.
  - `async_ensure_addon_installed(hass) -> str` — add repo if needed, install + start; returns slug. Raises `AddonError` on failure.
  - `async_provision_token(hass, slug) -> str` — generate token, merge into options, restart; returns token.
  - `async_read_token(hass, slug) -> str | None` — read `admin_token` from add-on options.
  - `async_get_base_url(hass, slug) -> str` — `http://{hostname}:3000`.
- Consumes: `homeassistant.components.hassio.get_supervisor_client`, `AddonManager`, `aiohasupervisor` models.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_addon.py`:
```python
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from custom_components.byonk import addon


@pytest.fixture
def supervisor(hass):
    """Patch get_supervisor_client with a fake store/addons surface."""
    client = MagicMock()
    client.store.addons_list = AsyncMock(return_value=[])
    client.store.add_repository = AsyncMock()
    client.store.install_addon = AsyncMock()
    client.addons.start_addon = AsyncMock()
    with patch.object(addon, "get_supervisor_client", return_value=client):
        yield client


async def test_find_slug_matches_byonk_config_slug(hass, supervisor):
    item = MagicMock(slug="abcd1234_byonk", name="Byonk",
                     repository="abcd1234", installed=True)
    supervisor.store.addons_list.return_value = [item]
    assert await addon.async_find_addon_slug(hass) == "abcd1234_byonk"


async def test_ensure_adds_repo_when_missing(hass, supervisor):
    # First list empty -> add repo -> second list returns the addon
    item = MagicMock(slug="abcd1234_byonk", name="Byonk",
                     repository="abcd1234", installed=False)
    supervisor.store.addons_list.side_effect = [[], [item]]
    with patch.object(addon, "_async_start", new=AsyncMock()):
        slug = await addon.async_ensure_addon_installed(hass)
    assert slug == "abcd1234_byonk"
    supervisor.store.add_repository.assert_awaited_once()
    supervisor.store.install_addon.assert_awaited_once_with("abcd1234_byonk")


async def test_provision_sets_options_and_restarts(hass):
    mgr = MagicMock()
    mgr.async_get_addon_info = AsyncMock(
        return_value=MagicMock(options={"log_level": "info"})
    )
    mgr.async_set_addon_options = AsyncMock()
    mgr.async_restart_addon = AsyncMock()
    with patch.object(addon, "_get_addon_manager", return_value=mgr):
        token = await addon.async_provision_token(hass, "abcd1234_byonk")
    assert token  # non-empty, generated
    sent = mgr.async_set_addon_options.await_args.args[0]
    assert sent["admin_token"] == token
    assert sent["log_level"] == "info"  # preserves existing options
    mgr.async_restart_addon.assert_awaited_once()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_addon.py -v`
Expected: FAIL (ModuleNotFoundError: custom_components.byonk.addon).

- [ ] **Step 3: Implement `addon.py`**

```python
"""Supervisor add-on lifecycle + token provisioning for byonk."""
from __future__ import annotations

import logging
import secrets

from homeassistant.components.hassio import (
    AddonError,
    AddonManager,
    AddonState,
    get_supervisor_client,
)
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers.singleton import singleton

from .const import ADDON_CONFIG_SLUG, ADDON_NAME, BYONK_ADDON_REPO_URL, DEFAULT_PORT

_LOGGER = logging.getLogger(__name__)
DATA_ADDON_MANAGER = "byonk_addon_manager"


@singleton(DATA_ADDON_MANAGER)
@callback
def _get_addon_manager(hass: HomeAssistant, slug: str) -> AddonManager:
    return AddonManager(hass, _LOGGER, ADDON_NAME, slug)


async def async_find_addon_slug(hass: HomeAssistant) -> str | None:
    """Return the installable slug of the byonk add-on, or None."""
    client = get_supervisor_client(hass)
    for item in await client.store.addons_list():
        if item.slug.endswith(f"_{ADDON_CONFIG_SLUG}") or item.slug == ADDON_CONFIG_SLUG:
            return item.slug
    return None


async def async_ensure_addon_installed(hass: HomeAssistant) -> str:
    """Add the repo (if needed), install + start the add-on; return its slug."""
    client = get_supervisor_client(hass)
    slug = await async_find_addon_slug(hass)
    if slug is None:
        try:
            from aiohasupervisor.models import StoreAddRepository

            await client.store.add_repository(
                StoreAddRepository(repository=BYONK_ADDON_REPO_URL)
            )
        except Exception as err:  # SupervisorError subclasses
            raise AddonError(f"Could not add byonk add-on repository: {err}") from err
        slug = await async_find_addon_slug(hass)
        if slug is None:
            raise AddonError("byonk add-on not found after adding repository")

    # Install if needed.
    items = {i.slug: i for i in await client.store.addons_list()}
    if not getattr(items.get(slug), "installed", False):
        await client.store.install_addon(slug)
    await _async_start(hass, slug)
    return slug


async def _async_start(hass: HomeAssistant, slug: str) -> None:
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    if info.state != AddonState.RUNNING:
        await mgr.async_start_addon()


async def async_provision_token(hass: HomeAssistant, slug: str) -> str:
    """Generate a token, merge into add-on options, restart; return the token."""
    mgr = _get_addon_manager(hass, slug)
    token = secrets.token_hex(32)
    info = await mgr.async_get_addon_info()
    options = dict(info.options or {})
    options["admin_token"] = token
    await mgr.async_set_addon_options(options)
    await mgr.async_restart_addon()
    return token


async def async_read_token(hass: HomeAssistant, slug: str) -> str | None:
    """Read the admin token back from the add-on option (single source of truth)."""
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    token = (info.options or {}).get("admin_token")
    return token or None


async def async_get_base_url(hass: HomeAssistant, slug: str) -> str:
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    return f"http://{info.hostname}:{DEFAULT_PORT}"
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_addon.py -v`
Expected: PASS (3 passed). If `aiohasupervisor.models.StoreAddRepository` import path differs in the installed HA version, adjust the import and re-run.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/addon.py tests_ha/test_addon.py
git commit -m "feat(ha): add-on install + zero-touch token provisioning

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Config flow — zero-touch trust (`config_flow.py`)

**Files:**
- Create: `custom_components/byonk/config_flow.py`
- Modify: `custom_components/byonk/strings.json` (create), `custom_components/byonk/translations/en.json` (create)
- Test: `tests_ha/test_config_flow.py`

**Interfaces:**
- Consumes: `addon.async_ensure_addon_installed`, `async_provision_token`, `async_read_token`, `async_get_base_url`; `const.CONF_ADDON_SLUG`, `CONF_BASE_URL`.
- Produces: `class ByonkConfigFlow(ConfigFlow, domain=DOMAIN)` creating an entry with `data={CONF_ADDON_SLUG: slug, CONF_BASE_URL: url}` (NO token). Abort reasons: `not_hassio`, `single_instance_allowed`, `addon_error`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_config_flow.py`:
```python
from unittest.mock import AsyncMock, patch

from homeassistant import config_entries
from homeassistant.data_entry_flow import FlowResultType

from custom_components.byonk.const import CONF_BASE_URL, DOMAIN


async def _start(hass):
    return await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )


async def test_aborts_without_supervisor(hass):
    with patch("custom_components.byonk.config_flow.is_hassio", return_value=False):
        result = await _start(hass)
    assert result["type"] == FlowResultType.ABORT
    assert result["reason"] == "not_hassio"


async def test_happy_path_creates_entry_without_token(hass):
    with (
        patch("custom_components.byonk.config_flow.is_hassio", return_value=True),
        patch(
            "custom_components.byonk.config_flow.async_ensure_addon_installed",
            new=AsyncMock(return_value="abcd1234_byonk"),
        ),
        patch(
            "custom_components.byonk.config_flow.async_provision_token",
            new=AsyncMock(return_value="tok"),
        ),
        patch(
            "custom_components.byonk.config_flow.async_get_base_url",
            new=AsyncMock(return_value="http://addon:3000"),
        ),
        patch(
            "custom_components.byonk.config_flow.ByonkClient.async_get_config",
            new=AsyncMock(return_value={}),
        ),
    ):
        result = await _start(hass)
    assert result["type"] == FlowResultType.CREATE_ENTRY
    assert result["data"] == {"addon_slug": "abcd1234_byonk", CONF_BASE_URL: "http://addon:3000"}
    assert "admin_token" not in result["data"]
    assert "tok" not in str(result["data"])


async def test_addon_failure_aborts_gracefully(hass):
    from homeassistant.components.hassio import AddonError

    with (
        patch("custom_components.byonk.config_flow.is_hassio", return_value=True),
        patch(
            "custom_components.byonk.config_flow.async_ensure_addon_installed",
            new=AsyncMock(side_effect=AddonError("clone failed")),
        ),
    ):
        result = await _start(hass)
    assert result["type"] == FlowResultType.ABORT
    assert result["reason"] == "addon_error"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_config_flow.py -v`
Expected: FAIL (no config_flow module).

- [ ] **Step 3: Implement `config_flow.py` (config-entry flow only for now)**

```python
"""Config flow for the Byonk integration."""
from __future__ import annotations

from typing import Any

from homeassistant.components.hassio import AddonError, is_hassio
from homeassistant.config_entries import ConfigFlow, ConfigFlowResult
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .addon import (
    async_ensure_addon_installed,
    async_get_base_url,
    async_provision_token,
    async_read_token,
)
from .api import ByonkClient
from .const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN


class ByonkConfigFlow(ConfigFlow, domain=DOMAIN):
    """Zero-touch, Supervised-only setup."""

    VERSION = 1

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if self._async_current_entries():
            return self.async_abort(reason="single_instance_allowed")
        if not is_hassio(self.hass):
            return self.async_abort(reason="not_hassio")

        try:
            slug = await async_ensure_addon_installed(self.hass)
            token = await async_read_token(self.hass, slug)
            if not token:
                token = await async_provision_token(self.hass, slug)
            base_url = await async_get_base_url(self.hass, slug)
            client = ByonkClient(
                async_get_clientsession(self.hass), base_url, token
            )
            await client.async_get_config()  # auth probe
        except AddonError:
            return self.async_abort(reason="addon_error")

        await self.async_set_unique_id(DOMAIN)
        return self.async_create_entry(
            title="Byonk",
            data={CONF_ADDON_SLUG: slug, CONF_BASE_URL: base_url},
        )
```

`custom_components/byonk/strings.json`:
```json
{
  "config": {
    "abort": {
      "not_hassio": "Byonk requires the Byonk add-on, which needs a Home Assistant Supervised or HAOS installation.",
      "single_instance_allowed": "Byonk is already configured.",
      "addon_error": "Could not install or start the Byonk add-on automatically. Add the repository https://github.com/oetiker/byonk to the add-on store, install the Byonk add-on, then retry."
    }
  }
}
```

Create `custom_components/byonk/translations/en.json` with identical content to `strings.json`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_config_flow.py -v`
Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations tests_ha/test_config_flow.py
git commit -m "feat(ha): zero-touch config flow (Supervised-only)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Coordinator + entry setup + hub device (`coordinator.py`, `__init__.py`, `entity.py`)

**Files:**
- Create: `custom_components/byonk/coordinator.py`, `custom_components/byonk/entity.py`
- Modify: `custom_components/byonk/__init__.py`
- Test: `tests_ha/test_init.py`

**Interfaces:**
- Produces:
  - `@dataclass(frozen=True) class ByonkData` with fields `devices: list[dict]`, `pending: list[dict]`, `screens: list[dict]`, `panels: list[dict]`, `dither: list[str]`, `config: dict`; helper `screen_names() -> list[str]`, `default_screen() -> str | None`, `registration_enabled() -> bool`, `auth_mode() -> str | None`.
  - `type ByonkConfigEntry = ConfigEntry[ByonkCoordinator]`.
  - `class ByonkCoordinator(DataUpdateCoordinator[ByonkData])` with `client: ByonkClient`, `entry: ByonkConfigEntry`, `slug: str`; `async_setup() -> None`.
  - `entity.ByonkHubEntity(CoordinatorEntity[ByonkCoordinator])` with hub `DeviceInfo`.
  - `__init__.async_setup_entry/async_unload_entry`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_init.py`:
```python
from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

SCREENS = {
    "screens": [{"name": "transit", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og", "width": 800, "height": 480, "colors": "bw"}],
    "dither_algorithms": ["atkinson"],
}
CONFIG = {"registration": {"enabled": True}, "default_screen": "transit", "auth_mode": "api_key"}


def _entry():
    return MockConfigEntry(
        domain=DOMAIN,
        unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"},
    )


async def test_setup_entry_creates_hub_and_loads(hass):
    entry = _entry()
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value=CONFIG)),
    ):
        assert await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    assert entry.state is ConfigEntryState.LOADED
    assert entry.runtime_data.data.default_screen() == "transit"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_init.py -v`
Expected: FAIL (no coordinator / async_setup_entry).

- [ ] **Step 3: Implement `coordinator.py`**

```python
"""Data coordinator for byonk."""
from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import timedelta
import logging

from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import DOMAIN, UPDATE_INTERVAL_SECONDS

_LOGGER = logging.getLogger(__name__)

type ByonkConfigEntry = ConfigEntry["ByonkCoordinator"]


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

    def default_screen(self) -> str | None:
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
        return ByonkData(
            devices=devices,
            pending=pending,
            screens=screens.get("screens", []),
            panels=screens.get("panels", []),
            dither=screens.get("dither_algorithms", []),
            config=config,
        )
```

- [ ] **Step 4: Implement `entity.py`**

```python
"""Base entities for byonk."""
from __future__ import annotations

from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import ADDON_NAME, DOMAIN
from .coordinator import ByonkCoordinator


class ByonkHubEntity(CoordinatorEntity[ByonkCoordinator]):
    """Entity attached to the Byonk Server hub device."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: ByonkCoordinator) -> None:
        super().__init__(coordinator)
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, coordinator.entry.entry_id)},
            name=ADDON_NAME,
            manufacturer="Byonk",
            configuration_url=coordinator.client._base,  # noqa: SLF001
        )
```

- [ ] **Step 5: Implement `__init__.py`**

```python
"""The Byonk integration."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .addon import async_read_token
from .api import ByonkClient
from .const import CONF_ADDON_SLUG, CONF_BASE_URL, PLATFORMS
from .coordinator import ByonkConfigEntry, ByonkCoordinator


async def async_setup_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    slug = entry.data[CONF_ADDON_SLUG]
    token = await async_read_token(hass, slug)
    if not token:
        raise ConfigEntryAuthFailed("byonk admin token not provisioned")
    client = ByonkClient(
        async_get_clientsession(hass), entry.data[CONF_BASE_URL], token
    )
    coordinator = ByonkCoordinator(hass, entry, client, slug)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    return await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
```

Create empty platform modules so the forward setup doesn't fail (filled in later tasks):
`custom_components/byonk/sensor.py`, `select.py`, `switch.py`, each:
```python
"""Placeholder; entities added in later tasks."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .coordinator import ByonkConfigEntry


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    return
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_init.py -v`
Expected: PASS (1 passed).

- [ ] **Step 7: Commit**

```bash
git add custom_components/byonk
git commit -m "feat(ha): coordinator, entry setup, hub device

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Stage 2 — Mirror + telemetry

### Task 6: Reconciliation (subentry-per-device) in coordinator

**Files:**
- Modify: `custom_components/byonk/coordinator.py`
- Test: `tests_ha/test_reconcile.py`

**Interfaces:**
- Produces: `ByonkCoordinator._async_reconcile(data: ByonkData) -> None` — called at the end of `_async_update_data`. For each registered device (`registered is True`) with no subentry → `hass.config_entries.async_add_subentry(entry, ConfigSubentry(data={"key": key}, subentry_type="device", title=key, unique_id=key))`; for each existing `"device"` subentry whose key is gone → `async_remove_subentry`.
- Consumes: `ByonkData.devices` (each dict has `key`, `registered`).

- [ ] **Step 1: Write the failing test**

`tests_ha/test_reconcile.py`:
```python
from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

DEV = {"key": "AA:BB", "registered": True, "model": "og",
       "battery_voltage": 4.1, "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00",
       "firmware_version": "1.7.1", "screen": "transit", "dither": "atkinson", "panel": None}
SCREENS = {"screens": [{"name": "transit", "params": [], "schema_error": None}],
           "panels": [], "dither_algorithms": ["atkinson"]}


async def _setup(hass, devices):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=devices)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    return entry


async def test_registered_device_gets_subentry(hass):
    entry = await _setup(hass, [DEV])
    types = [s.subentry_type for s in entry.subentries.values()]
    keys = [s.unique_id for s in entry.subentries.values()]
    assert "device" in types
    assert "AA:BB" in keys
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_reconcile.py -v`
Expected: FAIL (no subentry created).

- [ ] **Step 3: Implement reconciliation**

In `coordinator.py`, add imports and call. At top:
```python
from homeassistant.config_entries import ConfigSubentry
```
Append to `_async_update_data` just before `return`:
```python
        data = ByonkData(...)  # existing construction
        self._async_reconcile(data)
        return data
```
Add method:
```python
    def _async_reconcile(self, data: ByonkData) -> None:
        existing = {
            sub.unique_id: sub_id
            for sub_id, sub in self.entry.subentries.items()
            if sub.subentry_type == "device"
        }
        registered_keys = {
            d["key"] for d in data.devices if d.get("registered")
        }
        for key in registered_keys - set(existing):
            self.hass.config_entries.async_add_subentry(
                self.entry,
                ConfigSubentry(
                    data={"key": key},
                    subentry_type="device",
                    title=key,
                    unique_id=key,
                ),
            )
        for key in set(existing) - registered_keys:
            self.hass.config_entries.async_remove_subentry(
                self.entry, existing[key]
            )
```

> Note: verify `async_add_subentry`/`async_remove_subentry` and the `ConfigSubentry` field names against the installed HA version (renamed in 2025.4). Adjust if the signature differs.

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_reconcile.py -v`
Expected: PASS (1 passed).

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/coordinator.py tests_ha/test_reconcile.py
git commit -m "feat(ha): reconcile byonk devices into HA subentries

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Per-device telemetry sensors (`sensor.py`)

**Files:**
- Modify: `custom_components/byonk/sensor.py`, `custom_components/byonk/entity.py`
- Test: `tests_ha/test_sensor.py`

**Interfaces:**
- Produces: `entity.ByonkDeviceEntity(CoordinatorEntity)` with per-device `DeviceInfo` (`identifiers={(DOMAIN, key)}`, `via_device=(DOMAIN, entry_id)`), helper `self.device -> dict | None` (looks up its key in `coordinator.data.devices`). Five `SensorEntity` classes wired in `sensor.async_setup_entry`, added per `"device"` subentry with `config_subentry_id`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_sensor.py` (reuse `_setup` pattern from Task 6; put a shared helper in `conftest.py` if preferred):
```python
from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

DEV = {"key": "AA:BB", "registered": True, "model": "og", "battery_voltage": 4.1,
       "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00", "firmware_version": "1.7.1",
       "screen": "transit", "dither": "atkinson", "panel": None}
SCREENS = {"screens": [{"name": "transit", "params": [], "schema_error": None}],
           "panels": [], "dither_algorithms": ["atkinson"]}


async def test_battery_sensor_state(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    state = hass.states.get("sensor.aa_bb_battery_voltage") or _find(hass, "battery")
    assert state is not None
    assert float(state.state) == 4.1


def _find(hass, needle):
    for s in hass.states.async_all("sensor"):
        if needle in s.entity_id:
            return s
    return None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_sensor.py -v`
Expected: FAIL (no sensor entities).

- [ ] **Step 3: Add `ByonkDeviceEntity` to `entity.py`**

```python
class ByonkDeviceEntity(CoordinatorEntity[ByonkCoordinator]):
    """Entity attached to one TRMNL device."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator)
        self._key = key
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, key)},
            name=f"TRMNL {key}",
            manufacturer="TRMNL",
            via_device=(DOMAIN, coordinator.entry.entry_id),
        )

    @property
    def device(self) -> dict | None:
        for d in self.coordinator.data.devices:
            if d.get("key") == self._key:
                return d
        return None

    @property
    def available(self) -> bool:
        return super().available and self.device is not None
```

- [ ] **Step 4: Implement `sensor.py`**

```python
"""Byonk sensors."""
from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from homeassistant.components.sensor import (
    SensorDeviceClass,
    SensorEntity,
    SensorEntityDescription,
)
from homeassistant.const import EntityCategory, UnitOfElectricPotential
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.util import dt as dt_util

from .const import DOMAIN
from .coordinator import ByonkConfigEntry
from .entity import ByonkDeviceEntity


@dataclass(frozen=True, kw_only=True)
class ByonkSensorDesc(SensorEntityDescription):
    value: Callable[[dict], object]


DEVICE_SENSORS: tuple[ByonkSensorDesc, ...] = (
    ByonkSensorDesc(
        key="battery_voltage",
        translation_key="battery_voltage",
        device_class=SensorDeviceClass.VOLTAGE,
        native_unit_of_measurement=UnitOfElectricPotential.VOLT,
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("battery_voltage"),
    ),
    ByonkSensorDesc(
        key="rssi",
        translation_key="rssi",
        device_class=SensorDeviceClass.SIGNAL_STRENGTH,
        native_unit_of_measurement="dBm",
        entity_category=EntityCategory.DIAGNOSTIC,
        entity_registry_enabled_default=False,
        value=lambda d: d.get("rssi"),
    ),
    ByonkSensorDesc(
        key="last_seen",
        translation_key="last_seen",
        device_class=SensorDeviceClass.TIMESTAMP,
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: dt_util.parse_datetime(d["last_seen"]) if d.get("last_seen") else None,
    ),
    ByonkSensorDesc(
        key="firmware_version",
        translation_key="firmware_version",
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("firmware_version"),
    ),
    ByonkSensorDesc(
        key="model",
        translation_key="model",
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("model"),
    ),
)


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    for sub_id, sub in entry.subentries.items():
        if sub.subentry_type != "device":
            continue
        key = sub.data["key"]
        async_add_entities(
            (ByonkDeviceSensor(coordinator, key, desc) for desc in DEVICE_SENSORS),
            config_subentry_id=sub_id,
        )


class ByonkDeviceSensor(ByonkDeviceEntity, SensorEntity):
    entity_description: ByonkSensorDesc

    def __init__(self, coordinator, key, description: ByonkSensorDesc) -> None:
        super().__init__(coordinator, key)
        self.entity_description = description
        self._attr_unique_id = f"{key}_{description.key}"

    @property
    def native_value(self):
        device = self.device
        return self.entity_description.value(device) if device else None
```

> Add the matching `entity.sensor.*` `translation_key` strings to `strings.json`/`translations/en.json` under `"entity": {"sensor": {...}}`.

> **Subentry-at-setup-time caveat:** entities are added for subentries that already exist when the platform sets up. Newly reconciled subentries appear on the next reload or when the platform's subentry-add listener fires. For Phase 3 simplicity, the platform reads current subentries at setup; a reload picks up new devices. (A subentry-add dispatcher can be added as a fast-follow.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_sensor.py -v`
Expected: PASS. (If the entity_id differs, the `_find` fallback locates it.)

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/sensor.py custom_components/byonk/entity.py custom_components/byonk/strings.json custom_components/byonk/translations tests_ha/test_sensor.py
git commit -m "feat(ha): per-device telemetry sensors

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Hub `pending_devices` sensor

**Files:**
- Modify: `custom_components/byonk/sensor.py`
- Test: `tests_ha/test_pending_sensor.py`

**Interfaces:**
- Produces: `ByonkPendingSensor(ByonkHubEntity, SensorEntity)` — `native_value` = `len(pending)`, `extra_state_attributes` = `{"devices": [{registration_code, model, last_seen}, ...]}`. Added once (not per subentry) in `sensor.async_setup_entry`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_pending_sensor.py`:
```python
from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og",
            "last_seen": "2026-06-29T09:00:00+00:00"}]
SCREENS = {"screens": [], "panels": [], "dither_algorithms": []}


async def test_pending_sensor_counts_and_lists(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=PENDING)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    state = next(s for s in hass.states.async_all("sensor") if "pending" in s.entity_id)
    assert state.state == "1"
    assert state.attributes["devices"][0]["registration_code"] == "ABCD-1234"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_pending_sensor.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `sensor.py` `async_setup_entry`, before the subentry loop:
```python
    async_add_entities([ByonkPendingSensor(coordinator)])
```
Add class:
```python
class ByonkPendingSensor(ByonkHubEntity, SensorEntity):
    _attr_translation_key = "pending_devices"
    _attr_entity_category = EntityCategory.DIAGNOSTIC

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_pending_devices"

    @property
    def native_value(self) -> int:
        return len(self.coordinator.data.pending)

    @property
    def extra_state_attributes(self) -> dict:
        return {
            "devices": [
                {
                    "registration_code": p.get("registration_code"),
                    "model": p.get("model"),
                    "last_seen": p.get("last_seen"),
                }
                for p in self.coordinator.data.pending
            ]
        }
```
Import `ByonkHubEntity` in `sensor.py`: `from .entity import ByonkDeviceEntity, ByonkHubEntity`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_pending_sensor.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/sensor.py tests_ha/test_pending_sensor.py
git commit -m "feat(ha): hub pending-devices sensor

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Stage 3 — Live controls

### Task 9: Per-device control selects (`select.py`)

**Files:**
- Modify: `custom_components/byonk/select.py`
- Test: `tests_ha/test_select.py`

**Interfaces:**
- Produces: three `SelectEntity` per device — `screen`, `dither`, `panel`. Options from `coordinator.data`. `async_select_option` writes via the admin API then `async_request_refresh()`. **`screen` write** sends `{"screen": opt, "params": default_params(screen_params)}` (valid-by-construction).
- Consumes: `param_form.default_params(param_fields) -> dict` (Task 11 — define a minimal version here and let Task 11 own the full one). To avoid a forward dependency, implement `default_params` in `param_form.py` as part of this task's prerequisites (create the file now with just `default_params`).

- [ ] **Step 1: Create `param_form.default_params` (prereq) + write the failing test**

Create `custom_components/byonk/param_form.py`:
```python
"""Build HA forms from byonk @params schemas."""
from __future__ import annotations


def default_params(param_fields: list[dict]) -> dict:
    """Return {name: default} for fields that declare a default."""
    return {
        f["name"]: f["default"]
        for f in param_fields
        if "default" in f and f["default"] is not None
    }
```

`tests_ha/test_select.py`:
```python
from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

DEV = {"key": "AA:BB", "registered": True, "screen": "transit", "dither": "atkinson", "panel": None}
SCREENS = {"screens": [
        {"name": "transit", "params": [{"name": "limit", "type": "int", "default": 8}], "schema_error": None},
        {"name": "weather", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson", "sierra"]}


async def _setup(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    return entry


async def test_screen_select_resets_params_to_defaults(hass):
    entry = await _setup(hass)
    update = AsyncMock(return_value={})
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
        patch("custom_components.byonk.coordinator.ByonkClient.async_update_device", new=update),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
        ent = next(s for s in hass.states.async_all("select") if "screen" in s.entity_id)
        await hass.services.async_call(
            "select", "select_option",
            {"entity_id": ent.entity_id, "option": "weather"}, blocking=True,
        )
    key, payload = update.await_args.args
    assert key == "AA:BB"
    assert payload["screen"] == "weather"
    assert payload["params"] == {}  # weather has no defaults
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_select.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement `select.py`**

```python
"""Byonk select entities."""
from __future__ import annotations

from homeassistant.components.select import SelectEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity
from .param_form import default_params


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    for sub_id, sub in entry.subentries.items():
        if sub.subentry_type != "device":
            continue
        key = sub.data["key"]
        async_add_entities(
            [
                ByonkScreenSelect(coordinator, key),
                ByonkDitherSelect(coordinator, key),
                ByonkPanelSelect(coordinator, key),
            ],
            config_subentry_id=sub_id,
        )


class _ByonkSelect(ByonkDeviceEntity, SelectEntity):
    _field: str

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator, key)
        self._attr_unique_id = f"{key}_{self._field}"
        self._attr_translation_key = self._field

    @property
    def current_option(self) -> str | None:
        device = self.device
        return device.get(self._field) if device else None

    async def _write(self, payload: dict) -> None:
        await self.coordinator.client.async_update_device(self._key, payload)
        await self.coordinator.async_request_refresh()


class ByonkScreenSelect(_ByonkSelect):
    _field = "screen"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.screen_names()

    async def async_select_option(self, option: str) -> None:
        params = default_params(self.coordinator.data.screen_params(option))
        await self._write({"screen": option, "params": params})


class ByonkDitherSelect(_ByonkSelect):
    _field = "dither"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.dither

    async def async_select_option(self, option: str) -> None:
        await self._write({"dither": option})


class ByonkPanelSelect(_ByonkSelect):
    _field = "panel"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.panel_names()

    async def async_select_option(self, option: str) -> None:
        await self._write({"panel": option})
```

Add `entity.select.{screen,dither,panel}` translation strings.

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_select.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/select.py custom_components/byonk/param_form.py custom_components/byonk/strings.json custom_components/byonk/translations tests_ha/test_select.py
git commit -m "feat(ha): per-device screen/dither/panel selects

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Hub global-settings entities (`switch.py` + hub selects)

**Files:**
- Modify: `custom_components/byonk/switch.py`, `custom_components/byonk/select.py`
- Test: `tests_ha/test_settings_entities.py`

**Interfaces:**
- Produces: `ByonkRegistrationSwitch(ByonkHubEntity, SwitchEntity)` → `PATCH /settings {registration_enabled}`; `ByonkDefaultScreenSelect`, `ByonkAuthModeSelect` (ByonkHubEntity, SelectEntity) → `PATCH /settings {default_screen|auth_mode}`. Added once each on the hub.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_settings_entities.py`:
```python
from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

SCREENS = {"screens": [{"name": "transit", "params": [], "schema_error": None}],
           "panels": [], "dither_algorithms": []}
CONFIG = {"registration": {"enabled": False}, "default_screen": "transit", "auth_mode": "api_key"}


async def test_registration_switch_turns_on(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    settings = AsyncMock(return_value={})
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value=CONFIG)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_update_settings", new=settings),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
        ent = next(s for s in hass.states.async_all("switch") if "registration" in s.entity_id)
        assert ent.state == "off"
        await hass.services.async_call(
            "switch", "turn_on", {"entity_id": ent.entity_id}, blocking=True
        )
    assert settings.await_args.args[0] == {"registration_enabled": True}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_settings_entities.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement `switch.py`**

```python
"""Byonk switch entities (global settings)."""
from __future__ import annotations

from typing import Any

from homeassistant.components.switch import SwitchEntity
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .coordinator import ByonkConfigEntry
from .entity import ByonkHubEntity


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    async_add_entities([ByonkRegistrationSwitch(entry.runtime_data)])


class ByonkRegistrationSwitch(ByonkHubEntity, SwitchEntity):
    _attr_translation_key = "registration_enabled"
    _attr_entity_category = EntityCategory.CONFIG

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_registration_enabled"

    @property
    def is_on(self) -> bool:
        return self.coordinator.data.registration_enabled()

    async def async_turn_on(self, **kwargs: Any) -> None:
        await self.coordinator.client.async_update_settings({"registration_enabled": True})
        await self.coordinator.async_request_refresh()

    async def async_turn_off(self, **kwargs: Any) -> None:
        await self.coordinator.client.async_update_settings({"registration_enabled": False})
        await self.coordinator.async_request_refresh()
```

- [ ] **Step 4: Add hub selects to `select.py`**

In `select.py` `async_setup_entry`, before the subentry loop:
```python
    async_add_entities(
        [ByonkDefaultScreenSelect(coordinator), ByonkAuthModeSelect(coordinator)]
    )
```
Add classes (import `ByonkHubEntity`, `EntityCategory`):
```python
from homeassistant.const import EntityCategory
from .entity import ByonkDeviceEntity, ByonkHubEntity


class ByonkDefaultScreenSelect(ByonkHubEntity, SelectEntity):
    _attr_translation_key = "default_screen"
    _attr_entity_category = EntityCategory.CONFIG

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_default_screen"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.screen_names()

    @property
    def current_option(self) -> str | None:
        return self.coordinator.data.default_screen()

    async def async_select_option(self, option: str) -> None:
        await self.coordinator.client.async_update_settings({"default_screen": option})
        await self.coordinator.async_request_refresh()


class ByonkAuthModeSelect(ByonkHubEntity, SelectEntity):
    _attr_translation_key = "auth_mode"
    _attr_entity_category = EntityCategory.CONFIG
    _attr_options = ["api_key", "ed25519"]

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_auth_mode"

    @property
    def current_option(self) -> str | None:
        return self.coordinator.data.auth_mode()

    async def async_select_option(self, option: str) -> None:
        await self.coordinator.client.async_update_settings({"auth_mode": option})
        await self.coordinator.async_request_refresh()
```

Add `entity.switch.registration_enabled`, `entity.select.default_screen`, `entity.select.auth_mode` strings.

- [ ] **Step 5: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_settings_entities.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/switch.py custom_components/byonk/select.py custom_components/byonk/strings.json custom_components/byonk/translations tests_ha/test_settings_entities.py
git commit -m "feat(ha): hub global-settings switch + selects

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Stage 4 — Onboarding (subentry form + Repairs)

### Task 11: Dynamic `@params` form builder (`param_form.py`)

**Files:**
- Modify: `custom_components/byonk/param_form.py`
- Test: `tests_ha/test_param_form.py`

**Interfaces:**
- Produces: `build_params_schema(param_fields: list[dict], current: dict | None = None) -> vol.Schema` — maps each field to a selector per the Global Constraints table; `required` → `vol.Required`, else `vol.Optional`; on edit, prefills via `suggested_value` from `current`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_param_form.py`:
```python
import voluptuous as vol
from homeassistant.helpers import selector

from custom_components.byonk.param_form import build_params_schema

FIELDS = [
    {"name": "station", "type": "string", "required": True, "label": "Stop"},
    {"name": "limit", "type": "int", "default": 8, "min": 1, "max": 30},
    {"name": "theme", "type": "enum", "options": ["light", "dark"]},
    {"name": "enabled", "type": "bool", "default": True},
]


def test_builds_selectors_per_type():
    schema = build_params_schema(FIELDS)
    markers = {str(m): m for m in schema.schema}
    assert "station" in markers
    # required field uses vol.Required
    assert any(isinstance(m, vol.Required) and m.schema == "station" for m in schema.schema)
    sel = schema.schema[next(m for m in schema.schema if m.schema == "limit")]
    assert isinstance(sel, selector.NumberSelector)
    enum_sel = schema.schema[next(m for m in schema.schema if m.schema == "theme")]
    assert isinstance(enum_sel, selector.SelectSelector)
    bool_sel = schema.schema[next(m for m in schema.schema if m.schema == "enabled")]
    assert isinstance(bool_sel, selector.BooleanSelector)


def test_optional_fields_are_optional():
    schema = build_params_schema(FIELDS)
    assert any(isinstance(m, vol.Optional) and m.schema == "limit" for m in schema.schema)
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_param_form.py -v`
Expected: FAIL (build_params_schema undefined).

- [ ] **Step 3: Implement `build_params_schema`**

Append to `param_form.py`:
```python
import voluptuous as vol
from homeassistant.helpers import selector


def _selector_for(field: dict):
    ftype = field.get("type", "string")
    if ftype in ("int", "float"):
        cfg = selector.NumberSelectorConfig(
            mode=selector.NumberSelectorMode.BOX,
            step=1 if ftype == "int" else "any",
        )
        if field.get("min") is not None:
            cfg["min"] = field["min"]
        if field.get("max") is not None:
            cfg["max"] = field["max"]
        if field.get("unit"):
            cfg["unit_of_measurement"] = field["unit"]
        return selector.NumberSelector(cfg)
    if ftype == "bool":
        return selector.BooleanSelector()
    if ftype == "enum":
        opts = []
        for o in field.get("options", []):
            if isinstance(o, dict):
                opts.append(selector.SelectOptionDict(value=str(o["value"]), label=o.get("label", str(o["value"]))))
            else:
                opts.append(selector.SelectOptionDict(value=str(o), label=str(o)))
        return selector.SelectSelector(
            selector.SelectSelectorConfig(options=opts, mode=selector.SelectSelectorMode.DROPDOWN)
        )
    if ftype == "color":
        return selector.TextSelector(selector.TextSelectorConfig(type=selector.TextSelectorType.COLOR))
    if ftype == "url":
        return selector.TextSelector(selector.TextSelectorConfig(type=selector.TextSelectorType.URL))
    # string
    text_type = selector.TextSelectorType.PASSWORD if field.get("sensitive") else selector.TextSelectorType.TEXT
    return selector.TextSelector(
        selector.TextSelectorConfig(type=text_type, multiline=bool(field.get("multiline")))
    )


def build_params_schema(param_fields: list[dict], current: dict | None = None) -> vol.Schema:
    current = current or {}
    schema: dict = {}
    for field in param_fields:
        if field.get("hidden"):
            continue
        name = field["name"]
        marker_cls = vol.Required if field.get("required") else vol.Optional
        description = None
        if name in current:
            description = {"suggested_value": current[name]}
        elif "default" in field and field["default"] is not None:
            description = {"suggested_value": field["default"]}
        marker = marker_cls(name, description=description)
        schema[marker] = _selector_for(field)
    return vol.Schema(schema)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_param_form.py -v`
Expected: PASS (2 passed).

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/param_form.py tests_ha/test_param_form.py
git commit -m "feat(ha): dynamic @params -> selector form builder

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Device subentry flow — add/edit (`config_flow.py`)

**Files:**
- Modify: `custom_components/byonk/config_flow.py`, `strings.json`, `translations/en.json`
- Test: `tests_ha/test_subentry_flow.py`

**Interfaces:**
- Produces: `ByonkConfigFlow.async_get_supported_subentry_types(config_entry) -> {"device": ByonkDeviceSubentryFlow}`; `class ByonkDeviceSubentryFlow(ConfigSubentryFlow)` with `async_step_user` (pick pending device or MAC + screen → params step) and `async_step_reconfigure`. On submit: `POST`/`PATCH` via the admin client; `async_create_entry(title=key, data={"key": key}, unique_id=key)`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_subentry_flow.py`:
```python
from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og",
            "last_seen": "2026-06-29T09:00:00+00:00"}]
SCREENS = {"screens": [{"name": "transit",
            "params": [{"name": "limit", "type": "int", "default": 8}], "schema_error": None}],
           "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"]}


async def _setup(hass, add_device):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=PENDING)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    return entry


async def test_add_device_posts_and_creates_subentry(hass):
    add_device = AsyncMock(return_value={"key": "ABCD-1234", "screen": "transit"})
    entry = await _setup(hass, add_device)
    with patch("custom_components.byonk.coordinator.ByonkClient.async_add_device", new=add_device):
        result = await hass.config_entries.subentries.async_init(
            (entry.entry_id, "device"), context={"source": "user"}
        )
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"key": "ABCD-1234", "screen": "transit"}
        )
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"limit": 5}
        )
    assert add_device.await_args.args[0]["key"] == "ABCD-1234"
    assert add_device.await_args.args[0]["params"] == {"limit": 5}
    assert any(s.unique_id == "ABCD-1234" for s in entry.subentries.values())
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_subentry_flow.py -v`
Expected: FAIL (no subentry support).

- [ ] **Step 3: Implement the subentry flow**

Add to `config_flow.py`:
```python
from collections.abc import Mapping
from typing import Any

import voluptuous as vol
from homeassistant.config_entries import (
    ConfigEntry,
    ConfigSubentryFlow,
    SubentryFlowResult,
)
from homeassistant.core import callback
from homeassistant.helpers import selector

from .param_form import build_params_schema
```
Add to `ByonkConfigFlow`:
```python
    @classmethod
    @callback
    def async_get_supported_subentry_types(
        cls, config_entry: ConfigEntry
    ) -> dict[str, type[ConfigSubentryFlow]]:
        return {"device": ByonkDeviceSubentryFlow}
```
Add the flow class:
```python
class ByonkDeviceSubentryFlow(ConfigSubentryFlow):
    """Add or edit a device->screen mapping."""

    def __init__(self) -> None:
        self._key: str | None = None
        self._screen: str | None = None
        self._extra: dict[str, Any] = {}

    @property
    def _coordinator(self):
        return self._get_entry().runtime_data

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        data = self._coordinator.data
        if user_input is not None:
            self._key = user_input["key"]
            self._screen = user_input["screen"]
            self._extra = {
                k: user_input[k] for k in ("panel", "dither") if user_input.get(k)
            }
            return await self.async_step_params()

        pending_opts = [
            selector.SelectOptionDict(
                value=p.get("registration_code") or p["mac"],
                label=f'{p.get("registration_code") or p["mac"]} · {p.get("model","?")}',
            )
            for p in data.pending
        ]
        schema = vol.Schema(
            {
                vol.Required("key"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=pending_opts, custom_value=True,
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
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
                        options=data.panel_names(), mode=selector.SelectSelectorMode.DROPDOWN
                    )
                ),
            }
        )
        return self.async_show_form(step_id="user", data_schema=schema)

    async def async_step_params(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        fields = self._coordinator.data.screen_params(self._screen)
        if user_input is not None or not fields:
            params = user_input or {}
            payload = {"key": self._key, "screen": self._screen, "params": params, **self._extra}
            await self._coordinator.client.async_add_device(payload)
            await self._coordinator.async_request_refresh()
            return self.async_create_entry(
                title=self._key, data={"key": self._key}, unique_id=self._key
            )
        return self.async_show_form(
            step_id="params", data_schema=build_params_schema(fields)
        )

    async def async_step_reconfigure(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        sub = self._get_reconfigure_subentry()
        self._key = sub.data["key"]
        device = next(
            (d for d in self._coordinator.data.devices if d["key"] == self._key), {}
        )
        self._screen = device.get("screen")
        fields = self._coordinator.data.screen_params(self._screen)
        if user_input is not None:
            await self._coordinator.client.async_update_device(
                self._key, {"screen": self._screen, "params": user_input}
            )
            await self._coordinator.async_request_refresh()
            return self.async_update_and_abort(
                self._get_entry(), sub, data={"key": self._key}
            )
        return self.async_show_form(
            step_id="reconfigure",
            data_schema=build_params_schema(fields, current=device.get("params") or {}),
        )
```

Add subentry step strings under `"config_subentries"` in `strings.json`/`en.json` (titles for `user`, `params`, `reconfigure`).

> Verify `subentries.async_init`/`async_configure` call shapes and `_get_entry`/`_get_reconfigure_subentry` against the installed HA version (2025.4+). Adjust names if needed.

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_subentry_flow.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations tests_ha/test_subentry_flow.py
git commit -m "feat(ha): device subentry add/edit flow with dynamic params

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 13: Repairs — pending-device issues (`repairs.py`)

**Files:**
- Create: `custom_components/byonk/repairs.py`
- Modify: `custom_components/byonk/coordinator.py` (call sync), `strings.json`/`en.json` (issue text)
- Test: `tests_ha/test_repairs.py`

**Interfaces:**
- Produces: `async_sync_pending_issues(hass, entry_id, pending: list[dict]) -> None` — create `issue_id = f"{ISSUE_PENDING_PREFIX}{code}"` per pending device, delete issues whose code is no longer pending. Called from `_async_reconcile`.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_repairs.py`:
```python
from unittest.mock import AsyncMock, patch

from homeassistant.helpers import issue_registry as ir
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og", "last_seen": None}]
SCREENS = {"screens": [], "panels": [], "dither_algorithms": []}


async def test_pending_creates_repair_issue(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=PENDING)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    reg = ir.async_get(hass)
    assert reg.async_get_issue(DOMAIN, "device_pending_ABCD-1234") is not None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_repairs.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement `repairs.py`**

```python
"""Repairs issues for pending byonk devices."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.helpers import issue_registry as ir

from .const import DOMAIN, ISSUE_PENDING_PREFIX


def async_sync_pending_issues(
    hass: HomeAssistant, pending: list[dict]
) -> None:
    reg = ir.async_get(hass)
    wanted: dict[str, dict] = {}
    for p in pending:
        code = p.get("registration_code") or p.get("mac")
        if not code:
            continue
        wanted[f"{ISSUE_PENDING_PREFIX}{code}"] = p

    for issue_id, p in wanted.items():
        ir.async_create_issue(
            hass,
            DOMAIN,
            issue_id,
            is_fixable=False,
            severity=ir.IssueSeverity.WARNING,
            translation_key="device_pending",
            translation_placeholders={
                "code": p.get("registration_code") or p.get("mac"),
                "model": p.get("model") or "TRMNL",
            },
        )

    for issue in list(reg.issues.values()):
        if (
            issue.domain == DOMAIN
            and issue.issue_id.startswith(ISSUE_PENDING_PREFIX)
            and issue.issue_id not in wanted
        ):
            ir.async_delete_issue(hass, DOMAIN, issue.issue_id)
```

In `coordinator.py`, import and call inside `_async_reconcile` (or right after building data):
```python
from .repairs import async_sync_pending_issues
...
        async_sync_pending_issues(self.hass, data.pending)
```

Add to `strings.json`/`en.json`:
```json
{
  "issues": {
    "device_pending": {
      "title": "TRMNL device {code} is waiting to be set up",
      "description": "A {model} device showing code {code} has connected but is not configured. Add it via the Byonk integration's Add device action."
    }
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `python -m pytest tests_ha/test_repairs.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/repairs.py custom_components/byonk/coordinator.py custom_components/byonk/strings.json custom_components/byonk/translations tests_ha/test_repairs.py
git commit -m "feat(ha): Repairs issues for pending devices

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 14: Reauth (re-provision) + full-suite green + docs/changelog

**Files:**
- Modify: `custom_components/byonk/config_flow.py` (reauth step), `__init__.py`
- Create: `docs/src/getting-started/home-assistant-integration.md`
- Modify: `docs/src/SUMMARY.md`, `CHANGES.md`
- Test: `tests_ha/test_reauth.py`

**Interfaces:**
- Produces: `ByonkConfigFlow.async_step_reauth/async_step_reauth_confirm` that re-reads or re-provisions the token (option blank → `async_provision_token`) and reloads. `ConfigEntryAuthFailed` from setup/coordinator triggers it.

- [ ] **Step 1: Write the failing test**

`tests_ha/test_reauth.py`:
```python
from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN


async def test_reauth_reprovisions_when_blank(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    provision = AsyncMock(return_value="newtok")
    with (
        patch("custom_components.byonk.config_flow.async_read_token", new=AsyncMock(return_value=None)),
        patch("custom_components.byonk.config_flow.async_provision_token", new=provision),
    ):
        result = await entry.start_reauth_flow(hass)
        if result.get("type") == "form":
            result = await hass.config_entries.flow.async_configure(result["flow_id"], {})
    provision.assert_awaited_once()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests_ha/test_reauth.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement reauth in `config_flow.py`**

```python
    async def async_step_reauth(
        self, entry_data: Mapping[str, Any]
    ) -> ConfigFlowResult:
        return await self.async_step_reauth_confirm()

    async def async_step_reauth_confirm(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        entry = self._get_reauth_entry()
        slug = entry.data[CONF_ADDON_SLUG]
        token = await async_read_token(self.hass, slug)
        if not token:
            await async_provision_token(self.hass, slug)
        return self.async_update_reload_and_abort(entry, data=entry.data)
```
(Import `Mapping`, `async_provision_token` at top of file if not already.)

- [ ] **Step 4: Run the reauth test**

Run: `python -m pytest tests_ha/test_reauth.py -v`
Expected: PASS. (Adjust to `_get_reauth_entry`/`async_update_reload_and_abort` names per installed HA version.)

- [ ] **Step 5: Write docs + changelog**

Create `docs/src/getting-started/home-assistant-integration.md` describing: install the integration via HACS (custom repo = the byonk repo); Add Integration → it installs the add-on, provisions trust, no token needed; the hub + per-device entities; onboarding via Repairs + Add device; that it requires Supervised/HAOS. Add a link line under Getting Started in `docs/src/SUMMARY.md`. Add an Unreleased entry to `CHANGES.md` describing the HA integration.

- [ ] **Step 6: Run the full Python suite + ruff + docs**

Run: `python -m pytest tests_ha -q && ruff check custom_components/byonk && (cd docs && mdbook build)`
Expected: all pass; mdBook builds clean.

- [ ] **Step 7: Commit**

```bash
git add custom_components/byonk docs CHANGES.md tests_ha/test_reauth.py
git commit -m "feat(ha): reauth/re-provision + docs + changelog

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review (completed by plan author)

**Spec coverage:**
- §4 trust/lifecycle → Tasks 3, 4, 14 (reauth). ✓
- §5 data layer + reconciliation → Tasks 2, 5, 6. ✓
- §6 entities (hub + per-device) → Tasks 7, 8, 9, 10. ✓
- §7 subentry flow + Repairs onboarding → Tasks 11, 12, 13. ✓
- §8 byonk-side: no change required → no task (correct). ✓
- §9 selectors → Task 11. ✓
- §10 files/manifest/hacs → Tasks 1, 5 (modules). ✓
- §11 testing → every task is TDD; full suite in Task 14. ✓
- §12 staged build → Stages 1–4 map 1:1. ✓

**Placeholder scan:** No "TBD/handle edge cases/similar to Task N" — each step carries real code/commands. The few "verify against installed HA version" notes are version-pinning cautions, not missing content (the canonical symbol names are provided).

**Type consistency:** `ByonkClient` method names match between `api.py` (Task 2) and all callers; `ByonkData` helpers (`screen_names`, `screen_params`, `dither`, `panel_names`, `default_screen`, `registration_enabled`, `auth_mode`) defined in Task 5 and used consistently in Tasks 7–13; `default_params`/`build_params_schema` defined before use (Tasks 9/11); `CONF_ADDON_SLUG`/`CONF_BASE_URL` consistent; `ISSUE_PENDING_PREFIX` defined Task 1, used Task 13.

**Known version-sensitive surfaces (flagged in-task, not blockers):** config-subentry method/field names (HA 2025.4 rename), `aiohasupervisor` model import paths, `subentries.async_init/async_configure` shapes. Each task tells the implementer to verify and adjust against the installed HA version.
