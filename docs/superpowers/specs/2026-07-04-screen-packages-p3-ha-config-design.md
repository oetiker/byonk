# Screen Packages — Plan 3: Home Assistant package management

_Date: 2026-07-04 · Status: design (approved in brainstorming, pending spec review)_

## 1. Context & the gap

Plan 2 shipped byonk's git-backed package **distribution** and its token-gated
admin API (`/api/admin/packages` register/patch/delete/update, enriched
`GET /packages` with status/sha/pin_kind, and `package_refresh_interval`). It is
verified working live on the HA test VM.

What is missing is any **management surface in Home Assistant**. Today, adding an
external screen package means hand-POSTing to the token-gated admin API. The HA
integration has:

- a **hub config entry** (`_hub_entry`, `_async_setup_hub_entry`) whose entities
  hang on a **hub device** (`via_device=(DOMAIN, entry.entry_id)`);
- **per-device config entries** created by discovery
  (`SOURCE_INTEGRATION_DISCOVERY`) and kept in sync by a strike-based reconcile
  loop in `coordinator.py`;
- server-level settings surfaced only as **hub entities**: a new-device-screen
  select (`ByonkNewDeviceScreenSelect` → `registration_screen`), an auth-mode
  select (`ByonkAuthModeSelect` → `auth_mode`), a registration switch, and a
  pending-devices sensor;
- **no Options Flow** and **no subentries**.

This plan implements spec §9a.4 ("Where package config lives in Home Assistant")
against that current architecture.

There are **no users yet**; existing HA config may be regenerated. No
backward-compatibility or migration of stored config is required.

## 2. Goals / non-goals

**Goals**

1. Manage the package registry (add / edit / remove `handle → {repo, pin,
   token}`) from a **native-looking** HA surface.
2. Move the singleton server settings (`registration_screen`, `auth_mode`,
   `package_refresh_interval`) off entities into an Options Flow.
3. Surface per-package fetch status and an "update all" action as hub entities.
4. Keep **byonk as the single source of truth** — the HA surfaces are thin
   fronts over the admin API; HA persists no secret and no authoritative copy of
   the registry.

**Non-goals**

- No byonk-core changes: the §9a admin API already exists and is sufficient.
- No byonk-native admin web UI (separate, unplanned surface).
- No change to the per-device config-entry model or device onboarding.
- `default_screen` (the global fallback for registered devices) is **not**
  surfaced here — it is not an entity today and stays at its config default.
  ("New-device screen" in §9a.4 maps to `registration_screen`, the key the
  entity being removed already writes.)

## 3. Architecture overview

The hub config entry gains three coordinated surfaces, all reading/writing byonk:

| Surface | HA mechanism | Byonk calls | Persists in HA? |
|---|---|---|---|
| **Packages** (add/edit/remove list) | **Config subentries** of the hub entry | `POST/PATCH/DELETE /packages` | thin identity record only (`handle`, `repo`, `pin`) — **never the token** |
| **Server settings** (singletons) | **Options Flow** ("Configure" ⚙) | `PATCH /settings` | nothing (read live, write through) |
| **Status & actions** | **Entities** on the hub device | `GET /packages`, `POST /packages/update` | nothing (reconciled from byonk) |

Byonk's `GET /api/admin/packages` is authoritative. The coordinator fetches it
each refresh into `ByonkData.packages` and **reconciles** HA subentries and
status entities to match — the same principle already used for device entries.

## 4. Packages as config subentries

### 4.1 Why subentries

Subentries give HA's native "Add package" button + list with per-row
Configure/Delete — the native look requested. They are a *storage* construct
(HA persists each subentry in the entry), so we treat them as a **reconciled
projection** of byonk's registry, exactly as per-device entries project byonk's
device list. The stored `data` is a thin identity record; the authoritative
repo/pin/token/status all live in byonk.

### 4.2 Subentry type & flow

