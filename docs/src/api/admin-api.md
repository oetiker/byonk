# Admin API

Byonk exposes a token-gated management API under `/api/admin/*`. It lets you read device
telemetry, manage device-to-screen mappings, inspect the effective config, and update
global settings — all without restarting the server.

## Enabling the API

The admin API is disabled by default. If no token is configured, **every** `/api/admin/*`
request returns `404 Not Found` — the route is invisible to unauthenticated callers.

To enable it, provide a secret token in either of these ways (the environment variable takes
precedence):

```
BYONK_ADMIN_TOKEN=mysecrettoken   # environment variable
```

or in `config.yaml`:

```yaml
admin:
  token: mysecrettoken
```

## Authentication

Every request must include the token as a Bearer credential:

```
Authorization: Bearer mysecrettoken
```

| Situation | HTTP status |
|-----------|-------------|
| No token configured (admin disabled) | `404 Not Found` |
| `Authorization` header missing or wrong token | `401 Unauthorized` |
| Token correct | request proceeds |

The comparison is constant-time to avoid timing side-channels.

---

## Endpoints

### GET /api/admin/devices

Return all known devices: every device that has been seen (with telemetry) merged with its
config mapping, plus any configured devices that have never connected yet.

**Response 200** — array of device objects:

```json
[
  {
    "key": "AA:BB:CC:DD:EE:FF",
    "mac": "AA:BB:CC:DD:EE:FF",
    "registration_code": "ABCD-1234",
    "registered": true,
    "model": "og",
    "firmware_version": "1.7.1",
    "last_seen": "2026-06-28T10:15:00+00:00",
    "battery_voltage": 4.12,
    "rssi": -58,
    "screen": "byonk-builtin/useful/swiss-departure-board",
    "dither": "atkinson",
    "panel": null,
    "colors": null,
    "params": { "station": "Olten, Südwest", "limit": 8 }
  }
]
```

Field notes:
- `key` — the config map key for this device (MAC or registration code).
- `registered` — `true` if the device appears in the `devices:` config section.
- `registration_code` is an empty string for devices that appear in config but have never
  connected (it is never `null`).
- Telemetry fields (`model`, `firmware_version`, `last_seen`, `battery_voltage`, `rssi`) are
  `null` for devices that are configured but have never connected.
- `screen`, `dither`, `panel`, `colors`, `params` reflect the resolved config mapping; they
  are `null` when the device has no mapping.

---

### GET /api/admin/pending

Return devices that have contacted the server but are not yet registered (i.e., they appear
in the device registry but have no matching entry in the `devices:` config section).

**Response 200** — array of pending-device objects:

```json
[
  {
    "mac": "AA:BB:CC:DD:EE:FF",
    "registration_code": "ABCD-1234",
    "model": "og",
    "firmware_version": "1.7.1",
    "last_seen": "2026-06-28T09:00:00+00:00"
  }
]
```

Use `registration_code` or `mac` as the `key` when calling `POST /api/admin/devices`.

---

### GET /api/admin/config

Return the effective configuration as JSON, parsed from the on-disk `config.yaml`. The
`admin.token` field is stripped from the response.

**Response 200** — the full config as a JSON object (structure mirrors `config.yaml`).

---

### GET /api/admin/screens

Return the available screens **grouped by package**, plus panel profiles and supported
dither algorithms. Every screen is addressed by its canonical `handle/path` reference — the
value a device's `screen` field is set to.

**Response 200**:

```json
{
  "packages": [
    {
      "handle": "byonk-builtin",
      "name": "byonk-builtin",
      "description": "Screens bundled with byonk.",
      "author": "Byonk",
      "license": "MIT",
      "screens": [
        {
          "ref": "byonk-builtin/useful/swiss-departure-board",
          "title": "Swiss Departure Board",
          "description": "Live public-transport departures for a Swiss stop.",
          "params": [
            {
              "name": "station",
              "type": "string",
              "required": false,
              "default": "Olten, Südwest",
              "label": "Stop name",
              "description": "Stop name as used by the transport API"
            },
            {
              "name": "limit",
              "type": "int",
              "required": false,
              "default": 8,
              "label": "Departures",
              "description": "Number of departures to show",
              "min": 1.0,
              "max": 30.0,
              "mode": "box"
            }
          ],
          "byonk": "0.15",
          "compat_warning": null
        }
      ]
    }
  ],
  "panels": [
    {
      "name": "trmnl_og",
      "width": 800,
      "height": 480,
      "colors": "#000000,#555555,#AAAAAA,#FFFFFF"
    }
  ],
  "dither_algorithms": [
    "floyd-steinberg",
    "atkinson",
    "atkinson-hybrid",
    "jarvis-judice-ninke",
    "sierra",
    "sierra-two-row",
    "sierra-lite",
    "sierra-light",
    "stucki",
    "burkes"
  ]
}
```

