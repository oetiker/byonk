# Screen Packages Plan 3 — HA package management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface byonk's Plan-2 package distribution in Home Assistant — packages as native config subentries, singleton server settings via an Options Flow, and per-package status + an update-all button as hub entities.

**Architecture:** The HA integration is a thin front over byonk's token-gated admin API; byonk stays the source of truth. Packages become config **subentries** of the hub entry (Add/Reconfigure flows call `POST/PATCH /packages`; a hub update-listener propagates subentry deletes to `DELETE /packages`; the coordinator reconciles subentries ↔ byonk's registry). Singleton settings move off two hub select entities into an **Options Flow** over `PATCH /settings`. Per-package **status sensors** and one **button** are reconciled from `GET /packages`.

**Tech Stack:** Python, Home Assistant custom component (`config_entries`, `ConfigSubentryFlow`, `OptionsFlow`, `DataUpdateCoordinator`), `pytest-homeassistant-custom-component`, `aioresponses`, `ruff`.

## Global Constraints

- **Byonk is the single source of truth.** HA persists no authoritative registry copy and **never persists a package token**. Token fields are write-only, passed straight to byonk. (spec §2, §4.2)
- **No byonk-core / admin-API changes.** The §9a API is complete. (spec §2)
- **No config migration.** No users yet; existing HA config may be regenerated. (spec §1)
- **`byonk-builtin` is never a subentry, never a status sensor** (embedded, `builtin: true`). (spec §4.4, §6)
- **Reference implementation in git:** flow-based `ConfigSubentryFlow` for this exact codebase/HA version lived at commit `b9f89df^` (`custom_components/byonk/config_flow.py`, class `ByonkDeviceSubentryFlow`) before Phase 5 removed device subentries. Consult it for idioms; do **not** copy device semantics.
- **Byonk settings keys** (verbatim, from `src/api/admin/write.rs` `SettingsWrite`): `registration_screen`, `auth_mode`, `package_refresh_interval` (also `registration_enabled`, `default_screen` — not touched here). "New-device screen" = `registration_screen`.
- **Run tests** from repo root: `.venv/bin/pytest tests_ha/… -v` and lint with `.venv/bin/ruff check custom_components/byonk tests_ha`. `make ha-check` runs ruff (on `custom_components/byonk`) + pytest.
- **All user-visible changes** documented in `CHANGES.md` (Unreleased) and `docs/src/`.

---

## File structure

- `custom_components/byonk/api.py` — MODIFY: 5 package methods; `ByonkReadOnlyError` carries byonk's message.
- `custom_components/byonk/coordinator.py` — MODIFY: `ByonkData.packages` + accessors; fetch packages; package-subentry reconcile.
- `custom_components/byonk/config_flow.py` — MODIFY: `async_get_supported_subentry_types` + `ByonkPackageSubentryFlow` (add/reconfigure); `async_get_options_flow` + `ByonkOptionsFlow`.
- `custom_components/byonk/__init__.py` — MODIFY: hub update-listener → propagate subentry delete to byonk.
- `custom_components/byonk/package_entities.py` — CREATE: `PackageStatusManager` + `ByonkPackageStatusSensor` + `setup_package_status_platform`.
- `custom_components/byonk/sensor.py` — MODIFY: hub branch wires the status manager.
- `custom_components/byonk/button.py` — CREATE: `ByonkUpdatePackagesButton`.
- `custom_components/byonk/select.py` — MODIFY: delete `ByonkNewDeviceScreenSelect` + `ByonkAuthModeSelect`.
- `custom_components/byonk/const.py` — MODIFY: add `Platform.BUTTON`.
- `custom_components/byonk/strings.json` + `translations/en.json` — MODIFY: options flow, subentry, button, status-sensor strings; drop the two select strings.
- `tests_ha/conftest.py` — MODIFY: package state + package client methods in the `byonk` fixture.
- `tests_ha/…` — CREATE test modules per task.

---

### Task 1: API client — package methods + richer 409

**Files:**
- Modify: `custom_components/byonk/api.py`
- Modify: `tests_ha/conftest.py`
- Test: `tests_ha/test_api.py`

