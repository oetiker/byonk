# Byonk ↔ Home Assistant — Phase 1: Admin/Management API

**Date:** 2026-06-28
**Status:** Approved (design)
**Scope of this spec:** Phase 1 only — the byonk-side "friendliness" layer. Phases 2–4 are summarized for context but specced separately.

---

## 1. Background & overall vision

Byonk is a self-hosted content server for TRMNL e-ink devices: a single static Rust
binary shipped as a multi-arch Docker image (`ghcr.io/oetiker/byonk`, amd64 + arm64).
It is configured via env vars (`BIND_ADDR`, `CONFIG_FILE`, `SCREENS_DIR`, `FONTS_DIR`)
and a YAML config file (`config.yaml`) that maps devices → screens and defines screen,
panel, and dither settings. Device telemetry (MAC, model, firmware, `last_seen`,
`battery_voltage`, `rssi`) is tracked in an **in-memory** registry and is currently
**not exposed over HTTP**.

The user wants byonk usable from **Home Assistant** with two deliverables:

- An **Add-on** that runs byonk as a Supervisor-managed container (reusing the
  prebuilt image, with persistent config).
- An **Integration** (`custom_components/byonk/`) that surfaces and **fully manages**
  byonk via **HA-idiomatic UI elements** (select / switch / number / text entities and
  native config/options forms) — not merely an embedded web panel. Full read-write of
  the byonk configuration and operating mode from HA.

Both live as folders inside the existing byonk repo. The integration talks to a new
**admin/management API** added to byonk. Byonk remains the source of truth and persists
all changes to `config.yaml`.

### Phased plan (each phase = its own spec → plan → build)

1. **Phase 1 (this spec)** — Byonk admin API + per-screen param schemas + config
   hot-reload + comment-preserving config writes. Pure Rust; independently testable.
2. **Phase 2** — HA Add-on packaging (`homeassistant/` add-on repository structure,
   prebuilt image, persistent `addon_config`, options, optional Ingress for `/dev`).
3. **Phase 3** — HA Integration (`custom_components/byonk/`): config flow, HA Device
   per TRMNL, sensors, diagnostic entities, select/number/switch/text controls,
   subentry-based device add/edit, registration onboarding flow.
4. **Phase 4** — Release & docs (multi-arch publishing aligned to add-on versions,
   mdBook docs, HACS metadata).

Everything in phases 2–4 depends on the Phase 1 API contract, which is why Phase 1 is
built first.

---

## 2. Phase 1 goals & non-goals

### Goals

- Expose live device telemetry and the effective configuration over HTTP (read).
- Allow full read-write management of device→screen mappings and global settings (write).
- Make per-screen parameters self-describing (param schemas) so HA can render native forms.
- Apply config writes **without losing user comments/formatting** in `config.yaml`.
- Apply config writes **without a restart** (hot-reload of in-memory config).
- Secure the whole admin surface behind a bearer token; disabled by default.

### Non-goals (Phase 1)

- Any Home Assistant code (add-on or integration) — later phases.
- Runtime log-level changes (deferred; log level stays an env/add-on setting).
- Editing screen definitions / Lua / SVG or panel profiles via the API (screen
  *authoring* stays file-based; only device mappings + global settings are writable).
- Switching the container run mode (serve vs dev) — that is an add-on/Supervisor
  concern handled in Phase 2/3.
- Persisting the device registry across restarts (it remains in-memory; devices
  repopulate as they poll).

---

## 3. Authentication

- Admin token sourced from env var **`BYONK_ADMIN_TOKEN`**, falling back to an optional
  `admin.token` field in `config.yaml`.
- **If no token is configured, every `/api/admin/*` route returns `404 Not Found`** — the
  admin surface is invisible and secure by default. The add-on (Phase 2) injects the token
  via env, and the integration (Phase 3) reuses it.
- When enabled, each admin request must carry `Authorization: Bearer <token>`. Missing or
  wrong token → `401 Unauthorized`. Constant-time comparison for the token check.
- Token is never returned by any endpoint (e.g. `GET /api/admin/config` omits `admin.token`).
- The existing TRMNL-facing endpoints (`/api/setup`, `/api/display`, …) are unchanged and
  keep their own auth.

---

## 4. API surface

All endpoints are JSON, under `/api/admin/`, token-gated as above. Errors use the existing
`ApiError` JSON shape with appropriate status codes.

### Read

