# Screen Params as Live Device-Page Entities — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edit a TRMNL's screen `@params` directly on the Home Assistant device page as live, instant-apply entities (in the "Controls"/Steuerung card), replacing the post-onboarding Reconfigure dialog.

**Architecture:** A shared `param_entities.py` provides a `ByonkParamEntity` base, four typed subclasses (Text/Number/Select/Switch), and a `ParamPlatformManager` that adds/removes param entities as the device's current screen changes. Each of the four HA platforms (text/number/select/switch) wires its type slice. Writes read-modify-write the device's full `params` dict under a per-device lock (byonk's `params` PATCH is a full replacement).

**Tech Stack:** Home Assistant custom component (Python), pytest-homeassistant-custom-component.

## Global Constraints

- Never `git add -A`/`git add .` — stage explicit paths and verify `git diff --cached` before committing.
- HA tests + lint must pass before each commit:
  - `.venv/bin/python -m pytest tests_ha -q`
  - `.venv/bin/ruff check custom_components/byonk tests_ha`
- HA Python ≤ 3.13; the `.venv` is already set up.
- byonk is NOT changed by this plan (its `params` PATCH stays a full replacement; validation unchanged).
- Param entities carry **no `entity_category`** (they must render in the device's "Controls"/Steuerung card).
- `unique_id` for a param entity is `f"{key}_param_{field_name}"`.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

### Task 1: Param-entity framework + Text platform

**Files:**
- Modify: `custom_components/byonk/coordinator.py` (add `param_lock`)
- Create: `custom_components/byonk/param_entities.py` (base + `ByonkParamText` + manager)
- Create: `custom_components/byonk/text.py`
- Modify: `custom_components/byonk/const.py` (add `Platform.TEXT`)
- Test: `tests_ha/test_param_entities.py`

**Interfaces:**
- Consumes: `ByonkDeviceEntity` (`.device`, `.coordinator`, `._key`), `ByonkCoordinator` (`.client.async_update_device(key, payload)`, `.async_refresh()`, `.data.screen_params(name)`, `.data.devices`, `.hass`, `.async_add_listener(cb)`), `CONF_DEVICE_KEY`.
- Produces: `ByonkParamEntity`, `ByonkParamText`, `setup_param_platform(entry, async_add_entities, types: set[str], entity_cls) -> None`, and `ByonkCoordinator.param_lock(key: str) -> asyncio.Lock`.

- [ ] **Step 1: Add the per-device lock to the coordinator**

In `custom_components/byonk/coordinator.py`, in `ByonkCoordinator.__init__`, after `self._orphan_strikes: dict[str, int] = {}` add:

```python
        self._param_locks: dict[str, asyncio.Lock] = {}
```

Then add this method to `ByonkCoordinator` (e.g. right after `__init__`):

```python
    def param_lock(self, key: str) -> asyncio.Lock:
        """Per-device lock serialising read-modify-write of the params dict."""
        return self._param_locks.setdefault(key, asyncio.Lock())
```

(`asyncio` is already imported at the top of the file.)

- [ ] **Step 2: Write the failing test**

Create `tests_ha/test_param_entities.py`:

```python
from tests_ha.conftest import make_device_entry, make_hub_entry

SCREENS = {
    "screens": [
        {"name": "transit", "params": [{"name": "station", "type": "string"}], "schema_error": None},
        {"name": "floerli", "params": [{"name": "room", "type": "string"}], "schema_error": None},
        {"name": "calibrator", "params": [], "schema_error": None},
    ],
    "panels": [{"name": "trmnl_og"}],
    "dither_algorithms": ["atkinson"],
}


async def _setup(hass, byonk, screen, params):
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": screen, "params": params}]
    byonk.screens = SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_string_param_is_text_entity(hass, byonk):
    await _setup(hass, byonk, "transit", {"station": "Olten"})
    st = hass.states.get("text.trmnl_aa_bb_station")
    assert st is not None
    assert st.state == "Olten"


async def test_no_param_entities_for_paramless_screen(hass, byonk):
    await _setup(hass, byonk, "calibrator", {})
    assert hass.states.get("text.trmnl_aa_bb_station") is None


async def test_text_param_write_sends_full_params(hass, byonk):
    await _setup(hass, byonk, "transit", {"station": "Olten"})
    await hass.services.async_call(
        "text", "set_value",
        {"entity_id": "text.trmnl_aa_bb_station", "value": "Bern"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"params": {"station": "Bern"}}


async def test_param_entities_reconcile_on_screen_change(hass, byonk):
    hub = await _setup(hass, byonk, "transit", {"station": "Olten"})
    assert hass.states.get("text.trmnl_aa_bb_station") is not None
    # device switched to floerli
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": "floerli", "params": {"room": "Kitchen"}}]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert hass.states.get("text.trmnl_aa_bb_station") is None
    assert hass.states.get("text.trmnl_aa_bb_room") is not None
    assert hass.states.get("text.trmnl_aa_bb_room").state == "Kitchen"
```

- [ ] **Step 3: Run it to verify it fails**

Run: `.venv/bin/python -m pytest tests_ha/test_param_entities.py -q`
Expected: FAIL — no `text.*` entities (Text platform/module do not exist yet).

- [ ] **Step 4: Create `custom_components/byonk/param_entities.py`**

```python
"""Dynamic per-screen parameter entities for byonk devices."""
from __future__ import annotations

import logging

from homeassistant.components.text import TextEntity, TextMode
from homeassistant.core import callback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity

_LOGGER = logging.getLogger(__name__)


class ByonkParamEntity(ByonkDeviceEntity):
    """Base for entities editing one screen @param of a device."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: ByonkCoordinator, key: str, field: dict) -> None:
        super().__init__(coordinator, key)
        self._field = field
        self._attr_unique_id = f"{key}_param_{field['name']}"
        self._attr_name = field.get("label") or field["name"]

    @property
    def _current_params(self) -> dict:
        device = self.device
        return dict(device.get("params") or {}) if device else {}

    @property
    def _value(self):
        return self._current_params.get(self._field["name"])

    @property
    def available(self) -> bool:
        if not super().available:
            return False
        device = self.device
        if not device:
            return False
        fields = self.coordinator.data.screen_params(device.get("screen"))
        return any(f["name"] == self._field["name"] for f in fields)

    async def _write_param(self, value) -> None:
        async with self.coordinator.param_lock(self._key):
            device = self.device
            params = dict(device.get("params") or {}) if device else {}
            params[self._field["name"]] = value
            try:
                await self.coordinator.client.async_update_device(
                    self._key, {"params": params}
                )
            except ByonkApiError as err:
                _LOGGER.warning(
                    "param write failed for %s.%s: %s",
                    self._key, self._field["name"], err,
                )
                return
            await self.coordinator.async_refresh()


class ByonkParamText(ByonkParamEntity, TextEntity):
    def __init__(self, coordinator: ByonkCoordinator, key: str, field: dict) -> None:
        super().__init__(coordinator, key, field)
        self._attr_mode = (
            TextMode.PASSWORD if field.get("sensitive") else TextMode.TEXT
        )

    @property
    def native_value(self) -> str | None:
        v = self._value
        return None if v is None else str(v)

    async def async_set_value(self, value: str) -> None:
        await self._write_param(value)


class ParamPlatformManager:
    """Add/remove param entities of given types as the device's screen changes."""

    def __init__(self, coordinator, key, async_add_entities, types, entity_cls):
        self._coordinator = coordinator
        self._key = key
        self._async_add_entities = async_add_entities
        self._types = types
        self._entity_cls = entity_cls
        self._entities: dict[str, ByonkParamEntity] = {}

    def _device_screen(self) -> str | None:
        for d in self._coordinator.data.devices:
            if d.get("key") == self._key:
                return d.get("screen")
        return None

    @callback
    def reconcile(self) -> None:
        screen = self._device_screen()
        fields = self._coordinator.data.screen_params(screen) if screen else []
        desired = {
            f["name"]: f
            for f in fields
            if f.get("type") in self._types and not f.get("hidden")
        }
        new = {
            name: self._entity_cls(self._coordinator, self._key, field)
            for name, field in desired.items()
            if name not in self._entities
        }
        for name, entity in new.items():
            self._entities[name] = entity
        if new:
            self._async_add_entities(list(new.values()))
        for name in list(self._entities):
            if name not in desired:
                entity = self._entities.pop(name)
                self._coordinator.hass.async_create_task(entity.async_remove())


def setup_param_platform(
    entry: ByonkConfigEntry, async_add_entities, types: set[str], entity_cls
) -> None:
    """Wire a platform's param entities for a device entry (dynamic per screen)."""
    coordinator = entry.runtime_data
    key = entry.data[CONF_DEVICE_KEY]
    manager = ParamPlatformManager(
        coordinator, key, async_add_entities, types, entity_cls
    )
    manager.reconcile()
    entry.async_on_unload(coordinator.async_add_listener(manager.reconcile))
```

- [ ] **Step 5: Create `custom_components/byonk/text.py`**

```python
"""Byonk text entities (string/color/url screen params)."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry
from .param_entities import ByonkParamText, setup_param_platform


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        setup_param_platform(
            entry, async_add_entities, {"string", "color", "url"}, ByonkParamText
        )
```

- [ ] **Step 6: Register the Text platform in `const.py`**

In `custom_components/byonk/const.py`, add `Platform.TEXT` to the `PLATFORMS` list (append it after `Platform.NUMBER`):

```python
PLATFORMS: list[Platform] = [
    Platform.SENSOR,
    Platform.SELECT,
    Platform.SWITCH,
    Platform.NUMBER,
    Platform.TEXT,
]
```

- [ ] **Step 7: Run the tests**

Run: `.venv/bin/python -m pytest tests_ha/test_param_entities.py -q`
Expected: PASS (4 tests).
Then: `.venv/bin/python -m pytest tests_ha -q && .venv/bin/ruff check custom_components/byonk tests_ha`
Expected: all pass, ruff clean.

- [ ] **Step 8: Commit**

```bash
git add custom_components/byonk/coordinator.py custom_components/byonk/param_entities.py custom_components/byonk/text.py custom_components/byonk/const.py tests_ha/test_param_entities.py
git commit -m "feat(ha): screen string params as live Text entities + reconcile framework

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Number, Select, and Switch param entities

**Files:**
- Modify: `custom_components/byonk/param_entities.py` (add 3 classes)
- Modify: `custom_components/byonk/number.py` (wire param numbers)
- Modify: `custom_components/byonk/select.py` (wire param selects)
- Modify: `custom_components/byonk/switch.py` (wire param switches)
- Test: `tests_ha/test_param_entities.py` (extend)

**Interfaces:**
- Consumes: `ByonkParamEntity`, `setup_param_platform` (Task 1).
- Produces: `ByonkParamNumber`, `ByonkParamSelect`, `ByonkParamSwitch`.

- [ ] **Step 1: Write the failing tests**

Append to `tests_ha/test_param_entities.py`:

```python
NUM_SCREENS = {
    "screens": [
        {"name": "transit", "params": [
            {"name": "station", "type": "string"},
            {"name": "limit", "type": "int", "min": 1, "max": 30},
        ], "schema_error": None},
        {"name": "gphoto", "params": [
            {"name": "show_status", "type": "bool"},
            {"name": "theme", "type": "enum", "options": [
                {"value": "light", "label": "Light"}, {"value": "dark", "label": "Dark"}]},
        ], "schema_error": None},
    ],
    "panels": [{"name": "trmnl_og"}],
    "dither_algorithms": ["atkinson"],
}


async def _setup_num(hass, byonk, screen, params):
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": screen, "params": params}]
    byonk.screens = NUM_SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_int_param_number_coerces_to_int(hass, byonk):
    await _setup_num(hass, byonk, "transit", {"station": "Olten", "limit": 8})
    st = hass.states.get("number.trmnl_aa_bb_limit")
    assert st is not None
    assert float(st.state) == 8.0
    await hass.services.async_call(
        "number", "set_value",
        {"entity_id": "number.trmnl_aa_bb_limit", "value": 12}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert payload["params"]["limit"] == 12
    assert isinstance(payload["params"]["limit"], int)
    # other params preserved
    assert payload["params"]["station"] == "Olten"


async def test_bool_param_is_switch(hass, byonk):
    await _setup_num(hass, byonk, "gphoto", {"show_status": False, "theme": "light"})
    st = hass.states.get("switch.trmnl_aa_bb_show_status")
    assert st is not None
    assert st.state == "off"
    await hass.services.async_call(
        "switch", "turn_on",
        {"entity_id": "switch.trmnl_aa_bb_show_status"}, blocking=True,
    )
    _key, payload = byonk.update_device.await_args.args
    assert payload["params"]["show_status"] is True


async def test_enum_param_select_includes_current_value(hass, byonk):
    # stored value not among declared options -> still shown, not "unknown"
    await _setup_num(hass, byonk, "gphoto", {"show_status": True, "theme": "sepia"})
    st = hass.states.get("select.trmnl_aa_bb_theme")
    assert st is not None
    assert st.state == "sepia"
    assert "light" in st.attributes["options"]
    assert "sepia" in st.attributes["options"]
```

- [ ] **Step 2: Run to verify failure**

Run: `.venv/bin/python -m pytest tests_ha/test_param_entities.py -q`
Expected: FAIL — `number.*`/`switch.*`/`select.*_theme` param entities don't exist yet.

- [ ] **Step 3: Add the three entity classes to `param_entities.py`**

Add these imports at the top of `custom_components/byonk/param_entities.py`:

```python
from homeassistant.components.number import NumberEntity, NumberMode
from homeassistant.components.select import SelectEntity
from homeassistant.components.switch import SwitchEntity
```

Add these classes after `ByonkParamText`:

```python
class ByonkParamNumber(ByonkParamEntity, NumberEntity):
    _attr_mode = NumberMode.BOX

    def __init__(self, coordinator: ByonkCoordinator, key: str, field: dict) -> None:
        super().__init__(coordinator, key, field)
        if field.get("min") is not None:
            self._attr_native_min_value = field["min"]
        if field.get("max") is not None:
            self._attr_native_max_value = field["max"]
        self._attr_native_step = (
            1.0 if field.get("type") == "int" else (field.get("step") or 0.01)
        )
        if field.get("unit"):
            self._attr_native_unit_of_measurement = field["unit"]

    @property
    def native_value(self) -> float | None:
        v = self._value
        return None if v is None else float(v)

    async def async_set_native_value(self, value: float) -> None:
        coerced = int(value) if self._field.get("type") == "int" else value
        await self._write_param(coerced)


class ByonkParamSelect(ByonkParamEntity, SelectEntity):
    @property
    def options(self) -> list[str]:
        opts = [o["value"] for o in self._field.get("options", [])]
        current = self._value
        if current is not None and current not in opts:
            return [*opts, current]
        return opts

    @property
    def current_option(self) -> str | None:
        v = self._value
        return None if v is None else str(v)

    async def async_select_option(self, option: str) -> None:
        await self._write_param(option)


class ByonkParamSwitch(ByonkParamEntity, SwitchEntity):
    @property
    def is_on(self) -> bool:
        return bool(self._value)

    async def async_turn_on(self, **kwargs) -> None:
        await self._write_param(True)

    async def async_turn_off(self, **kwargs) -> None:
        await self._write_param(False)
```

- [ ] **Step 4: Wire the Number platform**

In `custom_components/byonk/number.py`, add the import:

```python
from .param_entities import ByonkParamNumber, setup_param_platform
```

Change `async_setup_entry` so device entries also get param numbers:

```python
async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        async_add_entities([ByonkRefreshNumber(coordinator, entry.data[CONF_DEVICE_KEY])])
        setup_param_platform(
            entry, async_add_entities, {"int", "float"}, ByonkParamNumber
        )
```

- [ ] **Step 5: Wire the Select platform**

In `custom_components/byonk/select.py`, add the import:

```python
from .param_entities import ByonkParamSelect, setup_param_platform
```

In `async_setup_entry`, inside the `if CONF_DEVICE_KEY in entry.data:` branch, after the `async_add_entities([...ByonkScreenSelect...])` call and before `return`, add:

```python
        setup_param_platform(entry, async_add_entities, {"enum"}, ByonkParamSelect)
        return
```

(Replace the existing bare `return` at the end of that branch with the two lines above.)

- [ ] **Step 6: Wire the Switch platform**

In `custom_components/byonk/switch.py`, add the import:

```python
from .param_entities import ByonkParamSwitch, setup_param_platform
```

Change `async_setup_entry` so device entries get param switches instead of early-returning:

```python
async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        setup_param_platform(entry, async_add_entities, {"bool"}, ByonkParamSwitch)
        return
    async_add_entities([ByonkRegistrationSwitch(entry.runtime_data)])
```

- [ ] **Step 7: Run the tests**

Run: `.venv/bin/python -m pytest tests_ha/test_param_entities.py -q`
Expected: PASS (all, including the 3 new).
Then: `.venv/bin/python -m pytest tests_ha -q && .venv/bin/ruff check custom_components/byonk tests_ha`
Expected: all pass, ruff clean.

- [ ] **Step 8: Commit**

```bash
git add custom_components/byonk/param_entities.py custom_components/byonk/number.py custom_components/byonk/select.py custom_components/byonk/switch.py tests_ha/test_param_entities.py
git commit -m "feat(ha): screen int/float/enum/bool params as Number/Select/Switch entities

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Remove the Reconfigure flow

**Files:**
- Modify: `custom_components/byonk/config_flow.py` (delete `async_step_reconfigure`)
- Modify: `custom_components/byonk/strings.json` (remove dead keys)
- Modify: `custom_components/byonk/translations/en.json` (remove dead keys)
- Modify: `tests_ha/test_config_flow.py` (remove reconfigure tests)

**Interfaces:** none new. Onboarding's `async_step_dev_params` (and its `build_params_schema` + `coerce_params` use) is unchanged.

- [ ] **Step 1: Delete `async_step_reconfigure`**

In `custom_components/byonk/config_flow.py`, delete the entire `async def async_step_reconfigure(self, ...)` method (from its `async def` line through its final `return self.async_show_form(step_id="reconfigure", ...)` block). Leave `async_step_dev_params` and all other steps intact.

- [ ] **Step 2: Remove dead strings**

In both `custom_components/byonk/strings.json` and `custom_components/byonk/translations/en.json`, under `config`, remove these now-unused keys:
- `config.step.reconfigure`
- `config.abort.reconfigure_successful`
- `config.abort.not_supported`
- `config.abort.update_failed`
- `config.error.update_failed`

Leave `config.step.configure`, `config.step.dev_params`, `config.error.add_failed`, `config.abort.add_failed`, and all other keys.

- [ ] **Step 3: Remove reconfigure tests**

In `tests_ha/test_config_flow.py`, delete any test exercising the reconfigure flow (search for `reconfigure` / `async_step_reconfigure` / `"reconfigure"` and remove those test functions). Do not touch onboarding/discovery tests.

- [ ] **Step 4: Run tests + lint + validate JSON**

Run: `.venv/bin/python -m pytest tests_ha -q`
Expected: PASS (no reconfigure tests remain; nothing else broke).
Run: `.venv/bin/ruff check custom_components/byonk tests_ha`
Expected: clean.
Run: `.venv/bin/python -c "import json; json.load(open('custom_components/byonk/strings.json')); json.load(open('custom_components/byonk/translations/en.json')); print('json ok')"`
Expected: `json ok`.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_config_flow.py
git commit -m "refactor(ha): remove Reconfigure flow (params now live device-page entities)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Documentation + CHANGES

**Files:**
- Modify: `CHANGES.md`
- Modify: `docs/src/guide/ha-integration.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Update CHANGES.md**

In `## Unreleased` → `### New`, add:

```markdown
- **Home Assistant screen parameters are now live device-page entities**: each
  parameter of a device's current screen appears as its own control in the
  device's Controls card (Text / Number / Switch / Select by type) and applies
  instantly. The controls update automatically when you change the device's
  screen. This replaces the per-device Reconfigure dialog.
```

In `### Changed` (or `### Removed` if present under Unreleased), add:

```markdown
- The Home Assistant device **Reconfigure** dialog has been removed; screen
  parameters are edited via the live device-page entities instead. (Onboarding
  still prompts for a screen's parameters.)
```

- [ ] **Step 2: Update the HA integration guide**

In `docs/src/guide/ha-integration.md`, update the per-device entity documentation:
- In the section listing per-device entities, add that a device also exposes **one entity per parameter of its current screen** (string→Text, int/float→Number, bool→Switch, enum→Select), shown in the Controls card and applied instantly; the set changes automatically when the screen changes.
- If the page documents editing parameters via "Reconfigure", replace that with the live-entity description.

Read the file first to find the right spot; keep edits to existing prose, do not restructure the page.

- [ ] **Step 3: Build the docs**

Run: `make docs`
Expected: mdBook build succeeds.

- [ ] **Step 4: Commit**

```bash
git add CHANGES.md docs/src/guide/ha-integration.md
git commit -m "docs: screen params as live device-page entities

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- 4-platform layout + shared `param_entities.py` (base + manager + 4 classes) → Tasks 1 (base+manager+Text) & 2 (Number/Select/Switch) ✓
- Coordinator `param_lock` → Task 1 ✓
- Read-modify-write full params under lock + immediate refresh → `_write_param` (Task 1) ✓
- No `entity_category` (Controls card) → base sets none ✓
- Type mapping incl. int coercion, enum include-current, sensitive→password → Task 2 (+ Text mode in Task 1) ✓
- Dynamic add/remove reconcile on screen change → `ParamPlatformManager` (Task 1), tested ✓
- Remove Reconfigure, keep onboarding `dev_params` → Task 3 ✓
- Skip `hidden` → reconcile filter `not f.get("hidden")` (Task 1) ✓
- Tests (unit-ish coercion/options + integration reconcile/write) → Tasks 1–2 ✓
- Docs/CHANGES → Task 4 ✓
- Out of scope (no byonk change, multiline single-line, refresh Number vs refresh_rate param coexist) → respected; multiline handled by Text default (no special case) ✓

**Placeholder scan:** no TBD/TODO; every code step has real code; the only prose-only step is the docs guide edit (Task 4 Step 2), inherently descriptive.

**Type consistency:** `setup_param_platform(entry, async_add_entities, types, entity_cls)` used identically in text/number/select/switch. `ByonkParamEntity`/`ByonkParamText`/`ByonkParamNumber`/`ByonkParamSelect`/`ByonkParamSwitch`, `param_lock(key)`, `_write_param(value)`, `_value` consistent across tasks. Entity domains match the platform each is wired into (Text→text.py, Number→number.py, Select→select.py, Switch→switch.py). `unique_id = f"{key}_param_{name}"` matches the global constraint.