**Interfaces:**
- Produces (on `ByonkClient`): `async_get_packages() -> list[dict]`, `async_add_package(payload: dict) -> dict`, `async_update_package(handle: str, payload: dict) -> dict`, `async_delete_package(handle: str) -> dict`, `async_update_packages() -> dict`.
- Produces: `ByonkReadOnlyError.message: str` (byonk's error text, or `""`).
- Produces (conftest `byonk` fixture `state`): `packages: list[dict]`, `add_package`, `update_package`, `delete_package`, `update_packages` AsyncMocks.

- [ ] **Step 1: Write failing tests** in `tests_ha/test_api.py` (append):

```python
async def test_get_packages(mock_aioresponse):
    mock_aioresponse.get(
        f"{BASE}/api/admin/packages",
        payload=[{"handle": "weather", "builtin": False, "status": "ready"}],
    )
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        result = await client.async_get_packages()
    assert result[0]["handle"] == "weather"


async def test_add_package_posts(mock_aioresponse):
    mock_aioresponse.post(f"{BASE}/api/admin/packages", payload={"handle": "weather"})
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        await client.async_add_package({"handle": "weather", "repo": "r", "pin": "main"})
    req = next(iter(mock_aioresponse.requests.values()))[0]
    assert req.kwargs["json"]["handle"] == "weather"


async def test_update_package_patches_handle(mock_aioresponse):
    mock_aioresponse.patch(f"{BASE}/api/admin/packages/weather", payload={"handle": "weather"})
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        await client.async_update_package("weather", {"pin": "v2"})


async def test_update_all_packages_posts(mock_aioresponse):
    mock_aioresponse.post(f"{BASE}/api/admin/packages/update", payload={"ok": True})
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        assert await client.async_update_packages() == {"ok": True}


async def test_delete_package_409_carries_message(mock_aioresponse):
    mock_aioresponse.delete(
        f"{BASE}/api/admin/packages/weather",
        status=409,
        payload={"error": "package `weather` is referenced by device `AA:BB`"},
    )
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        with pytest.raises(ByonkReadOnlyError) as exc:
            await client.async_delete_package("weather")
    assert "referenced by device" in exc.value.message
```

- [ ] **Step 2: Run — verify they fail**

Run: `.venv/bin/pytest tests_ha/test_api.py -v`
Expected: FAIL (`AttributeError: ... async_get_packages`; `ByonkReadOnlyError` has no `message`).

- [ ] **Step 3: Implement** in `custom_components/byonk/api.py`.

Enrich the 409 error to carry a message. Change the class:

```python
class ByonkReadOnlyError(ByonkApiError):
    """Config is embedded/read-only, or a delete is blocked by a reference (409)."""

    def __init__(self, message: str = "") -> None:
        super().__init__(message)
        self.message = message
```

In `_request`, replace the 409 branch so it reads the body message like the 400 branch:

```python
                if resp.status == 409:
                    body = await _safe_json(resp)
                    raise ByonkReadOnlyError(
                        body.get("error") or body.get("message") or ""
                    )
```

Add the methods after `async_update_settings`:

```python
    async def async_get_packages(self) -> list[dict]:
        return await self._request("GET", "/api/admin/packages")

    async def async_add_package(self, payload: dict) -> dict:
        return await self._request("POST", "/api/admin/packages", json=payload)

    async def async_update_package(self, handle: str, payload: dict) -> dict:
        return await self._request("PATCH", f"/api/admin/packages/{handle}", json=payload)

    async def async_delete_package(self, handle: str) -> dict:
        return await self._request("DELETE", f"/api/admin/packages/{handle}")

    async def async_update_packages(self) -> dict:
        return await self._request("POST", "/api/admin/packages/update")
```

- [ ] **Step 4: Extend the `byonk` fixture** in `tests_ha/conftest.py`. In the `state = SimpleNamespace(...)` add:

```python
        packages=[],
        add_package=AsyncMock(return_value={"handle": "x"}),
        update_package=AsyncMock(return_value={"handle": "x"}),
        delete_package=AsyncMock(return_value={"ok": True}),
        update_packages=AsyncMock(return_value={"ok": True}),
```

and in the `patch.multiple("custom_components.byonk.coordinator.ByonkClient", ...)` block add:

```python
            async_get_packages=AsyncMock(side_effect=lambda *a, **k: list(state.packages)),
            async_add_package=state.add_package,
            async_update_package=state.update_package,
            async_delete_package=state.delete_package,
            async_update_packages=state.update_packages,
```

- [ ] **Step 5: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_api.py -v`
Expected: PASS (incl. existing `test_409_raises_readonly`, which still raises `ByonkReadOnlyError`).

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/api.py tests_ha/conftest.py tests_ha/test_api.py
git commit -m "feat(ha): admin API client methods for packages"
```

---

### Task 2: Coordinator — packages in `ByonkData`

**Files:**
- Modify: `custom_components/byonk/coordinator.py`
- Test: `tests_ha/test_package_reconcile.py` (created here; extended in Task 5)

**Interfaces:**
- Consumes: `ByonkClient.async_get_packages` (Task 1).
- Produces: `ByonkData.packages: list[dict]`; `ByonkData.non_builtin_packages() -> list[dict]`; `ByonkData.package(handle) -> dict | None`.

- [ ] **Step 1: Write failing test** `tests_ha/test_package_reconcile.py`:

```python
from tests_ha.conftest import make_hub_entry

PKGS = [
    {"handle": "byonk-builtin", "builtin": True, "status": "ready"},
    {"handle": "weather", "builtin": False, "repo": "r", "pin": "main",
     "pin_kind": "branch", "resolved_sha": "abc", "status": "ready",
     "last_fetched": "2026-07-04T00:00:00+00:00", "error": None},
]


async def test_coordinator_exposes_packages(hass, byonk):
    byonk.packages = PKGS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    data = hub.runtime_data.data
    assert [p["handle"] for p in data.non_builtin_packages()] == ["weather"]
    assert data.package("weather")["status"] == "ready"
    assert data.package("missing") is None
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_reconcile.py -v`
Expected: FAIL (`ByonkData` has no `packages` / `non_builtin_packages`).

- [ ] **Step 3: Implement** in `coordinator.py`.

Add to the `ByonkData` dataclass fields (after `config: dict`):

```python
    packages: list[dict]
```

Add accessors to `ByonkData`:

```python
    def non_builtin_packages(self) -> list[dict]:
        return [p for p in self.packages if not p.get("builtin")]

    def package(self, handle: str) -> dict | None:
        for p in self.packages:
            if p.get("handle") == handle:
                return p
        return None
```

In `_async_update_data`, add the fetch to the `asyncio.gather` and the constructor:

```python
            devices, pending, screens, config, packages = await asyncio.gather(
                self.client.async_get_devices(),
                self.client.async_get_pending(),
                self.client.async_get_screens(),
                self.client.async_get_config(),
                self.client.async_get_packages(),
            )
```

and pass `packages=packages` to `ByonkData(...)`.

- [ ] **Step 4: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_reconcile.py -v`
Expected: PASS.

- [ ] **Step 5: Run the full suite** (a new required `ByonkData` field can break other constructions):

Run: `.venv/bin/pytest tests_ha -q`
Expected: PASS (only `coordinator.py` constructs `ByonkData`; the fixture already returns `packages`).

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/coordinator.py tests_ha/test_package_reconcile.py
git commit -m "feat(ha): coordinator fetches package registry"
```

---

### Task 3: Package subentry — Add flow

**Files:**
- Modify: `custom_components/byonk/config_flow.py`
- Modify: `custom_components/byonk/strings.json`, `custom_components/byonk/translations/en.json`
- Test: `tests_ha/test_package_subentry_flow.py`

**Interfaces:**
- Consumes: `ByonkClient.async_add_package` (Task 1); `hub.runtime_data` coordinator.
- Produces: subentry type `"package"`; class `ByonkPackageSubentryFlow(ConfigSubentryFlow)` with `async_step_user`. Subentry created with `unique_id=<handle>`, `data={"handle","repo","pin"}` (**no token**).

- [ ] **Step 1: Write failing tests** `tests_ha/test_package_subentry_flow.py`:

```python
from tests_ha.conftest import make_hub_entry


async def _setup(hass, byonk):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_add_package_posts_and_creates_subentry(hass, byonk):
    hub = await _setup(hass, byonk)
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "user"}
    )
    result = await hass.config_entries.subentries.async_configure(
        result["flow_id"],
        {"handle": "weather", "repo": "github.com/acme/screens", "pin": "main", "token": "s3cr3t"},
    )
    assert result["type"] == "create_entry"
    assert byonk.add_package.await_args.args[0] == {
        "handle": "weather", "repo": "github.com/acme/screens", "pin": "main", "token": "s3cr3t",
    }
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    assert "token" not in sub.data  # token never persisted


async def test_add_package_surfaces_byonk_error(hass, byonk):
    from custom_components.byonk.api import ByonkValidationError
    byonk.add_package.side_effect = ByonkValidationError("package `weather` already exists")
    hub = await _setup(hass, byonk)
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "user"}
    )
    result = await hass.config_entries.subentries.async_configure(
        result["flow_id"], {"handle": "weather", "repo": "r", "pin": "main"}
    )
    assert result["type"] == "form"
    assert result["errors"]["base"] == "add_failed"
    assert not any(s.unique_id == "weather" for s in hub.subentries.values())
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_subentry_flow.py -v`
Expected: FAIL (`package` is not a supported subentry type).

- [ ] **Step 3: Implement** in `config_flow.py`.

Add imports:

```python
from homeassistant.config_entries import (
    ConfigFlow,
    ConfigFlowResult,
    ConfigSubentryFlow,
    SubentryFlowResult,
)
from .api import ByonkApiError, ByonkAuthError, ByonkClient, ByonkValidationError
```

On `ByonkConfigFlow`, add the classmethod (gate to the hub entry — device entries get none):

```python
    @classmethod
    @callback
    def async_get_supported_subentry_types(
        cls, config_entry
    ) -> dict[str, type[ConfigSubentryFlow]]:
        if CONF_DEVICE_KEY in config_entry.data:
            return {}
        return {"package": ByonkPackageSubentryFlow}
```

Add the flow class at module end:

```python
class ByonkPackageSubentryFlow(ConfigSubentryFlow):
    """Add or edit a package (handle -> {repo, pin, token})."""

    @property
    def _coordinator(self):
        return self._get_entry().runtime_data

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        errors: dict[str, str] = {}
        if user_input is not None:
            payload = {
                "handle": user_input["handle"],
                "repo": user_input["repo"],
                "pin": user_input.get("pin") or "main",
            }
            if user_input.get("token"):
                payload["token"] = user_input["token"]
            try:
                await self._coordinator.client.async_add_package(payload)
            except ByonkApiError as err:
                errors["base"] = "add_failed"
                return self.async_show_form(
                    step_id="user",
                    data_schema=_package_schema(user_input),
                    errors=errors,
                    description_placeholders={"error": str(err)},
                )
            # Do NOT refresh the coordinator here: a refresh runs the package
            # reconcile (Task 5), which would create this subentry itself and make
            # async_create_entry abort "already_configured". The subentry-change
            # listener refreshes and builds the status sensor.
            return self.async_create_entry(
                title=f'{user_input["handle"]} — {user_input["repo"]}',
                data={"handle": user_input["handle"], "repo": user_input["repo"], "pin": payload["pin"]},
                unique_id=user_input["handle"],
            )
        return self.async_show_form(step_id="user", data_schema=_package_schema())
```

Add the schema helper at module level (reused by Task 4):

```python
def _package_schema(current: dict | None = None) -> vol.Schema:
    current = current or {}
    return vol.Schema(
        {
            vol.Required("handle", default=current.get("handle", "")): str,
            vol.Required("repo", default=current.get("repo", "")): str,
            vol.Optional("pin", default=current.get("pin", "main")): str,
            vol.Optional("token"): selector.TextSelector(
                selector.TextSelectorConfig(type=selector.TextSelectorType.PASSWORD)
            ),
        }
    )
```

Note: in Task 4 the reconfigure step reuses `_package_schema` but with `handle` read-only — handled there.

- [ ] **Step 4: Add strings.** In `strings.json` and `translations/en.json`, add under a top-level `"config_subentries"` key:

```json
  "config_subentries": {
    "package": {
      "title": "Screen package",
      "initiate_flow": { "user": "Add package", "reconfigure": "Edit package" },
      "step": {
        "user": {
          "title": "Add a screen package",
          "description": "Register a git-backed screen package. {error}",
          "data": { "handle": "Handle", "repo": "Repository URL", "pin": "Pin (branch, tag or sha)", "token": "Access token (optional)" }
        }
      },
      "error": { "add_failed": "byonk rejected the package: {error}" }
    }
  }
```

- [ ] **Step 5: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_subentry_flow.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_package_subentry_flow.py
git commit -m "feat(ha): add-package subentry flow"
```

---

### Task 4: Package subentry — Reconfigure flow

**Files:**
- Modify: `custom_components/byonk/config_flow.py`
- Modify: `custom_components/byonk/strings.json`, `custom_components/byonk/translations/en.json`
- Test: `tests_ha/test_package_subentry_flow.py`

**Interfaces:**
- Consumes: `ByonkClient.async_update_package` (Task 1); `_package_schema` (Task 3); `data.package(handle)` (Task 2).
- Produces: `ByonkPackageSubentryFlow.async_step_reconfigure`.

- [ ] **Step 1: Write failing tests** (append to `tests_ha/test_package_subentry_flow.py`):

```python
PKG = {"handle": "weather", "builtin": False, "repo": "github.com/acme/screens",
       "pin": "main", "pin_kind": "branch", "resolved_sha": "abc",
       "status": "ready", "last_fetched": None, "error": None}


async def _hub_with_pkg(hass, byonk):
    from homeassistant.config_entries import ConfigSubentry, ConfigSubentryData
    byonk.packages = [PKG]
    hub = make_hub_entry(hass)
    hub.subentries = {}
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_reconfigure_patches_pin(hass, byonk):
    hub = await _hub_with_pkg(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "reconfigure",
        "subentry_id": sub.subentry_id},
    )
    result = await hass.config_entries.subentries.async_configure(
        result["flow_id"], {"handle": "weather", "repo": PKG["repo"], "pin": "v2.0.0"}
    )
    assert result["type"] == "abort"
    assert byonk.update_package.await_args.args[0] == "weather"
    assert byonk.update_package.await_args.args[1]["pin"] == "v2.0.0"