Field notes:
- Screens are grouped under the package that provides them. The package-level `name`,
  `description`, `author`, and `license` come from that package's `byonk-screens.yaml`
  manifest.
- `ref` is the canonical `handle/path` reference (e.g. `byonk-builtin/useful/gphoto`) — the
  assignable screen id, and what `screen` is set to on a device.
- `title` and `description` come from the screen's `meta.yaml`.
- `params` is the screen's parameter schema (`ParamField[]`), sourced from `meta.yaml`.
- `byonk` is the engine-compatibility requirement declared in `meta.yaml`.
- `compat_warning` is `null` when the running engine satisfies the screen's `byonk`
  requirement, or a human-readable string when it does not (the screen is still served).
- `width` and `height` may be `null` for panels without explicit dimensions.
- Optional `ParamField` keys (`label`, `description`, `min`, `max`, `step`, `unit`, `mode`,
  `options`) are omitted from the JSON when not set.

---

### GET /api/admin/packages

List the registered screen packages. `byonk-builtin` is always present (it is the embedded
built-in package, registered even without a `packages:` config entry); any additional entries
come from the `packages:` config section.

**Response 200** — array of package objects:

```json
[
  {
    "handle": "byonk-builtin",
    "repo": null,
    "pin": null,
    "builtin": true,
    "token_set": false,
    "screen_count": 11,
    "status": "ready",
    "pin_kind": "embedded",
    "resolved_sha": null,
    "last_fetched": null,
    "error": null
  },
  {
    "handle": "weather",
    "repo": "github.com/acme/screens",
    "pin": "v1.4.0",
    "builtin": false,
    "token_set": true,
    "screen_count": 3,
    "status": "ready",
    "pin_kind": "tag",
    "resolved_sha": "13dce1d25716356cc7fc2ef7d137b8dfc3157fbf",
    "last_fetched": "2026-07-03T12:34:56+00:00",
    "error": null
  }
]
```

Field notes:
- `handle` — the short registry key; also the first segment of every `handle/path` screen ref.
- `repo` / `pin` — the source repo and pin for remote packages; both `null` for the embedded
  built-in.
- `builtin` — `true` for the embedded `byonk-builtin` handle (or any package without a remote
  repo).
- `token_set` — whether an auth token is configured for the package. The token itself is
  **never** serialized in any response; only this boolean is exposed.
- `screen_count` — number of screens the loader discovered in the package.
- `status` — one of:
  - `"ready"` — fetched (or embedded) and currently serving.
  - `"fetching"` — a fetch is in progress right now.
  - `"error"` — the package has never been fetched successfully (e.g. just
    registered and the background fetch hasn't completed yet, or every fetch
    attempt has failed and nothing is cached). It is not currently serving.
  - `"offline"` — the most recent refresh attempt failed, but a previously
    fetched checkout is still cached and continues to serve. A fetch failure
    never takes down an already-cached package.
- `pin_kind` — how `pin` was resolved: `"sha"`, `"tag"`, `"branch"`, or
  `"embedded"` for the built-in package. `null` if the package has never been
  successfully fetched. **A full commit `sha` pin is immutable** — it is
  fetched once and cached forever, never re-fetched. **A `tag` or `branch`
  pin is mutable** — it is re-fetched on demand (via the update endpoints
  below) and automatically every `package_refresh_interval` seconds.
- `resolved_sha` — the commit sha the package is currently pinned/fetched at,
  or `null` if never successfully fetched. The cache is keyed by `repo` +
  `resolved_sha`.
- `last_fetched` — RFC3339 timestamp of the last successful fetch, or `null`
  if never successfully fetched.
- `error` — the most recent fetch error message, or `null` if the last fetch
  (or the current state) has no error.

---

### POST /api/admin/packages