- Register a subentry type `"package"` via
  `ByonkConfigFlow.async_get_supported_subentry_types` →
  `{"package": ByonkPackageSubentryFlowHandler}` (a `ConfigSubentryFlow`).
- **Add** (`async_step_user`): form fields `handle` (required), `repo`
  (required), `pin` (optional; byonk defaults mutable `main`), `token`
  (optional, **write-only**). On submit → `POST /api/admin/packages`. On success
  → `async_create_entry(title=<handle> — <repo> @ <pin>, data={handle, repo,
  pin})`. Byonk errors (duplicate handle, bad ref) are surfaced as form errors.
- **Reconfigure** (`async_step_reconfigure`): form pre-filled from a **live**
  `GET /packages` lookup by handle (repo/pin shown; token blank). `handle` is
  immutable (it is the key). On submit → `PATCH /api/admin/packages/:handle`
  with any changed `repo`/`pin` and, if the token field is non-blank, `token`.
  → `async_update_and_abort` updating the subentry title/data. A blank token
  leaves byonk's stored token untouched.
- **Token handling:** the token field is write-only — collected, sent to byonk,
  never stored in the subentry and never displayed (byonk's `GET` only reports
  `token_set: bool`).

### 4.3 Deletion propagation (and the 409 self-heal)

HA's native subentry delete removes the subentry from HA storage immediately;
there is no per-subentry "on remove" callback. We propagate deletions to byonk
via the hub entry's **update listener** (`entry.add_update_listener`): it diffs
previous vs current subentries and, for a removed `package` subentry, calls
`DELETE /api/admin/packages/:handle`.

Byonk rejects deletion of a package still referenced by a device,
`default_screen`, or `registration.screen` (**409**). Because HA has already
dropped the subentry but byonk still has the package, the next coordinator
reconcile (§4.4) **re-adds the subentry** — the delete visibly "bounces back."
This is acceptable self-healing for v1; the reference must be cleared first. We
log the 409 at warning level. (A future polish could raise an HA *repair* issue
naming the blocking reference.)

### 4.4 Reconcile (byonk → subentries)

In `_async_reconcile`, after the existing device logic, add package reconcile:

- Build `byonk_handles = {p["handle"] for p in data.packages if not
  p["builtin"]}` and `ha_handles` from the hub entry's `package` subentries.
- **Add** a subentry for each `byonk_handles − ha_handles`
  (`hass.config_entries.async_add_subentry`).
- **Remove** a subentry for each `ha_handles − byonk_handles`
  (`async_remove_subentry`) — covers packages deleted directly in byonk.
- **Update** title/data when repo/pin changed (`async_update_subentry`).

Unlike device removal, package reconcile needs **no strike/grace counter**: the
registry only changes on explicit admin action, so immediate convergence is
correct. `byonk-builtin` is excluded (never a subentry).

## 5. Options Flow — server settings

Add `ByonkConfigFlow.async_get_options_flow` → `ByonkOptionsFlow`. A single-step
form (`async_step_init`) whose fields are read **live** from `GET /config` on
open and written on submit via `PATCH /api/admin/settings`:

| Field | Byonk key | Widget / source |
|---|---|---|
| New-device screen | `registration_screen` | select over `data.screen_names()` (qualified `handle/path` refs) |
| Auth mode | `auth_mode` | select over byonk's allowed modes |
| Package refresh interval (s; 0 = off) | `package_refresh_interval` | integer field |

The Options Flow **stores nothing** in HA. The registration on/off **switch
stays an entity** (§6) — it is a frequent, automatable toggle, not a form
setting.

## 6. Status & action entities (hub device)

- **One status sensor per non-builtin package**, reconciled from
  `data.packages`. State = `status` (`ready|fetching|error|offline`);
  `resolved_sha`, `last_fetched`, `error`, `repo`, `pin`, `pin_kind` are
  **extra state attributes** (one row per package, not four). Named by handle,
  e.g. *"weather: status"*. Associated with the package's subentry
  (`config_subentry_id`) for grouping, shown on the hub device. `byonk-builtin`
  gets none (nothing to observe).
  - Dynamic add/remove handled by the platform's reconcile (the same
    dispatcher/`async_add_entities` pattern already used for per-device param
    entities), driven off `data.packages`.