async def test_reconfigure_blank_token_omits_token(hass, byonk):
    hub = await _hub_with_pkg(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "reconfigure",
        "subentry_id": sub.subentry_id},
    )
    await hass.config_entries.subentries.async_configure(
        result["flow_id"], {"handle": "weather", "repo": PKG["repo"], "pin": "main"}
    )
    assert "token" not in byonk.update_package.await_args.args[1]
```

Note: `_hub_with_pkg` relies on the Task-5 reconcile to create the subentry from `byonk.packages`. If Task 5 is not yet implemented when running this test in isolation, create the subentry explicitly first; once Task 5 lands, the reconcile creates it.

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_subentry_flow.py -k reconfigure -v`
Expected: FAIL (no `async_step_reconfigure`).

- [ ] **Step 3: Implement** — add to `ByonkPackageSubentryFlow`:

```python
    async def async_step_reconfigure(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        sub = self._get_reconfigure_subentry()
        handle = sub.data["handle"]
        pkg = self._coordinator.data.package(handle) or sub.data
        if user_input is not None:
            payload = {"repo": user_input["repo"], "pin": user_input.get("pin") or "main"}
            if user_input.get("token"):
                payload["token"] = user_input["token"]
            try:
                await self._coordinator.client.async_update_package(handle, payload)
            except ByonkApiError as err:
                return self.async_show_form(
                    step_id="reconfigure",
                    data_schema=_package_schema({"handle": handle, **user_input}),
                    errors={"base": "add_failed"},
                    description_placeholders={"error": str(err)},
                )
            await self._coordinator.async_request_refresh()
            return self.async_update_and_abort(
                self._get_entry(), sub,
                title=f'{handle} — {user_input["repo"]}',
                data={"handle": handle, "repo": user_input["repo"], "pin": payload["pin"]},
            )
        return self.async_show_form(
            step_id="reconfigure",
            data_schema=_package_schema({"handle": handle, "repo": pkg.get("repo", ""), "pin": pkg.get("pin", "main")}),
        )
```