Register a new remote screen package. Triggers an asynchronous background
fetch (fire-and-forget) — the response reflects whatever status exists at
that instant (typically no status yet, since the fetch hasn't completed).
Poll `GET /api/admin/packages` for the settled result.

**Request body**:

```json
{
  "handle": "weather",
  "repo": "github.com/acme/screens",
  "pin": "v1.4.0",
  "token": "ghp_xxxxxxxxxxxx"
}
```

Required field: `handle`. `repo`, `pin`, and `token` are optional (though a
package needs `repo`/`pin` to have anything to fetch). `token` is used for
authenticating against a private repo and — like every package `token` — is
never echoed back in any response.

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Registered — returns the package's `PackageInfo` (same shape as `GET /api/admin/packages` entries) |
| `400` | Validation error (missing `handle`) |
| `409` | `handle` is `byonk-builtin` (reserved), a package with that `handle` already exists, or config is embedded/read-only (`set CONFIG_FILE`) |

---

### PATCH /api/admin/packages/:handle

Update an existing package's `repo`, `pin`, or `token`. All fields are
optional; an **omitted field keeps its current value** — in particular, an
omitted `token` is never cleared.

**Request body** (all fields optional):

```json
{
  "pin": "v1.5.0"
}
```

If `repo` or `pin` changes, a background re-fetch is triggered (same
fire-and-forget semantics as `POST /api/admin/packages`).

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Updated — returns the package's `PackageInfo` |
| `404` | No package with that handle |
| `409` | `handle` is `byonk-builtin` (reserved), or config is embedded/read-only |

---

### DELETE /api/admin/packages/:handle

Remove a package registration. Rejected if any device's `screen` still
references the handle (`<handle>/...`) — delete or repoint those device
mappings first. On success, the in-memory loader is rebuilt immediately so
the handle's screens stop resolving right away (the cached checkout on disk
is left in place).

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Deleted — `{"ok": true}` |
| `404` | No package with that handle |
| `409` | `handle` is `byonk-builtin` (reserved); a device references the handle (message names the offending device); or config is embedded/read-only |

---

### POST /api/admin/packages/:handle/update

Trigger a re-fetch of a single package handle. Fire-and-forget: the fetch
runs in the background, and the response reflects whatever status exists at
that instant. Poll `GET /api/admin/packages` for the settled status. Calling
this on `byonk-builtin` is accepted but is a no-op (the embedded package is
never fetched).

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Refresh triggered — returns the package's `PackageInfo` (pre-refresh snapshot) |
| `404` | No package with that handle |

---

### POST /api/admin/packages/update

Trigger a forced re-fetch of every registered non-builtin package
(fire-and-forget, runs in the background). Forcing bypasses the "already
cached and immutable" skip that a normal periodic refresh applies to sha
pins — every handle gets a real fetch attempt. Poll `GET /api/admin/packages`
for the settled status of each package.

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Refresh triggered for all packages — `{"ok": true}` |

---

### POST /api/admin/devices

Create a new device mapping in `config.yaml`.

**Request body**:

```json
{
  "key": "AA:BB:CC:DD:EE:FF",
  "screen": "byonk-builtin/useful/swiss-departure-board",
  "panel": null,
  "dither": "atkinson",
  "colors": null,
  "params": { "station": "Bern, Bahnhof", "limit": 10 }
}
```

Required fields: `key`, `screen`. `screen` must be a qualified `handle/path` reference (as
listed by `GET /api/admin/screens`). All other fields are optional.

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Created — `{"key": "AA:BB:CC:DD:EE:FF", "screen": "byonk-builtin/useful/swiss-departure-board"}` |
| `400` | Validation error (missing `key`/`screen`, unknown screen, param type mismatch, out-of-range value) |
| `409` | Device key already exists, or config is embedded/read-only (`set CONFIG_FILE` env var) |

---

### PATCH /api/admin/devices/:key

Update an existing device mapping. The `:key` in the URL must match an existing entry in the
`devices:` config section.

The top-level fields (`screen`, `panel`, `dither`, `colors`) merge individually: omitted
ones keep their current values.

**`params` is a full replacement:** when the `params` key is present in the request body, it
replaces the device's entire param map. To change a single param, send the complete set of
params including any unchanged ones. Omit the `params` key entirely to leave existing params
untouched.

**Request body** (all fields optional):

```json
{
  "screen": "byonk-builtin/useful/swiss-departure-board",
  "dither": "floyd-steinberg",
  "params": { "station": "Bern, Bahnhof", "limit": 5 }
}
```

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Updated — `{"key": "AA:BB:CC:DD:EE:FF", "screen": "byonk-builtin/useful/swiss-departure-board"}` |
| `400` | Validation error |
| `404` | No device with that key |
| `409` | Config is embedded/read-only |

---

### DELETE /api/admin/devices/:key

Remove a device mapping from `config.yaml`.

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Deleted — `{"deleted": "AA:BB:CC:DD:EE:FF"}` |
| `404` | No device with that key |
| `409` | Config is embedded/read-only |

---

### PATCH /api/admin/settings

Update global settings in `config.yaml`. All fields are optional; only provided fields are
changed.

**Request body**:

```json
{
  "registration_enabled": true,
  "auth_mode": "api_key",
  "package_refresh_interval": 3600
}
```

