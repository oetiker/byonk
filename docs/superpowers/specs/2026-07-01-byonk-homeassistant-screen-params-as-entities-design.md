# Screen parameters as live device-page entities (Home Assistant)

_Status: design approved 2026-07-01. Follow-on to the Phase 6 device-page work.
Goal: edit a TRMNL's screen `@params` directly from the device page as live,
instant-apply entities (in the "Controls"/Steuerung card), replacing the
post-onboarding Reconfigure dialog._

## Background

byonk screens can declare typed parameters via an `@params` header (parsed into
`ParamField`s, exposed by `GET /api/admin/screens`). Today the only way to edit a
configured device's params is the integration's **Reconfigure** flow, which:

- is hidden behind the device's ⋮ menu, and
- shows nothing for a device whose current screen declares no params (e.g.
  `calibrator`), so the entry point is non-obvious.

This feature surfaces each parameter of the device's **current screen** as its
own Home Assistant entity on the device page, alongside the screen/dither/panel
selects and the refresh Number.

## Decisions (approved)

- **Dynamic add/remove**: only the current screen's params are shown; switching a
  device's screen removes the old param entities and adds the new screen's.
- **Replace Reconfigure**: the post-onboarding Reconfigure param form is removed.
  Onboarding's `dev_params` step stays (it still seeds params at add time).
- **All common types, skip `hidden`**: string/color/url→Text, int/float→Number,
  enum→Select, bool→Switch.
- **byonk unchanged**: `params` PATCH stays a full replacement; the HA side does a
  read-modify-write under a per-device lock.

## `@params` field shape (from `GET /api/admin/screens`)

Each screen's `params` is a list of field descriptors (serialized `ParamField`):

```
name: str            # param key
type: "string"|"int"|"float"|"bool"|"enum"|"color"|"url"
required: bool
default: any?        # present only if declared
label: str?          # human label (fallback: name)
description: str?
min, max, step: number?
unit: str?
mode: str?
options: [{value, label}]   # for enum
sensitive, multiline, hidden, advanced: bool
```

## Architecture

HA entities are per-domain and a param's `type` selects its domain, so param
entities span **four platforms**. A shared module owns the common logic; each
platform module wires its type slice.

### New file: `custom_components/byonk/param_entities.py`

- **`ByonkParamEntity(ByonkDeviceEntity)`** — base for all param entities.
  - `__init__(coordinator, key, field)` stores the field descriptor.
  - `unique_id = f"{key}_param_{field['name']}"`.
  - `_attr_has_entity_name = True`; `_attr_name = field.get("label") or field["name"]`.
  - **No `entity_category`** → renders in the device's "Controls" (Steuerung) card.
  - `_current_params` property → `dict(self.device.get("params") or {})`.
  - `_value` property → `self._current_params.get(field["name"])`.
  - `available` → base availability **and** the field is still in the current
    screen's params (guards the brief window before reconcile runs).
  - `async _write_param(value)`:
    ```python
    async with self.coordinator.param_lock(self._key):
        params = dict(self.device.get("params") or {})
        params[self._field["name"]] = value
        await self.coordinator.client.async_update_device(
            self._key, {"params": params}
        )
        await self.coordinator.async_refresh()
    ```
    Full-dict send matches byonk's full-replace; the lock + immediate refresh make
    rapid multi-field edits safe (the next writer reads the prior result).
  - Byonk validation errors (`ByonkApiError`) are caught and logged at WARNING; the
    entity keeps its prior value (the coordinator refresh re-reads truth).

- **Concrete classes** (each sets the HA domain mixin + type-specific read/write):
  - `ByonkParamText(ByonkParamEntity, TextEntity)` — string/color/url.
    `native_value = _value`; `async_set_value(v) -> _write_param(v)`;
    `mode = PASSWORD if field.sensitive else TEXT`; multiline ignored by Text (HA
    Text has no multiline — multiline strings still edit as single-line text).
  - `ByonkParamNumber(ByonkParamEntity, NumberEntity)` — int/float.
    `native_value = float(_value) if _value is not None else None`;
    `native_min/max/step` from field (`step` defaults to 1 for int, "any" for
    float); `native_unit_of_measurement = field.unit`;
    `async_set_native_value(v)`: coerce `int(v)` when `type == "int"`, else `v`,
    then `_write_param`.
  - `ByonkParamSelect(ByonkParamEntity, SelectEntity)` — enum.
    `options = [o["value"] for o in field.options]` **plus the current value if
    absent** (same guard as the panel fix, so it never shows "unknown");
    `current_option = _value`; `async_select_option(o) -> _write_param(o)`.
  - `ByonkParamSwitch(ByonkParamEntity, SwitchEntity)` — bool.
    `is_on = bool(_value)`; `async_turn_on/off -> _write_param(True/False)`.