Make `handle` read-only on reconfigure: in `_package_schema`, when a `handle` default is present and we are reconfiguring, HA shows it; to prevent edits, add `vol.In([current["handle"]])` — simplest is to keep the field but ignore any change (the impl above always uses `sub.data["handle"]`). Leave the schema as-is; the handler ignores a changed handle.

- [ ] **Step 4: Add the `reconfigure` step strings** under `config_subentries.package.step` in both JSON files:

```json
        "reconfigure": {
          "title": "Edit package {handle}",
          "description": "Change the repository, pin, or token. Leave token blank to keep the current one. {error}",
          "data": { "handle": "Handle", "repo": "Repository URL", "pin": "Pin (branch, tag or sha)", "token": "New access token (optional)" }
        }
```

- [ ] **Step 5: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_subentry_flow.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_package_subentry_flow.py
git commit -m "feat(ha): reconfigure-package subentry flow"
```

---

### Task 5: Coordinator reconcile — subentries ↔ byonk registry

**Files:**
- Modify: `custom_components/byonk/coordinator.py`
- Test: `tests_ha/test_package_reconcile.py`

**Interfaces:**
- Consumes: `ByonkData.non_builtin_packages` (Task 2); `hass.config_entries.async_add_subentry` / `async_remove_subentry` / `async_update_subentry`.
- Produces: package reconcile inside `_async_reconcile` — creates a `"package"` subentry for each byonk non-builtin handle without one, removes subentries whose handle byonk no longer has, updates title/data when repo/pin change. `byonk-builtin` excluded.

- [ ] **Step 1: Write failing tests** (append to `tests_ha/test_package_reconcile.py`):

```python
import pytest


async def test_reconcile_creates_and_removes_subentries(hass, byonk):
    byonk.packages = PKGS  # builtin + weather
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    # weather present, builtin excluded
    handles = {s.unique_id for s in hub.subentries.values() if s.subentry_type == "package"}
    assert handles == {"weather"}

    # byonk drops weather -> reconcile removes the subentry on next refresh
    byonk.packages = [PKGS[0]]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    handles = {s.unique_id for s in hub.subentries.values() if s.subentry_type == "package"}
    assert handles == set()


async def test_reconcile_updates_title_on_pin_change(hass, byonk):
    byonk.packages = PKGS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    byonk.packages = [PKGS[0], {**PKGS[1], "pin": "v9"}]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    assert sub.data["pin"] == "v9"
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_reconcile.py -k reconcile -v`
Expected: FAIL (no subentries created).

- [ ] **Step 3: Implement.** Add imports at top of `coordinator.py`:

```python
from homeassistant.config_entries import ConfigSubentry, ConfigSubentryData
```

At the end of `_async_reconcile`, call a new helper, then define it:

```python
    async def _async_reconcile(self, data: ByonkData) -> None:
        ...  # existing device reconcile unchanged
        self._reconcile_packages(data)

    @callback
    def _reconcile_packages(self, data: ByonkData) -> None:
        subs = {
            s.unique_id: s
            for s in self.entry.subentries.values()
            if s.subentry_type == "package"
        }
        byonk = {p["handle"]: p for p in data.non_builtin_packages()}

        # Remove subentries byonk no longer has.
        for handle, sub in subs.items():
            if handle not in byonk:
                self.hass.config_entries.async_remove_subentry(self.entry, sub.subentry_id)

        for handle, pkg in byonk.items():
            title = f'{handle} — {pkg.get("repo", "")}'
            want = {"handle": handle, "repo": pkg.get("repo"), "pin": pkg.get("pin")}
            sub = subs.get(handle)
            if sub is None:
                self.hass.config_entries.async_add_subentry(
                    self.entry,
                    ConfigSubentry(
                        data=ConfigSubentryData(want),
                        subentry_type="package",
                        title=title,
                        unique_id=handle,
                    ),
                )
            elif dict(sub.data) != want or sub.title != title:
                self.hass.config_entries.async_update_subentry(
                    self.entry, sub, data=want, title=title
                )