- **One "Update packages" button** on the hub device →
  `POST /api/admin/packages/update` (re-fetches all mutable-pinned packages). No
  per-package update control — editing a pin already triggers a fetch.

## 7. Entities removed

- `ByonkNewDeviceScreenSelect` and `ByonkAuthModeSelect` (both in `select.py`)
  are deleted; their settings now live in the Options Flow. The per-device
  screen select and all per-device param entities are unchanged. Update
  `strings.json`/translations accordingly.

## 8. API client additions (`api.py`)

Add to `ByonkClient` (all token-authenticated, mirroring existing methods):

- `async_get_packages() -> list[dict]` — `GET /api/admin/packages`
- `async_add_package(payload) -> dict` — `POST /api/admin/packages`
- `async_update_package(handle, payload) -> dict` — `PATCH /api/admin/packages/:handle`
- `async_delete_package(handle) -> dict` — `DELETE /api/admin/packages/:handle`
- `async_update_packages() -> dict` — `POST /api/admin/packages/update`

## 9. Coordinator changes (`coordinator.py`)

- `ByonkData` gains `packages: list[dict]` plus accessors as needed
  (e.g. `package_status(handle)`).
- `_async_update_data` adds `async_get_packages()` to the `asyncio.gather`.
- `_async_reconcile` gains the package-subentry reconcile (§4.4).
- Package status/button entities added to their platform setups, reconciled from
  `data.packages`.

## 10. HA version requirement

Config subentries and the programmatic `async_add_subentry` /
`async_remove_subentry` / `async_update_subentry` APIs require a recent Home
Assistant Core. **Confirm the minimum supported version during planning** and,
if needed, note it in the docs / add-on requirements. The integration already
targets a modern Core (uses `runtime_data`, `SOURCE_INTEGRATION_DISCOVERY`).

## 11. Testing

- **`tests_ha` (pytest + ruff):**
  - Subentry Add flow: valid input → `POST /packages` called → subentry created;
    byonk error → form error, no subentry.
  - Reconfigure flow: blank token → token omitted from PATCH; non-blank → sent.
  - Deletion propagation: removed subentry → `DELETE /packages` called; 409 →
    logged, subentry re-added on next reconcile.
  - Reconcile: byonk adds/removes a package → subentry appears/disappears;
    builtin never becomes a subentry.
  - Options Flow: fields prefilled from `GET /config`; submit → `PATCH /settings`
    with exactly the changed keys.
  - Status sensor: state + attributes map from a package row; add/remove tracks
    `data.packages`.
- **Live VM check** (`make ha-rebuild` already done for Plan 2; here only the
  integration changes → `make ha-deploy` + reload): add a package via the native
  "Add package" UI pointing at `github.com/oetiker/byonk-dist-test`, confirm the
  status sensor goes `fetching → ready` with the resolved sha, assign a device to
  its screen, confirm render; edit the pin and confirm re-fetch; delete and
  confirm removal (and the 409 bounce when a device still references it).

## 12. Risks / open items

- **Subentry deletion vs. byonk's 409 guard** (§4.3): the delete bounces back
  instead of showing an inline error — the one UX rough edge of the native-list
  choice. Accepted for v1; repair-issue polish deferred.
- **HA minimum version** for subentries (§10) — verify in planning.
- Whether status sensors should associate with the subentry vs. sit plainly on
  the hub device is an implementation detail; default to subentry association for
  grouping, fall back to hub-device-only if it complicates reconcile.

## 13. Out of scope

- byonk-core / admin-API changes; a byonk-native web UI; `default_screen`
  surfacing; device-model changes; per-package update buttons.