| Method + path | Purpose |
|---|---|
| `GET /api/admin/devices` | All known devices: union of configured devices and live-seen devices. Each entry merges configured mapping (screen, dither, panel, colors, params) with live telemetry (model, firmware, `last_seen`, `battery_voltage`, `rssi`) and the **resolved active** screen/dither/panel actually being served. |
| `GET /api/admin/pending` | Devices that have connected but are **not registered** — includes the registration code shown on-device, model/firmware, `last_seen`. Empty when registration is disabled. |
| `GET /api/admin/config` | The full effective configuration as JSON (devices, screens, panels, global settings). Secrets such as `admin.token` are omitted. |
| `GET /api/admin/screens` | Enumerations the integration needs to build native forms: list of screens each with its **param schema**; list of available panel profiles; list of available dither algorithms. |

### Write

| Method + path | Purpose |
|---|---|
| `POST /api/admin/devices` | Add a device mapping. Body: `{ key, screen, panel?, dither?, colors?, params? }` where `key` is a MAC (`AA:BB:…`) or a registration code (`ABCDE-FGHJK`). Used both for fresh additions and to register a pending device. `409` if the key already exists. |
| `PATCH /api/admin/devices/{key}` | Update any subset of `screen`, `panel`, `dither`, `colors`, `refresh`, `params` for an existing mapping. `404` if the key is not in config. |
| `DELETE /api/admin/devices/{key}` | Remove a device mapping. `404` if not present. |
| `PATCH /api/admin/settings` | Update global settings: `registration.enabled`, `auth_mode`, `default_screen`. Partial updates allowed. |

### Cross-cutting write behavior

- All writes are validated **before** mutating the file (e.g. unknown screen name,
  unknown panel, malformed colors, params failing the screen's schema → `400` with a
  descriptive message; nothing is written).
- Writes require a **file-backed config** (`CONFIG_FILE` set / a real `config.yaml` on
  disk). If byonk is running purely from the embedded default with no writable file,
  write endpoints return `409 Conflict` with an explanatory message.
- A successful write returns the updated resource (the same shape the corresponding GET
  would return for that device / settings block).
- Writes are serialized with a mutex so concurrent admin requests cannot interleave
  file patches.

---

## 5. Per-screen parameter schema

### Where it lives

An **optional `params_schema:` block** under each screen in `config.yaml`, beside the
existing `script` / `template` / `default_refresh` fields. Declarative, readable without
executing Lua, and co-located with the screen definition.

*(Alternative considered and rejected for Phase 1: embedding the schema inside the Lua
script. Rejected to avoid having to execute Lua merely to read metadata, and to keep the
schema serializable straight from config.)*

### Shape

Each schema entry is a list of field descriptors:

```yaml
screens:
  transit:
    script: transit.lua
    template: transit.svg
    default_refresh: 60
    params_schema:
      - name: station
        type: string
        required: true
        description: "Stop name as used by the transport API"
      - name: limit
        type: int
        required: false
        default: 8
        description: "Number of departures to show"
```

Field descriptor fields:

- `name` (string, required)
- `type` (enum, required): one of `string | int | float | bool | enum | color | url`
- `required` (bool, default `false`)
- `default` (any, optional)
- `description` (string, optional)
- `options` (list, required **iff** `type == enum`): allowed values

### Behavior

- Exposed via `GET /api/admin/screens`.
- Used to **validate** `params` on `POST`/`PATCH` device writes. Screens with no
  `params_schema` accept arbitrary params (back-compat; no validation).
- Keeping the schema in sync with the Lua implementation is the screen author's
  responsibility (same as any schema/impl split). Existing screens in this repo gain
  `params_schema` blocks as part of this phase where their params are known
  (`transit`, `gphoto`, `mandelbrot`, `fontdemo-bitmap`, …); screens without one keep
  working.

---

## 6. Comment-preserving config writes

### Libraries

Add `yamlpath` and `yamlpatch` (from the zizmor project; actively maintained,
comment/format-preserving surgical patching). `serde_yaml` remains for parsing the file
into `AppConfig`.

### Write strategy (avoids yamlpatch's weak spots)

`yamlpatch`'s `Replace` is unreliable on sequences/flow lists. The strategy below routes
around that:

- **Device add / edit / remove** → **remove the device's subtree, then append a freshly
  block-serialized subtree**. Device blocks are machine-managed, so they carry no user
  comments to lose; every comment *outside* the device block is preserved.
  - *Add*: append a new block under `devices:`.
  - *Edit*: remove the existing `devices.<key>` subtree + append the updated block.
  - *Remove*: remove the `devices.<key>` subtree.
- **Global scalar settings** (`registration.enabled`, `auth_mode`, `default_screen`) →
  in-place scalar **`Replace`** (supported and safe).

### Pipeline for a write