- **`setup_param_platform(entry, async_add_entities, types, entity_cls)`** — the
  shared reconcile manager (one per platform per device entry):
  - Captures `async_add_entities`, the device `key`, the `types` set, `entity_cls`,
    and a `dict[str, ByonkParamEntity]` of created entities by param name.
  - `@callback _reconcile()`:
    - `screen = device_row["screen"]`; `fields = coordinator.data.screen_params(screen)`;
      keep fields whose `type` ∈ `types` and not `hidden`.
    - **add**: for `name in desired - existing`, build `entity_cls(coordinator, key, field)`,
      `async_add_entities([e])`, track it.
    - **remove**: for `name in existing - desired`,
      `hass.async_create_task(e.async_remove())`, untrack it.
  - Runs once at setup, then `entry.async_on_unload(coordinator.async_add_listener(_reconcile))`.

### Coordinator change (`coordinator.py`)

- Add a per-device write lock accessor:
  ```python
  def param_lock(self, key: str) -> asyncio.Lock:
      return self._param_locks.setdefault(key, asyncio.Lock())
  ```
  (`self._param_locks: dict[str, asyncio.Lock] = {}` in `__init__`.)
- `screen_params(name)` already exists on `ByonkData`.

### Platform wiring

- **`const.py`**: add `Platform.TEXT` to `PLATFORMS`.
- **New `text.py`**: `async_setup_entry` → for device entries,
  `setup_param_platform(entry, async_add_entities, {"string","color","url"}, ByonkParamText)`.
- **`number.py`**: keep `ByonkRefreshNumber`; additionally call
  `setup_param_platform(..., {"int","float"}, ByonkParamNumber)` for device entries.
- **`select.py`**: keep the device/hub selects; additionally call
  `setup_param_platform(..., {"enum"}, ByonkParamSelect)` for device entries.
- **`switch.py`**: it currently early-returns for device entries; instead, for
  device entries call `setup_param_platform(..., {"bool"}, ByonkParamSwitch)`; the
  hub branch keeps `ByonkRegistrationSwitch`.

### Config-flow change (`config_flow.py`)

- **Remove `async_step_reconfigure`** entirely (it only edited device params).
- Keep `async_step_dev_params` (onboarding) and its `build_params_schema` +
  `coerce_params` usage.
- Remove now-dead strings from `strings.json` + `translations/en.json`:
  `config.step.reconfigure`, `config.abort.reconfigure_successful`,
  `config.abort.not_supported`, `config.abort.update_failed`,
  `config.error.update_failed`.

## Data flow (edit a param)

```
user edits "limit" Number on the device page
  -> ByonkParamNumber.async_set_native_value(8.0)
  -> int coercion -> 8
  -> _write_param(8): lock -> read coordinator params {station:"Olten",limit:5}
       -> PATCH /api/admin/devices/<key> {"params":{"station":"Olten","limit":8}}
       -> coordinator.async_refresh()
  -> byonk validate_params OK -> config.yaml updated -> next poll reflects limit=8
```

Switching the screen (existing Screen select) sends the new screen's
`default_params`, byonk replaces params, the coordinator refresh fires, and each
platform's `_reconcile` swaps the param entity set.

## Error handling

- `ByonkApiError` on write → logged WARNING, value reverts on refresh (no crash).
- Param not in current screen (race) → entity `available = False` until reconcile
  removes it.
- Screen with no params (`calibrator`) → no param entities (correct).

## Testing

**Unit (`tests_ha/test_param_entities.py`)**
- `ByonkParamNumber` coerces an int field's `8.0` → `8` in the PATCH payload.
- `_write_param` sends the **full** params dict with exactly one key changed.
- `ByonkParamSelect.options` includes the current value when it is not among the
  field's declared options.

**Integration (`tests_ha`)**
- A device on `transit` exposes `text.*_station` and `number.*_limit`; no param
  entities for a device on `calibrator`.
- Changing the device row's `screen` from `transit` to `floerli` (and the screens
  fixture) reconciles: `station`/`limit` entities removed, `room`/`test_timestamp`
  added.
- Setting `number.*_limit` calls `async_update_device(key, {"params": {…incl
  limit…}})` with the other params preserved.
- Remove/replace the existing Reconfigure flow tests (`test_config_flow` /
  `test_reconfigure`) since the step is gone.

## Out of scope

- No byonk changes (params PATCH stays full-replace; validation unchanged).
- No change to onboarding's `dev_params` form.
- `multiline` params do not get a multiline editor (HA Text is single-line);
  revisit only if a real multiline param appears.
- The per-device refresh **Number** (Phase 6) is separate from any screen
  `refresh_rate` **param** (e.g. `gphoto`); both can coexist as distinct entities.
