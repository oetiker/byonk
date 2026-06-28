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
| `GET /api/admin/screens` | Enumerations the integration needs to build native forms: list of screens each with its **param schema** (and any schema-parse error for that screen); list of available panel profiles; list of available dither algorithms. |

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

The schema is **part of the screen definition** — it lives in the screen's own `.lua`
file, so it travels with the screen (portable/shareable) and sits right next to the code
that reads `params.*`, minimizing drift.

### Where it lives: a `@params` header in the `.lua` file

A YAML block inside a Lua block-comment at the top of the script, **parsed textually —
never executed**. byonk extracts the text between the `@params` marker and the closing
`]]`, then parses it as YAML.

```lua
--[[ @params
station:
  type: string
  label: "Stop name"
  required: true
  description: "Stop name as used by the transport API"
limit:
  type: int
  label: "Departures"
  default: 8
  min: 1
  max: 30
  unit: ""
  mode: box
]]

local station = params.station or "Olten, Südwest"
local limit   = params.limit or 8
```

*(Alternatives considered and rejected: `params_schema:` in `config.yaml` — far from the
code, drifts, and not portable with the screen; a sidecar file — proliferates files. A
Lua-executed metadata table was rejected to avoid running scripts merely to read
metadata.)*

### Schema shape

The block is a mapping of **param key → field descriptor**.

**Core fields (always supported):**

- `type` (required): one of `string | int | float | bool | enum | color | url`
- `required` (bool, default `false`)
- `default` (any, optional)
- `description` (string, optional) — helper text
- `label` (string, optional) — display name; falls back to a prettified key

**UI-helper fields (all included in v1):**

- Numbers (`int`/`float`): `min`, `max`, `step`, `unit` (e.g. `"s"`, `"dBm"`),
  `mode` (`box | slider`)
- `enum`: `options` as a list of `{ value, label }` pairs (a bare list of values is also
  accepted and labels default to the values)
- `sensitive` (bool) — mask the field (rendered as a password input)
- `multiline` (bool) — long-text input
- `hidden` (bool) — debug-only params (e.g. `test_timestamp`); known to byonk/docs but
  omitted from HA forms by default
- `advanced` (bool) — shown under an "advanced" expander in the form

**Deferred to a later phase:** `pattern` (regex), `group`/`section` (form grouping),
`show_if` (conditional visibility).

### Validation at ingestion (warn, non-fatal)

When a screen is loaded (startup and on reload), byonk parses and validates its `@params`
block:

- A **well-formed** block is stored and exposed via `GET /api/admin/screens`.
- A **malformed** block (bad YAML, unknown `type`, `enum` without `options`, etc.) is
  **logged as a clear error and surfaced in `GET /api/admin/screens`** for that screen,
  but **byonk keeps serving the screen** with its params treated as unvalidated. One
  screen's typo never takes the server down.
- A screen with **no `@params` block** is valid and accepts arbitrary params
  (back-compat).

### Use

- Exposed via `GET /api/admin/screens` (with per-screen parse errors, if any).
- Used to **validate** `params` on `POST`/`PATCH` device writes, per field `type`,
  `required`, and numeric/enum constraints. Screens without a valid schema skip param
  validation.
- Keeping the schema in sync with the Lua implementation is the screen author's
  responsibility (advisory, same as any schema/impl split).

### Scope of work: schema **all** screens in this repo

Every screen in `screens/` gets a `@params` header as part of Phase 1, derived by reading
each script's `params.*` usage:

| Screen | Params |
|---|---|
| `transit` | `station` str (def "Olten, Südwest"); `limit` int (def 8, min 1, max 30) |
| `gphoto` | `album_url` url **(required)**; `show_status` bool (def false); `refresh_rate` int (def 3600, unit "s") |
| `floerli` | `room` str (def "Rosa"); `test_timestamp` int (**hidden**, debug) |
| `fontdemo-bitmap` | `font_prefix` **enum** [X11Helv, X11LuSans, X11LuType, X11Term, X11Misc] (def X11Helv) |
| `default`, `graytest`, `hello`, `hintdemo`, `calibrator`, `mandelbrot`, `fontdemo-terminus` | no params (no `@params` block, or an empty one) |

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
- `src/models/config.rs` — optional `admin.token`; JSON (de)serialization for API
  responses; helpers to add/edit/remove device blocks and patch globals.
- New module for the **`@params` schema**: types for the field descriptors, a textual
  extractor that pulls the `@params` block out of a `.lua` file, a parser/validator
  (warn-non-fatal), and param-value validation against a schema.
- Screen loading (`AssetLoader` / `ContentPipeline` / `RenderService` where `.lua` files
  are read) — extract + validate each screen's `@params` at load/reload; cache the parsed
  schema and any parse error per screen.
- `src/services/device_registry.rs` — a method to list all devices (registry is currently
  find-by-id only); compute pending/unregistered set.
- `src/services/content_pipeline.rs` — read config via the swappable handle.
- New module for comment-preserving YAML patching (wrapping `yamlpath`/`yamlpatch`).
- `screens/*.lua` — add `@params` headers to all screens (per §5 table).
- `Cargo.toml` — add `yamlpath`, `yamlpatch`, `arc-swap`.

---

## 9. Testing (TDD)

Follow the project's TDD discipline; write tests first.

- **YAML patch unit tests:** add/edit/remove device + global scalar edits each preserve
  surrounding comments; malformed inputs rejected.
- **Param-schema tests:** `@params` extraction from a `.lua` file (incl. no-block and
  empty-block cases); well-formed schema parses and serializes correctly via
  `GET /api/admin/screens`; a malformed block is reported as an error there **without**
  failing the server and the screen still renders; param validation accepts/rejects per
  `type`, `required`, and numeric/enum constraints; all repo screens' `@params` headers
  parse cleanly.
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
- `docs/src/` — new page documenting the admin API (endpoints, auth) and the `@params`
  screen-schema format, wired into `SUMMARY.md`.

---

## 11. Risks & mitigations

- **`yamlpatch` limitations on sequences/flow lists** → mitigated by the remove+append
  device-block strategy and scalar-only global Replaces (§6). Covered by round-trip tests.
- **Hot-reload touching many config readers** → contained by the arc-swap accessor; the
  pipeline is the one non-trivial consumer and is explicitly addressed (§7).
- **Schema drift** between the `@params` header and the Lua code → minimized by
  co-location in the same file; validation is advisory and screens without a schema stay
  permissive (back-compat).
- **`@params` extractor edge cases** (nested `]]`, CRLF, BOM, indentation) → covered by
  extractor unit tests; malformed blocks degrade gracefully (warn, serve unvalidated).
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