| Field | Type | Allowed values |
|-------|------|---------------|
| `registration_enabled` | boolean | `true` / `false` |
| `auth_mode` | string | `"api_key"` or `"ed25519"` |
| `package_refresh_interval` | integer | seconds between automatic re-fetches of mutable (tag/branch) package pins; `0` disables periodic refresh (the default) |

There is no `default_screen` or `registration_screen` field here — the screen
shown to un-onboarded or unassigned devices is the reserved `DEFAULT` device,
set like any other device via `POST` / `PATCH /api/admin/devices/DEFAULT`
(see above).

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Applied — `{"ok": true}` |
| `400` | Validation error (unknown screen, invalid auth_mode) |
| `409` | Config is embedded/read-only |

---

## Comment-preserving writes and hot-reload

All write endpoints (`POST`, `PATCH`, `DELETE`) modify `config.yaml` in place using a
targeted YAML path patch. Existing comments and formatting in the file are preserved — only
the specific keys that changed are rewritten.

After a successful write the server reloads the config atomically (via an ARC swap) so the
change takes effect **without a restart**. The next `/api/display` request for an affected
device will use the updated mapping immediately. If the reloaded YAML fails to parse, the
write is rolled back to the previous file contents.

Writes require a file-backed config. If the server was started with an embedded/bundled
config (no `CONFIG_FILE` environment variable), write endpoints return `409 Conflict` with
the message `"config is embedded/read-only; set CONFIG_FILE"`.

---

## Parameter schema format

Each screen declares its accepted parameters in the `params:` block of its `meta.yaml`.
Byonk parses this as YAML. The result is returned by `GET /api/admin/screens` and validated
on every device write.

### Syntax

```yaml
# <screen>/meta.yaml
title: My Screen
description: What this screen shows.
byonk: "0.15"
params:
  <param-name>:
    type: <type>
    # … other keys …
```

### Field reference

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `type` | string | — | **Required.** One of `string`, `int`, `float`, `bool`, `enum`, `color`, `url`. |
| `required` | bool | `false` | When `true`, the param must be present in every device mapping. |
| `default` | any | — | Default value shown in UI when param is absent. |
| `label` | string | — | Human-readable name for UI display. |
| `description` | string | — | Longer hint shown in tooltips or help text. |
| `min` | number | — | Minimum value (applies to `int` and `float`). |
| `max` | number | — | Maximum value (applies to `int` and `float`). |
| `step` | number | — | Increment step for UI sliders. |
| `unit` | string | — | Unit label shown next to the value (e.g., `"px"`, `"°C"`). |
| `mode` | string | — | UI hint for input style (e.g., `"box"` for a numeric input box). |
| `options` | list | — | Required for `enum` type. A list of bare strings (`[a, b]`) or `{value, label}` objects. |
| `sensitive` | bool | `false` | Treat value as a secret (mask in UI). |
| `multiline` | bool | `false` | Use a textarea instead of a single-line input. |
| `hidden` | bool | `false` | Do not show in UI (still accepted in API). |
| `advanced` | bool | `false` | Collapse into an "advanced" section in UI. |

### Example — Swiss departure board screen

The bundled `byonk-builtin/useful/swiss-departure-board` screen declares its params in
`meta.yaml`:

```yaml
# useful/swiss-departure-board/meta.yaml
title: Swiss Departure Board
description: Live public-transport departures for a Swiss stop.
byonk: "0.15"
params:
  station:
    type: string
    label: "Stop name"
    default: "Olten, Südwest"
    description: "Stop name as used by the transport API"
  limit:
    type: int
    label: "Departures"
    default: 8
    min: 1
    max: 30
    mode: box
    description: "Number of departures to show"
```

Field order is preserved in the schema response. The `script.lua` accesses these values via
the `params` Lua table (`params.station`, `params.limit`).

### Enum options

Enum options can be plain strings (where `label` defaults to the value):

```yaml
theme:
  type: enum
  options: [light, dark, auto]
```

Or objects with explicit labels:

```yaml
theme:
  type: enum
  options:
    - { value: light, label: "Light mode" }
    - { value: dark,  label: "Dark mode" }
    - { value: auto,  label: "Follow system" }
```

### Validation

When a device mapping is created or updated via the API, Byonk validates every provided
param against the screen's schema:

- Missing required params → `400 Bad Request`
- Wrong type (e.g., string where `int` expected) → `400 Bad Request`
- Value outside `min`/`max` range → `400 Bad Request`
- Value not in `enum` options → `400 Bad Request`

Extra params not listed in the schema are silently accepted (ignored by validation).