1. Read `config.yaml` text from disk.
2. Validate the requested change against the in-memory parsed config (and param schema).
3. Build the `yamlpatch` operation(s) per the strategy above.
4. Apply to the text; write the result back atomically (write to temp file in the same
   dir, then rename).
5. Reparse the new text into `AppConfig`; if parse fails, **roll back** to the previous
   file contents and return `500` (should not happen if validation is correct, but guards
   against corruption).
6. Hot-swap the in-memory config (§7).

### Comment-preservation guarantee (test target)

A round-trip test takes a `config.yaml` rich with comments, performs each write type, and
asserts that all comments not inside the touched device block are byte-identical
afterward.

---

## 7. Config hot-reload

- `AppState.config` becomes an **`arc-swap`** handle (`Arc<ArcSwap<AppConfig>>`) instead of
  a plain `Arc<AppConfig>`. All readers use a `.load()` accessor.
- `ContentPipeline` currently holds its own `Arc<AppConfig>`. It must read through the same
  swappable handle (preferred) **or** be rebuilt on each config change. Decision deferred to
  planning; the simplest correct approach (likely: pipeline reads the handle) wins. The
  acceptance criterion is that `/api/display` reflects config changes with no restart.
- After a successful write (§6), reparse and atomically `store` the new `AppConfig`.
- **Bonus (include if cheap):** reuse the existing dev-mode file-watcher so *external* edits
  to `config.yaml` also trigger a reload. Not required for Phase 1 acceptance.

---

## 8. Affected byonk code (orientation, not prescription)

- `src/server.rs` — add `/api/admin/*` routes; `AppState.config` → arc-swap; admin auth
  middleware.
- `src/api/` — new admin handlers module.
- `src/models/config.rs` — `params_schema` on screen config; optional `admin.token`; JSON
  (de)serialization for API responses; helpers to add/edit/remove device blocks and patch
  globals.
- `src/services/device_registry.rs` — a method to list all devices (registry is currently
  find-by-id only); compute pending/unregistered set.
- `src/services/content_pipeline.rs` — read config via the swappable handle.
- New module for comment-preserving YAML patching (wrapping `yamlpath`/`yamlpatch`).
- `Cargo.toml` — add `yamlpath`, `yamlpatch`, `arc-swap`.

---

## 9. Testing (TDD)

Follow the project's TDD discipline; write tests first.

- **YAML patch unit tests:** add/edit/remove device + global scalar edits each preserve
  surrounding comments; malformed inputs rejected.
- **Param-schema tests:** schema parses from config; serializes correctly via
  `GET /api/admin/screens`; param validation accepts/rejects correctly per type and
  `required`.
- **API integration tests** (axum harness, mirroring existing `tests/`): each endpoint's
  happy path + error paths — `401` (wrong token), `404` (admin disabled, unknown device),
  `409` (duplicate key, embedded-only config), `400` (validation failures).
- **Hot-reload test:** `PATCH` a device's screen → `GET /api/admin/devices` reflects it →
  `GET /api/display` for that device serves the new screen, all without a restart.
- `make check` (fmt + clippy + tests) clean.

---

## 10. Documentation & changelog

- `CHANGES.md` — add entries under **Unreleased** for the admin API, param schemas, and
  hot-reload.
- `docs/src/` — new page documenting the admin API (endpoints, auth, `params_schema`
  format), wired into `SUMMARY.md`.

---

## 11. Risks & mitigations

- **`yamlpatch` limitations on sequences/flow lists** → mitigated by the remove+append
  device-block strategy and scalar-only global Replaces (§6). Covered by round-trip tests.
- **Hot-reload touching many config readers** → contained by the arc-swap accessor; the
  pipeline is the one non-trivial consumer and is explicitly addressed (§7).
- **Schema drift** between `params_schema` and Lua → author responsibility; validation is
  advisory and screens without a schema stay permissive (back-compat).
- **Empty/flow constructs in existing config** (e.g. `params: {}`) → exercised by tests
  against the repo's real `config.yaml` to catch patcher edge cases early.

---

## 12. Acceptance criteria (Phase 1 done when…)

1. With `BYONK_ADMIN_TOKEN` unset, all `/api/admin/*` return `404`.
2. With a token set, all endpoints in §4 work with `Bearer` auth and reject bad/missing tokens.
3. A device's screen/params can be changed via the API and is served on the next
   `/api/display` **without restarting** byonk.
4. After any write, comments elsewhere in `config.yaml` are preserved.
5. `GET /api/admin/screens` returns param schemas usable to build forms; param validation
   enforces them on writes.
6. `make check` passes; `CHANGES.md` and admin-API docs updated.