```

Note the add-flow race guard (Task 3): because the add flow does **not** refresh, `_reconcile_packages` only sees the new package after the flow's own `async_create_entry`, at which point the subentry already exists → the `sub is None` branch is skipped.

- [ ] **Step 4: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_reconcile.py -v`
Expected: PASS.

- [ ] **Step 5: Run subentry-flow tests** (Task 4's `_hub_with_pkg` now relies on this reconcile):

Run: `.venv/bin/pytest tests_ha/test_package_subentry_flow.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/coordinator.py tests_ha/test_package_reconcile.py
git commit -m "feat(ha): reconcile package subentries against byonk registry"
```

---

### Task 6: Delete propagation — hub subentry-change listener

**Files:**
- Modify: `custom_components/byonk/__init__.py`
- Test: `tests_ha/test_package_delete_propagation.py`

**Interfaces:**
- Consumes: `ByonkClient.async_delete_package` (Task 1); `ByonkReadOnlyError.message` (Task 1); the reconcile (Task 5, for the 409 self-heal).
- Produces: a hub-entry update listener that, when a `"package"` subentry disappears from `entry.subentries`, calls `DELETE /packages/:handle`. On 409 it logs the reference message; the next reconcile re-creates the subentry (self-heal).

- [ ] **Step 1: Write failing tests** `tests_ha/test_package_delete_propagation.py`:

```python
import logging
from tests_ha.conftest import make_hub_entry

PKG = {"handle": "weather", "builtin": False, "repo": "r", "pin": "main",
       "pin_kind": "branch", "resolved_sha": "abc", "status": "ready",
       "last_fetched": None, "error": None}


async def _hub(hass, byonk):
    byonk.packages = [PKG]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_removing_subentry_deletes_from_byonk(hass, byonk):
    hub = await _hub(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    byonk.packages = []  # byonk will report it gone after our delete
    hass.config_entries.async_remove_subentry(hub, sub.subentry_id)
    await hass.async_block_till_done()
    assert byonk.delete_package.await_args.args[0] == "weather"


async def test_delete_409_logs_and_self_heals(hass, byonk, caplog):
    from custom_components.byonk.api import ByonkReadOnlyError
    hub = await _hub(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    byonk.delete_package.side_effect = ByonkReadOnlyError(
        "package `weather` is referenced by device `AA:BB`"
    )
    with caplog.at_level(logging.WARNING):
        hass.config_entries.async_remove_subentry(hub, sub.subentry_id)
        await hass.async_block_till_done()
    assert "referenced by device" in caplog.text
    # byonk still has it -> reconcile re-creates the subentry
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert any(s.unique_id == "weather" for s in hub.subentries.values())
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_delete_propagation.py -v`
Expected: FAIL (`delete_package` never called).

- [ ] **Step 3: Implement** in `__init__.py`.

Add imports:

```python
import logging
from .api import ByonkApiError, ByonkClient, ByonkReadOnlyError

_LOGGER = logging.getLogger(__name__)
```

In `_async_setup_hub_entry`, after `entry.runtime_data = coordinator`, seed the snapshot **on the coordinator** (not on the `ConfigEntry`, which may restrict attribute assignment) and register the listener:

```python
    coordinator._pkg_handles = {
        s.unique_id for s in entry.subentries.values() if s.subentry_type == "package"
    }
    entry.async_on_unload(entry.add_update_listener(_async_hub_updated))
```

Add the listener:

```python
async def _async_hub_updated(hass: HomeAssistant, entry: ByonkConfigEntry) -> None:
    """Propagate a removed package subentry to byonk (DELETE /packages/:handle)."""
    coordinator = entry.runtime_data
    current = {
        s.unique_id for s in entry.subentries.values() if s.subentry_type == "package"
    }
    previous = getattr(coordinator, "_pkg_handles", set())
    coordinator._pkg_handles = current
    removed = previous - current
    if not removed:
        return
    for handle in removed:
        try:
            await coordinator.client.async_delete_package(handle)
        except ByonkReadOnlyError as err:
            _LOGGER.warning(
                "cannot delete package %s: %s (it will reappear until the "
                "reference is cleared)", handle, err.message,
            )
        except ByonkApiError as err:
            _LOGGER.warning("delete package %s failed: %s", handle, err)
    await coordinator.async_request_refresh()
```

Note: the update listener fires on every entry change (incl. reconcile's `async_add_subentry`). The `previous`/`current` snapshot diff makes non-removals a no-op, so reconcile-driven adds do not trigger spurious deletes.

- [ ] **Step 4: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_delete_propagation.py -v`
Expected: PASS.

- [ ] **Step 5: Run full suite** (the update listener touches all hub flows):

Run: `.venv/bin/pytest tests_ha -q`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/__init__.py tests_ha/test_package_delete_propagation.py
git commit -m "feat(ha): propagate package subentry deletion to byonk"
```

---

### Task 7: Options Flow — server settings

**Files:**
- Modify: `custom_components/byonk/config_flow.py`
- Modify: `custom_components/byonk/strings.json`, `custom_components/byonk/translations/en.json`
- Test: `tests_ha/test_options_flow.py`

**Interfaces:**
- Consumes: `ByonkClient.async_update_settings` (existing); `data.screen_names()`, `data.registration_screen()`, `data.auth_mode()` (existing).
- Produces: `ByonkConfigFlow.async_get_options_flow` → `ByonkOptionsFlow` with `async_step_init` writing `registration_screen`, `auth_mode`, `package_refresh_interval` via `PATCH /settings`.

- [ ] **Step 1: Write failing test** `tests_ha/test_options_flow.py`:

```python
from tests_ha.conftest import make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"


async def test_options_flow_writes_settings(hass, byonk):
    byonk.config = {"registration": {"enabled": True, "screen": ""},
                    "auth_mode": "api_key", "package_refresh_interval": 3600}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    result = await hass.config_entries.options.async_init(hub.entry_id)
    assert result["type"] == "form"
    result = await hass.config_entries.options.async_configure(
        result["flow_id"],
        {"registration_screen": TRANSIT_REF, "auth_mode": "ed25519",
         "package_refresh_interval": 900},
    )
    assert result["type"] == "create_entry"
    sent = byonk.update_settings.await_args.args[0]
    assert sent["registration_screen"] == TRANSIT_REF
    assert sent["auth_mode"] == "ed25519"
    assert sent["package_refresh_interval"] == 900
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_options_flow.py -v`
Expected: FAIL (no options flow → `UnknownHandler` / abort).

- [ ] **Step 3: Implement** in `config_flow.py`.

Add import:

```python
from homeassistant.config_entries import OptionsFlow
```

On `ByonkConfigFlow`:

```python
    @staticmethod
    @callback
    def async_get_options_flow(config_entry) -> OptionsFlow:
        return ByonkOptionsFlow()
```

Add the class:

```python
class ByonkOptionsFlow(OptionsFlow):
    """Server-level settings that byonk owns (thin front over PATCH /settings)."""

    async def async_step_init(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        coordinator = self.config_entry.runtime_data
        if user_input is not None:
            screen = user_input["registration_screen"]
            await coordinator.client.async_update_settings(
                {
                    "registration_screen": "" if screen == BUILTIN_SCREEN_LABEL else screen,
                    "auth_mode": user_input["auth_mode"],
                    "package_refresh_interval": int(user_input["package_refresh_interval"]),
                }
            )
            await coordinator.async_request_refresh()
            return self.async_create_entry(title="", data={})

        data = coordinator.data
        current_screen = data.registration_screen() or BUILTIN_SCREEN_LABEL
        interval = data.config.get("package_refresh_interval", 0)
        schema = vol.Schema(
            {
                vol.Required("registration_screen", default=current_screen): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=[BUILTIN_SCREEN_LABEL, *data.screen_names()],
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Required("auth_mode", default=data.auth_mode() or "api_key"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=["api_key", "ed25519"],
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Required("package_refresh_interval", default=interval): selector.NumberSelector(
                    selector.NumberSelectorConfig(
                        min=0, max=86400, step=1, unit_of_measurement="s",
                        mode=selector.NumberSelectorMode.BOX,
                    )
                ),
            }
        )
        return self.async_show_form(step_id="init", data_schema=schema)
```

Ensure `BUILTIN_SCREEN_LABEL` is imported in `config_flow.py` (`from .const import ..., BUILTIN_SCREEN_LABEL`).

- [ ] **Step 4: Add strings** under a top-level `"options"` key in both JSON files:

```json
  "options": {
    "step": {
      "init": {
        "title": "Byonk server settings",
        "data": {
          "registration_screen": "New-device screen",
          "auth_mode": "Authentication mode",
          "package_refresh_interval": "Package refresh interval (seconds; 0 = off)"
        }
      }
    }
  }
```

- [ ] **Step 5: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_options_flow.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_options_flow.py
git commit -m "feat(ha): options flow for server settings"
```

---

### Task 8: Remove migrated select entities

**Files:**
- Modify: `custom_components/byonk/select.py`
- Modify: `custom_components/byonk/strings.json`, `custom_components/byonk/translations/en.json`
- Modify: `tests_ha/test_settings_entities.py`

**Interfaces:**
- Removes: `ByonkNewDeviceScreenSelect`, `ByonkAuthModeSelect` (settings now in the Options Flow, Task 7).

- [ ] **Step 1: Update the tests first** (they encode the removal). In `tests_ha/test_settings_entities.py`, **delete** `test_new_device_screen_select` and `test_new_device_screen_builtin`, and add:

```python
async def test_hub_has_no_settings_selects(hass, byonk):
    byonk.config = {"registration": {"enabled": True}, "auth_mode": "api_key"}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert hass.states.get("select.byonk_new_device_screen") is None
    assert hass.states.get("select.byonk_auth_mode") is None
```

Keep `test_registration_switch_turns_on` unchanged.

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_settings_entities.py -v`
Expected: FAIL (the selects still exist).

- [ ] **Step 3: Implement.** In `select.py`:
- Delete the classes `ByonkNewDeviceScreenSelect` and `ByonkAuthModeSelect`.
- In `async_setup_entry`, replace the hub branch (the final `async_add_entities([...])`) with nothing — the hub adds no selects now:

```python
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        async_add_entities(
            [
                ByonkScreenSelect(coordinator, key),
                ByonkDitherSelect(coordinator, key),
                ByonkPanelSelect(coordinator, key),
            ]
        )
        setup_param_platform(entry, async_add_entities, {"enum"}, ByonkParamSelect)
    # hub entry: settings live in the Options Flow, not entities
```

- Remove the now-unused `BUILTIN_SCREEN_LABEL` and `EntityCategory` imports **only if** no longer referenced in the file (they are not, after deletion).

- [ ] **Step 4: Remove select strings.** In `strings.json` and `translations/en.json`, delete the `new_device_screen` and `auth_mode` entries under `entity.select`.

- [ ] **Step 5: Run — verify pass + lint**

Run: `.venv/bin/pytest tests_ha/test_settings_entities.py -v && .venv/bin/ruff check custom_components/byonk`
Expected: PASS, no lint errors (catches unused imports).

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/select.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_settings_entities.py
git commit -m "refactor(ha): move new-device-screen + auth-mode into options flow"
```

---

### Task 9: Package status sensors (dynamic, hub device)

**Files:**
- Create: `custom_components/byonk/package_entities.py`
- Modify: `custom_components/byonk/sensor.py`
- Modify: `custom_components/byonk/strings.json`, `custom_components/byonk/translations/en.json`
- Test: `tests_ha/test_package_status.py`

**Interfaces:**
- Consumes: `ByonkData.non_builtin_packages` (Task 2); `ByonkHubEntity` (existing); the `ParamPlatformManager` reconcile idiom.
- Produces: `setup_package_status_platform(entry, async_add_entities)`; `ByonkPackageStatusSensor` (state = package `status`; attributes `resolved_sha`, `last_fetched`, `error`, `repo`, `pin`, `pin_kind`).

- [ ] **Step 1: Write failing test** `tests_ha/test_package_status.py`:

```python
from tests_ha.conftest import make_hub_entry

PKGS = [
    {"handle": "byonk-builtin", "builtin": True, "status": "ready"},
    {"handle": "weather", "builtin": False, "repo": "r", "pin": "main",
     "pin_kind": "branch", "resolved_sha": "abc123", "status": "ready",
     "last_fetched": "2026-07-04T00:00:00+00:00", "error": None},
]


async def test_status_sensor_reflects_package(hass, byonk):
    byonk.packages = PKGS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    state = hass.states.get("sensor.byonk_weather_status")
    assert state is not None
    assert state.state == "ready"
    assert state.attributes["resolved_sha"] == "abc123"
    # builtin has no status sensor
    assert hass.states.get("sensor.byonk_byonk_builtin_status") is None


async def test_status_sensor_added_and_removed(hass, byonk):
    byonk.packages = [PKGS[0]]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert hass.states.get("sensor.byonk_weather_status") is None
    byonk.packages = PKGS
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert hass.states.get("sensor.byonk_weather_status") is not None
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_status.py -v`
Expected: FAIL (no such sensor).

- [ ] **Step 3: Implement** `custom_components/byonk/package_entities.py`:

```python
"""Dynamic per-package status sensors on the hub device."""
from __future__ import annotations

from homeassistant.components.sensor import SensorEntity
from homeassistant.const import EntityCategory
from homeassistant.core import callback
from homeassistant.helpers import entity_registry as er
from homeassistant.util import dt as dt_util

from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkHubEntity


class ByonkPackageStatusSensor(ByonkHubEntity, SensorEntity):
    """One sensor per non-builtin package: state = fetch status."""

    _attr_entity_category = EntityCategory.DIAGNOSTIC
    _attr_translation_key = "package_status"

    def __init__(self, coordinator: ByonkCoordinator, handle: str) -> None:
        super().__init__(coordinator)
        self._handle = handle
        self._attr_unique_id = f"{coordinator.entry.entry_id}_pkg_{handle}_status"
        self._attr_translation_placeholders = {"handle": handle}
        self._attr_name = f"{handle}: status"

    @property
    def _pkg(self) -> dict | None:
        return self.coordinator.data.package(self._handle)

    @property
    def available(self) -> bool:
        return super().available and self._pkg is not None

    @property
    def native_value(self) -> str | None:
        pkg = self._pkg
        return pkg.get("status") if pkg else None

    @property
    def extra_state_attributes(self) -> dict:
        pkg = self._pkg or {}
        lf = pkg.get("last_fetched")
        return {
            "resolved_sha": pkg.get("resolved_sha"),
            "last_fetched": dt_util.parse_datetime(lf) if lf else None,
            "error": pkg.get("error"),
            "repo": pkg.get("repo"),
            "pin": pkg.get("pin"),
            "pin_kind": pkg.get("pin_kind"),
        }


class PackageStatusManager:
    """Add/remove a status sensor per non-builtin package as the registry changes."""

    def __init__(self, coordinator: ByonkCoordinator, async_add_entities) -> None:
        self._coordinator = coordinator
        self._async_add_entities = async_add_entities
        self._entities: dict[str, ByonkPackageStatusSensor] = {}

    @callback
    def reconcile(self) -> None:
        desired = {p["handle"] for p in self._coordinator.data.non_builtin_packages()}
        new = {
            h: ByonkPackageStatusSensor(self._coordinator, h)
            for h in desired
            if h not in self._entities
        }
        for h, ent in new.items():
            self._entities[h] = ent
        if new:
            self._async_add_entities(list(new.values()))
        for h in list(self._entities):
            if h not in desired:
                self._remove(self._entities.pop(h))

    def _remove(self, entity: ByonkPackageStatusSensor) -> None:
        registry = er.async_get(self._coordinator.hass)
        if entity.entity_id and registry.async_get(entity.entity_id):
            registry.async_remove(entity.entity_id)
        else:
            self._coordinator.hass.async_create_task(
                entity.async_remove(force_remove=True)
            )


def setup_package_status_platform(entry: ByonkConfigEntry, async_add_entities) -> None:
    coordinator = entry.runtime_data
    manager = PackageStatusManager(coordinator, async_add_entities)
    manager.reconcile()
    entry.async_on_unload(coordinator.async_add_listener(manager.reconcile))
```

In `sensor.py`, wire the hub branch — replace the hub comment line in `async_setup_entry`:

```python
    from .package_entities import setup_package_status_platform
    setup_package_status_platform(entry, async_add_entities)
```

(Place the import at the top of `sensor.py` instead of inline, matching file style.)

- [ ] **Step 4: Add the sensor name string** under `entity.sensor` in both JSON files (used only if translation_key resolves; the explicit `_attr_name` already covers display):

```json
      "package_status": { "name": "{handle}: status" }
```

- [ ] **Step 5: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_status.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/package_entities.py custom_components/byonk/sensor.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_package_status.py
git commit -m "feat(ha): per-package status sensors on the hub device"
```

---

### Task 10: "Update packages" button

**Files:**
- Modify: `custom_components/byonk/const.py`
- Create: `custom_components/byonk/button.py`
- Modify: `custom_components/byonk/strings.json`, `custom_components/byonk/translations/en.json`
- Test: `tests_ha/test_package_button.py`

**Interfaces:**
- Consumes: `ByonkClient.async_update_packages` (Task 1); `ByonkHubEntity`.
- Produces: `Platform.BUTTON` in `PLATFORMS`; `ByonkUpdatePackagesButton` on the hub device.

- [ ] **Step 1: Write failing test** `tests_ha/test_package_button.py`:

```python
from tests_ha.conftest import make_hub_entry


async def test_update_packages_button(hass, byonk):
    byonk.packages = [{"handle": "weather", "builtin": False, "status": "ready"}]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    ent = "button.byonk_update_packages"
    assert hass.states.get(ent) is not None
    await hass.services.async_call(
        "button", "press", {"entity_id": ent}, blocking=True
    )
    assert byonk.update_packages.await_count == 1
```

- [ ] **Step 2: Run — verify fail**

Run: `.venv/bin/pytest tests_ha/test_package_button.py -v`
Expected: FAIL (no button platform/entity).

- [ ] **Step 3: Implement.** In `const.py`, add `Platform.BUTTON` to `PLATFORMS`:

```python
PLATFORMS: list[Platform] = [
    Platform.BUTTON,
    Platform.SENSOR,
    Platform.SELECT,
    Platform.SWITCH,
    Platform.TEXT,
]
```

Create `custom_components/byonk/button.py`:

```python
"""Byonk buttons (hub actions)."""
from __future__ import annotations

import logging

from homeassistant.components.button import ButtonEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry
from .entity import ByonkHubEntity

_LOGGER = logging.getLogger(__name__)


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        return  # device entries have no hub buttons
    async_add_entities([ByonkUpdatePackagesButton(entry.runtime_data)])


class ByonkUpdatePackagesButton(ByonkHubEntity, ButtonEntity):
    _attr_translation_key = "update_packages"

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_update_packages"

    async def async_press(self) -> None:
        try:
            await self.coordinator.client.async_update_packages()
        except ByonkApiError as err:
            _LOGGER.warning("update packages failed: %s", err)
            return
        await self.coordinator.async_request_refresh()
```

- [ ] **Step 4: Add the button string** under `entity.button` in both JSON files:

```json
    "button": {
      "update_packages": { "name": "Update packages" }
    }
```

- [ ] **Step 5: Run — verify pass**

Run: `.venv/bin/pytest tests_ha/test_package_button.py -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/const.py custom_components/byonk/button.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_package_button.py
git commit -m "feat(ha): update-packages button on the hub device"
```

---

### Task 11: Docs, full check, and live VM verification

**Files:**
- Modify: `CHANGES.md`
- Modify: `docs/src/guide/ha-addon.md` (or the integration doc page)
- No test file (manual + full suite).

- [ ] **Step 1: Document** the feature. In `CHANGES.md` (Unreleased), add:

```markdown
- Home Assistant: manage screen packages from the UI — add/edit/remove packages
  as native config subentries, server settings (new-device screen, auth mode,
  package refresh interval) in the Configure dialog, per-package status sensors,
  and an "Update packages" button. The new-device-screen and auth-mode select
  entities are replaced by the Configure dialog.
```

Add a short "Managing screen packages" section to the HA docs page describing: Add package (handle/repo/pin/token), that the token is write-only, the status sensor, the update button, and the delete caveat: **a package still referenced by a device cannot be deleted — reassign the device first, or the entry reappears.**

- [ ] **Step 2: Full HA check**

Run: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`
Expected: PASS, no lint errors.

- [ ] **Step 3: Docs build**

Run: `make docs`
Expected: builds clean.

- [ ] **Step 4: Deploy to the VM and reload** (integration-only change — no add-on rebuild needed; the Plan-2 server binary is already live):

```bash
SMB_USER=byonk SMB_PASS=byonk make ha-deploy
make ha-ssh CMD="ha core restart"
```

- [ ] **Step 5: Manual verification in the HA UI** (Settings → Devices & Services → Byonk):
  - **Add package** → handle `disttest`, repo `https://github.com/oetiker/byonk-dist-test.git`, pin `main`. A `disttest` subentry appears; `sensor.byonk_disttest_status` goes `fetching → ready` with a `resolved_sha`.
  - Assign a device to `disttest/hello`; confirm it renders (reuse the Plan-2 device or the display check).
  - **Edit package** → pin `→` the v2 sha or leave `main`; press **Update packages**; the status sensor's `resolved_sha`/`last_fetched` update.
  - **Configure** (⚙) → change new-device screen / auth mode / refresh interval; confirm via `GET /config` (token server-side; never printed — see the `ha-vm-admin-api-testing` memory).
  - **Delete** the `disttest` subentry while a device still references its screen → it **reappears** (byonk 409 self-heal) and a warning is logged; reassign the device, delete again → it stays gone and byonk's `GET /packages` no longer lists it.
  - Confirm the old `select.byonk_new_device_screen` / `select.byonk_auth_mode` entities are gone.

- [ ] **Step 6: Commit docs**

```bash
git add CHANGES.md docs/src/guide/ha-addon.md
git commit -m "docs(ha): document HA package management"
```

---

## Self-review notes

- **Spec coverage:** §4.2 add/reconfigure + write-only token → Tasks 3, 4; §4.3 delete propagation + 409 self-heal → Task 6; §4.4 reconcile → Task 5; §5 options flow → Task 7 (+ removal Task 8); §6 status sensors + update button → Tasks 9, 10; §8 client methods → Task 1; §9 coordinator → Tasks 2, 5; §11 testing + live check → per-task tests + Task 11.
- **HA version (spec §10):** flow-based subentries are proven in-repo (commit range `80ea75e..b9f89df`), so the test harness supports them. **During execution, confirm `hass.config_entries.async_add_subentry` / `async_update_subentry` / `async_remove_subentry` exist in the target HA** (programmatic subentry management is newer than the flow API). If absent, Task 5 must create subentries via the flow/`async_update_entry` path instead — verify at Task 5 Step 1.
- **`default_screen`** intentionally not surfaced (spec §2/§13).
- **Race guard:** the add flow (Task 3) deliberately does not refresh, so reconcile (Task 5) never double-creates a subentry; the delete listener (Task 6) diffs a snapshot so reconcile-driven adds don't trigger spurious deletes.
