# Byonk ↔ Home Assistant — Phase 3: HA Integration

**Date:** 2026-06-29
**Status:** Approved (design)
**Scope of this spec:** Phase 3 only — the Home Assistant **custom integration**
(`custom_components/byonk/`, Python) that manages a byonk instance running as the Phase 2
add-on. Phases 1 (admin API) and 2 (add-on) are **done** and are the contract this builds on.
Phase 4 (release automation, HACS default-list submission, docs polish) is referenced for
context but specced separately.

---

## 1. Background & where this fits

Byonk is a self-hosted content server for TRMNL e-ink devices (a static Rust binary shipped as
`ghcr.io/oetiker/byonk`). Phase 1 added a token-gated `/api/admin/*` management API; Phase 2
packaged byonk as a Supervisor add-on that reads `admin_token`/`log_level` from
`/data/options.json` and persists config under an editable `/config`.

The umbrella goal is to make byonk **fully manageable from Home Assistant**. Phase 3 delivers
the second user-facing piece: a **Python custom integration** that runs inside HA Core, talks
to byonk **only over the Phase 1 admin API** (HTTP + Bearer token), and gives the user an
HA-native UI for device telemetry, device→screen mappings (incl. dynamic per-screen params),
global settings, and onboarding — with **zero user-entered credentials**.

### Phased plan (each phase = its own spec → plan → build)

1. **Phase 1 — Byonk admin API. ✅ DONE.**
2. **Phase 2 — HA Add-on packaging + add-on-options reader. ✅ DONE** (PR #20, merged).
3. **Phase 3 (this spec) — HA Integration** (`custom_components/byonk/`).
4. **Phase 4 — Release & docs**: automate add-on `version:` bumps, HACS metadata/default-list,
   brands registration, docs polish.

### Cross-phase contract this phase must honour (from Phase 2 §5 + project memory)

- **Zero-touch trust.** The end user never copies, pastes, or sets a byonk token. The admin
  token has exactly **one home — the add-on option** (`/data/options.json`). byonk only *reads*
  it; the integration *provisions* it.
- **No redundancy / single source of truth.** byonk's `config.yaml` is authoritative for all
  device mappings and settings. The integration **mirrors** byonk state and **must not** keep
  its own copy of anything byonk owns — including the admin token, which it reads back from the
  add-on option at runtime rather than caching.

---

## 2. Phase 3 goals & non-goals

### Goals

- A Python custom integration under `custom_components/byonk/` (one repo, this repo) that is
  **the single thing an HA user installs** to get byonk running and managed in HA.
- **Zero-touch setup on Supervised/HAOS:** the config flow installs + starts the byonk add-on
  (auto-adding its community store repository), provisions the admin token into the add-on
  option, restarts the add-on, and reads the token back to talk to the admin API — all with no
  user-entered credentials.
- **HA-native management:** one HA Device per TRMNL device with telemetry sensors and live
  control entities; a hub device for global settings; a config **subentry** per device for
  add/edit of mappings including dynamic per-screen `@params`.
- **Onboarding** of new (unregistered) devices via a Repairs nudge + a code-labeled pending
  list in the add-device form, matching byonk's on-device registration-code screen.
- **byonk stays the source of truth.** The integration mirrors `config.yaml` via the admin API
  and reconciles continuously; it never duplicates byonk-owned state.

### Non-goals (Phase 3)

- **Non-Supervisor support.** The integration is **Supervised/HAOS-only** (guarded by
  `is_hassio`); on a plain Container/Core install it aborts with guidance. No manual
  host+token path (byonk-in-HA *is* the add-on — there is nothing to manage without it).
- **HACS default-list submission, brands registration, release automation, add-on `version:`
  automation** — Phase 4. Phase 3 ships a valid `manifest.json` + minimal `hacs.json` so the
  integration is installable as a HACS *custom repository*.
- **byonk server feature changes.** The only optional, deferrable byonk-side change is HA-worded
  wording for the existing registration screen (see §8); not required for Phase 3.
- **Per-param live entities** (`number`/`text` per param). Params are edited in the subentry
  form (YAGNI — no concrete per-param entity need yet).
- **Ingress / exposing byonk's `/dev` UI** (out of scope, as in Phase 2).

---

## 3. Architecture

```
HA Core (Python integration)                Supervisor                 byonk add-on (:3000)
────────────────────────────               ────────────               ─────────────────────
ConfigFlow (zero-touch, Supervised-only)
  ├─ store.add_repository(byonk add-on repo)  ──▶  clones community repo into add-on store
  ├─ store.install_addon(<hash>_byonk) + addons.start_addon
  ├─ AddonManager.async_set_addon_options(admin_token=<generated>) + async_restart_addon
  │                                                                  ──▶ reads /data/options.json
  └─ 1 ConfigEntry (stores add-on slug + base URL; NOT the token)
        │   token read back from add-on options at runtime (single source of truth)
        ├─ Hub HA Device "Byonk Server": global-settings entities + pending sensor
        └─ N ConfigSubentries (type "device"), one per registered TRMNL device
              └─ 1 HA Device each: telemetry sensors + live control selects
                          │
   DataUpdateCoordinator ─┴─ HTTP admin API (Bearer) ──▶ byonk /api/admin/{devices,pending,screens,config}
        └─ reconciliation: byonk config.yaml ⇄ HA subentries/devices
```

Three moving parts, each independently testable:

1. **Trust/lifecycle layer** (`addon.py`, config flow) — discovers/installs/starts the add-on
   and provisions the token via the Supervisor API. The only place that touches Supervisor.
2. **Data layer** (`api.py`, `coordinator.py`) — an async admin-API client + a single polling
   coordinator that exposes byonk state and reconciles it into HA devices/subentries.
3. **Presentation layer** (entities, subentry flow, repairs) — reads coordinator data, writes
   back through the admin API.

byonk is reached only over HTTP; there is **no shared code/FFI** with the Rust binary. Clean
process boundary.

---

## 4. Trust & lifecycle (Supervised-only, zero-touch) — `addon.py` + config flow

### `is_hassio` guard

`async_step_user` first calls `homeassistant.components.hassio.is_hassio(hass)`. If false →
`self.async_abort(reason="not_hassio")` ("Byonk requires the Byonk add-on, which needs a
Supervised/HAOS installation."). Single-instance: `_async_abort_entries_match` /
`async_set_unique_id` so only one byonk config entry exists.

### Option B — graceful auto-install (the simplest user journey)

The integration is **the single thing the user installs** (via HACS). On first setup the config
flow brings up the add-on itself. Verified-feasible call sequence (Core's `SUPERVISOR_TOKEN`
is **not** gated out of `/store/*`; no in-tree precedent, so handle errors carefully):

1. **Locate or add the add-on.** `client = get_supervisor_client(hass)`
   (`homeassistant.components.hassio.get_supervisor_client`).
   - `await client.store.addons_list()` → look for an add-on whose config slug is `byonk`
     (full installable slug is `<repo_hash>_byonk`, `repo_hash = sha1(url.lower())[:8]`).
   - If absent, `await client.store.add_repository(StoreAddRepository(repository=<BYONK_ADDON_REPO_URL>))`.
     This does a **blocking git clone**; on `SupervisorError` (bad URL/network/duplicate) fall
     back gracefully (see below). After a successful add, the repo's add-ons are immediately
     listable (Supervisor reloads internally; no manual `store.reload()` needed). Re-list and
     resolve the `<hash>_byonk` slug (don't hard-code the hash — discover it).
2. **Install + start.** If not installed: `await client.store.install_addon(slug)` then
   `AddonManager.async_start_addon` (or `client.addons.start_addon(slug)`). Wait for
   `AddonState.RUNNING` via `async_get_addon_info()`.
3. **Provision the token.** Generate `secrets.token_hex(32)`; read current options via
   `AddonManager.async_get_addon_info().options`; `async_set_addon_options({**options,
   "admin_token": token})`; `async_restart_addon()`. After the restart byonk re-reads
   `/data/options.json` and `/api/admin/*` comes alive. **The token is never logged or shown.**
4. **Create the entry.** Store **the add-on slug + base URL** (`http://{addon_info.hostname}:3000`)
   in `entry.data` — **not the token.** Confirm reachability with an authenticated probe
   (`GET /api/admin/config`).

`AddonManager` is a per-slug singleton subclass (the zwave_js pattern), constructed with the
discovered slug:
```python
@singleton(DATA_ADDON_MANAGER)
@callback
def get_addon_manager(hass, slug) -> AddonManager:
    return ByonkAddonManager(hass, LOGGER, "Byonk", slug)
```

### Graceful fallback (no dead-ends)

Any `SupervisorError`/`AddonError` from the repo-add/install/start/options/restart path →
`async_abort` (or a form with an error) that shows the **one-line repo-add link** + "install
the Byonk add-on, then retry." So the magical happy path is attempted first, but a Supervisor
failure never bricks the flow. All Supervisor calls are guarded by `is_hassio`.

### Token read-back (no redundancy)

The entry stores no token. At `async_setup_entry` (and on `ConfigEntryAuthFailed`/404 from the
admin API) the integration reads the token from the add-on options
(`AddonManager.async_get_addon_info().options["admin_token"]`) and holds it **only in memory**
(`runtime_data`). The add-on option remains the single source of truth. If the option is blank
(e.g. user cleared it), the integration re-provisions (step 3) rather than caching.

---

## 5. Data layer — `api.py` + `coordinator.py`

### Admin-API client (`api.py`)

A thin async wrapper over Phase 1 endpoints using HA's shared aiohttp
(`async_get_clientsession`) + `Authorization: Bearer <token>`:
`get_devices()`, `get_pending()`, `get_screens()`, `get_config()`, `add_device(...)`,
`update_device(key, ...)`, `delete_device(key)`, `update_settings(...)`. Maps HTTP status to
typed errors: 404 (admin dormant / re-provision) → `ByonkAuthError` → surfaced as
`ConfigEntryAuthFailed`; 401 → auth error; 400 → `ByonkValidationError` (carries byonk's
message for form display); 409 → `ByonkReadOnlyError` (config embedded/read-only).

### Coordinator (`coordinator.py`)

One `DataUpdateCoordinator[ByonkData]`, `update_interval = timedelta(seconds=60)` (local +
cheap; writes call `async_request_refresh()` for instant reflection). `always_update=False`
(data carries `__eq__`). `_async_update_data` pulls `devices`, `pending`, `screens`, `config`
in parallel; transient failures → `UpdateFailed` (entities become unavailable, no crash);
auth/404 → `ConfigEntryAuthFailed` (triggers re-provision/reauth). Wiring uses typed
`entry.runtime_data` (PEP 695 `type ByonkConfigEntry = ConfigEntry[ByonkRuntime]`), not
`hass.data`.

### Reconciliation (byonk → HA mirror)

byonk's `config.yaml` is authoritative; the integration mirrors it. Each refresh:
- **Registered** device in `/devices` with **no** subentry → `hass.config_entries.async_add_subentry`
  (type `"device"`, `unique_id` = device key) + create its HA Device (`config_subentry_id=…`).
  This is how already-registered devices (and hand-edited `config.yaml`) appear in HA with no
  user action (the `github` integration does exactly this).
- Subentry whose device **vanished** from `/devices` → `async_remove_subentry` + device cleanup.
- This keeps HA in sync with out-of-band changes (File-editor edits, admin API from elsewhere)
  and guarantees a single source of truth.

> Note (verified): HA does **not** support discovery into a *subentry* flow — subentry flows are
> `user`/`reconfigure` only. So mirroring is done programmatically (above); user-initiated
> onboarding is the subentry `user` step (§7). There is no "discovered sub-device" card.

---

## 6. Entities

### Hub HA Device — "Byonk Server" (identifiers `{(DOMAIN, entry_id)}`)

| Entity | Platform | Source → write | Category |
|---|---|---|---|
| `switch.registration_enabled` | switch | `config.registration.enabled` → `PATCH /settings {registration_enabled}` | CONFIG |
| `select.default_screen` | select | options from `/screens`; → `PATCH /settings {default_screen}` | CONFIG |
| `select.auth_mode` | select | `api_key`/`ed25519` → `PATCH /settings {auth_mode}` | CONFIG |
| `sensor.pending_devices` | sensor | count of `/pending`; **attributes** = list of `{registration_code, model, last_seen}` | DIAGNOSTIC |

`sensor.pending_devices` is the at-a-glance list of unregistered devices (the same
registration codes shown on the devices' screens).

### Per-TRMNL HA Device (one per subentry; identifiers `{(DOMAIN, device_key)}`, `via_device` = hub)

| Entity | Platform | Source → write | Category |
|---|---|---|---|
| `sensor.battery_voltage` | sensor | `/devices[].battery_voltage` | DIAGNOSTIC |
| `sensor.signal_strength` (rssi) | sensor | `/devices[].rssi` | DIAGNOSTIC |
| `sensor.last_seen` | sensor (timestamp) | `/devices[].last_seen` | DIAGNOSTIC |
| `sensor.firmware_version` | sensor | `/devices[].firmware_version` | DIAGNOSTIC |
| `sensor.model` | sensor | `/devices[].model` | DIAGNOSTIC |
| `select.screen` | select | options from `/screens`; → `PATCH /devices/:key` | — |
| `select.dither` | select | options from `/screens.dither_algorithms`; → `PATCH /devices/:key` | — |
| `select.panel` | select | options from `/screens.panels`; → `PATCH /devices/:key` | — |

**`select.screen` param reset (decided):** changing the active screen sends
`{screen, params: <new screen's defaults>}` (defaults computed from that screen's `@params`),
so the mapping is **valid by construction**; the user fine-tunes params afterwards in the
subentry form. (Rationale: byonk validates params against the screen on write; a bare
screen-swap would leave stale, possibly-invalid params.)

**Per-screen `@params` are not live entities** — edited in the subentry form (§7). No
`number`/`text` platforms in this phase. `colors` (palette) is handled in the form, not as a
live entity.

Entities are `CoordinatorEntity[ByonkCoordinator]`; all writes go through the admin client then
`async_request_refresh()`.

---

## 7. Device management & onboarding — subentry flow + Repairs

### Subentry flow (`config_flow.py::ByonkDeviceSubentryFlow`)

`ConfigFlow.async_get_supported_subentry_types` → `{"device": ByonkDeviceSubentryFlow}`.

- **Add (`async_step_user`):**
  1. **Step "user":** pick a **pending device** from a code-labeled dropdown
     (`ABCD-1234 · TRMNL og · seen 2m ago`, from `/pending`) **or** type a MAC to pre-register;
     choose **screen**, optional **panel/dither/colors**.
  2. **Step "params":** a **dynamically built** voluptuous form for the chosen screen's
     `@params`, each field → an HA selector (see §9 mapping). Submit →
     `POST /api/admin/devices` then `async_create_entry(...)` (creates the subentry + device).
- **Edit (`async_step_reconfigure`):** same dynamic form, prefilled via
  `description={"suggested_value": current}` (so optional fields stay clearable); submit →
  `PATCH /api/admin/devices/:key`. **Remember:** the admin API treats `params` as a **full
  replacement** — the form always sends the complete param set.
- **Remove:** subentry deletion → `DELETE /api/admin/devices/:key`.

The **registration code is the join key**: it is rendered on the device's e-ink screen (byonk's
registration screen, Phase 1) and shown in the add-device list, so the physical device is
unambiguously matched to the list entry.

### Repairs (`repairs.py`)

Each refresh, for every device in `/pending`, `homeassistant.helpers.issue_registry.
async_create_issue(... "device_pending_<code>" ...)` ("TRMNL `ABCD-1234` is waiting to be set
up in Byonk"), `async_delete_issue` once it is no longer pending. This is the **"unconfigured"
marker** — visible in Settings → Repairs, self-resolving on onboarding. (Optional fast-follow:
a Repairs *fix flow* that links straight into the add-device form; informational issue is
sufficient for Phase 3.) The flow also ensures `registration.enabled = true` so unconfigured
devices show their code screen rather than nothing.

---

## 8. byonk-side changes

**None required.** Phase 1 already renders a registration screen showing each unregistered
device's registration code (`src/api/display.rs`, `render_registration_screen`), which is the
on-device half of the code-matching onboarding. The admin API already exposes everything the
integration needs.

**Optional fast-follow (deferred):** ship/point `config.registration.screen` at an HA-worded
registration screen ("Set me up in Home Assistant — code: ABCD-1234"). Adds a screen asset;
not needed for Phase 3.

---

## 9. Dynamic `@params` → HA selectors (verified mapping)

Each `ParamField` from `/api/admin/screens` maps to a `homeassistant.helpers.selector`:

| `@params` type | Selector |
|---|---|
| `string` | `TextSelector(TextSelectorConfig(type=TEXT))` (`multiline` → multiline; `sensitive` → `PASSWORD`) |
| `url` | `TextSelector(TextSelectorConfig(type=URL))` |
| `int` | `NumberSelector(NumberSelectorConfig(min, max, step=1, mode=BOX, unit_of_measurement=unit))` |
| `float` | `NumberSelector(... step="any" or `step`)` |
| `bool` | `BooleanSelector()` |
| `enum` | `SelectSelector(SelectSelectorConfig(options=[SelectOptionDict(value,label)], mode=DROPDOWN))` |
| `color` | `ColorRGBSelector()` (value `[r,g,b]`) — or `TextSelector(type=COLOR)` for a hex string, per byonk's `colors` format |

`required`/`optional` + `default` are realized with `vol.Required/Optional` and, for edit forms,
`suggested_value` (so optionals can be cleared → omitted from the result). `hidden` fields are
omitted from the form (still accepted by the API); `advanced` fields can be grouped. A screen
whose `schema_error != null` is still selectable; its param form surfaces the error and falls
back to free-form key/values. Built in `param_form.py`.

---

## 10. Files & manifest

```
custom_components/byonk/
  __init__.py        # async_setup_entry/unload, runtime_data, ties trust+coordinator
  manifest.json
  const.py           # DOMAIN, BYONK_ADDON_REPO_URL ("https://github.com/oetiker/byonk"),
                     #   addon config-slug ("byonk"), defaults
  addon.py           # ByonkAddonManager, slug discovery, repo-add, install/start, token provision/read-back
  api.py             # admin API client + typed errors
  coordinator.py     # DataUpdateCoordinator + reconciliation
  config_flow.py     # ByonkConfigFlow (trust) + ByonkDeviceSubentryFlow (device add/edit)
  param_form.py      # @params schema → voluptuous/selectors
  entity.py          # CoordinatorEntity base + DeviceInfo helpers (hub + per-device)
  sensor.py select.py switch.py
  repairs.py         # pending-device issues
  diagnostics.py     # (optional) redacted entry/coordinator dump
  strings.json
  translations/en.json
hacs.json            # repo root: { "name": "Byonk" } (minimal; full HACS metadata = Phase 4)
```

`manifest.json` (custom-integration required keys):
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
- `integration_type: "hub"` (creates multiple devices); `iot_class: "local_polling"`.
- `after_dependencies: ["hassio"]` (not a hard `dependencies`) so the integration still loads
  on Container/Core to show the friendly `not_hassio` abort; all Supervisor use is `is_hassio`-
  guarded.
- No external pip `requirements` (uses HA's bundled aiohttp + `aiohasupervisor` via the hassio
  component).

---

## 11. Testing (TDD)

Python tests with `pytest` + `pytest-homeassistant-custom-component` (separate from the Rust
`make` flow; add a Python lint/test target/CI step):

- **Config-flow / trust** (mock `AddonManager` + `get_supervisor_client`): `not_hassio` abort;
  happy path (repo-add → install → start → set-options → restart → entry created, token never in
  `entry.data`); graceful fallback on `SupervisorError`; single-instance guard; token read-back
  (entry stores no token; admin client uses the option value); re-provision when option blank.
- **API client:** status→error mapping (404→auth, 400→validation w/ message, 409→read-only);
  bearer header; `params` full-replacement on update.
- **Coordinator + reconciliation:** registered-with-no-subentry → subentry+device created;
  vanished device → removed; `UpdateFailed` vs `ConfigEntryAuthFailed` paths; `async_request_refresh`
  after writes.
- **Entities:** each sensor's value/availability; each select's options + `async_select_option`
  → correct PATCH; `select.screen` sends defaulted params; switch toggles settings; hub
  `pending_devices` count + attributes; `EntityCategory` assignments.
- **Subentry flow:** add (pending dropdown + manual MAC) → POST + subentry; reconfigure → PATCH
  full param set; dynamic `param_form` per `@params` type (int/float/enum/bool/color/url/string,
  required vs optional+default, `schema_error` fallback).
- **Repairs:** issue created per pending device; deleted on registration.
- **Manifest/HACS (static):** `manifest.json` keys + invariants (`integration_type: hub`,
  `after_dependencies: ["hassio"]`, required custom keys); `hacs.json` present/parses.

### Manual acceptance (documented, not automated)

On a real Supervised/HAOS install: install the integration via HACS → Add Integration →
add-on repo added + add-on installed/started + token provisioned + admin API reachable; a
connecting TRMNL device shows its code screen, appears in `sensor.pending_devices` + Repairs,
and is onboarded via the add-device form (code-matched); selects/switch round-trip to
`config.yaml`; hand-edits to `config.yaml` reconcile into HA.

---

## 12. Staged build (drives the implementation plan)

1. **Trust + skeleton:** package scaffolding, `manifest.json`/`hacs.json`, `const`, `api.py`,
   `addon.py` (discovery/repo-add/install/start/provision/read-back), `config_flow.py` trust
   path (+ `not_hassio` abort, graceful fallback), `coordinator.py`, hub HA Device with
   `pending_devices` sensor + global-settings entities. *Done when:* installing the integration
   brings up the add-on, provisions the token, and the admin API is reachable.
2. **Mirror + telemetry:** reconciliation loop; per-device subentries/HA Devices for
   already-registered devices; telemetry sensors.
3. **Live controls:** `select.screen`/`dither`/`panel` (+ screen param-reset), confirm
   hub `registration_enabled`/`default_screen`/`auth_mode`.
4. **Onboarding:** subentry add/edit flow with dynamic `@params` (`param_form.py`) + Repairs
   pending issues.

---

## 13. Risks & mitigations

- **Community-repo auto-install is unconventional** (no core-integration precedent; repo-add does
  a blocking git clone). *Mitigation:* the call is verified-feasible with Core's
  `SUPERVISOR_TOKEN`; wrap the whole path in graceful `SupervisorError` handling that falls back
  to a manual repo-add link — never a dead-end.
- **Supervisor/`aiohasupervisor` API drift** (method names stabilized in the 0.3.x line). *Mitigation:*
  go through `homeassistant.components.hassio` helpers (`get_supervisor_client`, `AddonManager`)
  rather than pinning the SDK; guard with `is_hassio`; verify symbols against the targeted Core.
- **Add-on slug is URL-hash-prefixed** (`<hash>_byonk`). *Mitigation:* discover via
  `store.addons_list()` matching the `byonk` config-slug; never hard-code the hash.
- **HA config-subentry API is relatively new** (Core 2025.3; renamed 2025.4). *Mitigation:* use
  the current symbols (`async_get_supported_subentry_types`, `ConfigSubentryFlow`,
  `_get_entry`/`_get_reconfigure_subentry`, `config_subentry_id`); set a minimum HA version in
  `hacs.json`.
- **No subentry discovery** (framework limitation). *Mitigation:* programmatic reconciliation +
  Repairs nudge + pending sensor deliver the onboarding UX without discovery cards.
- **Token redundancy risk.** *Mitigation:* entry stores no token; always read back from the
  add-on option; re-provision if blank.

---

## 14. Acceptance criteria (Phase 3 done when…)

1. `custom_components/byonk/` is a valid, HACS-installable custom integration
   (`manifest.json` + `hacs.json`); manifest static test passes.
2. On Supervised/HAOS, the config flow auto-adds the add-on repo, installs + starts the add-on,
   provisions the admin token into the add-on option, restarts it, and confirms admin-API
   reachability — **no user-entered credentials**; on non-Supervisor it aborts with guidance;
   Supervisor failures fall back gracefully.
3. The entry stores **no token**; the integration reads it back from the add-on option at
   runtime (single source of truth) and re-provisions if blank.
4. One hub device exposes global settings + a `pending_devices` sensor; one HA Device per
   registered TRMNL exposes telemetry sensors + `screen`/`dither`/`panel` selects; reconciliation
   keeps HA mirrored to byonk's `config.yaml`.
5. A subentry add/edit flow creates/updates/removes device mappings via the admin API, with a
   dynamic `@params` form (all field types) and full-param-replacement semantics; pending
   devices appear code-labeled in the add form and as Repairs issues that self-resolve on
   onboarding.
6. Python test suite (config flow/trust, coordinator/reconciliation, entities, subentry/param
   form, repairs, manifest) passes; the existing Rust `make check`/`make docs` stay green;
   `CHANGES.md` + docs updated.
