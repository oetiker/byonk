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
    "screen": "transit",
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

Return the list of known screens (with their parameter schemas), panel profiles, and
supported dither algorithms.

**Response 200**:

```json
{
  "screens": [
    {
      "name": "transit",
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
      "schema_error": null
    }
  ],
  "panels": [
    {
      "name": "trmnl_og",
      "width": 800,
      "height": 480,
      "colors": "bw"
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
- `schema_error` is `null` when the schema parsed successfully, or a string describing the
  error when the `@params` block is malformed or the script file cannot be read.
- Optional `ParamField` keys (`label`, `description`, `min`, `max`, `step`, `unit`, `mode`,
  `options`) are omitted from the JSON when not set.

---

### POST /api/admin/devices

Create a new device mapping in `config.yaml`.

**Request body**:

```json
{
  "key": "AA:BB:CC:DD:EE:FF",
  "screen": "transit",
  "panel": null,
  "dither": "atkinson",
  "colors": null,
  "params": { "station": "Bern, Bahnhof", "limit": 10 }
}
```

Required fields: `key`, `screen`. All other fields are optional.

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Created — `{"key": "AA:BB:CC:DD:EE:FF", "screen": "transit"}` |
| `400` | Validation error (missing `key`/`screen`, unknown screen, param type mismatch, out-of-range value) |
| `409` | Device key already exists, or config is embedded/read-only (`set CONFIG_FILE` env var) |

---

### PATCH /api/admin/devices/:key

Update an existing device mapping. Only the provided fields are changed; omitted fields
keep their current values. The `:key` in the URL must match an existing entry in the
`devices:` config section.

**Request body** (all fields optional):

```json
{
  "screen": "transit",
  "dither": "floyd-steinberg",
  "params": { "limit": 5 }
}
```

**Responses**:

| Status | Meaning |
|--------|---------|
| `200` | Updated — `{"key": "AA:BB:CC:DD:EE:FF", "screen": "transit"}` |
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
  "default_screen": "transit"
}
```

| Field | Type | Allowed values |
|-------|------|---------------|
| `registration_enabled` | boolean | `true` / `false` |
| `auth_mode` | string | `"api_key"` or `"ed25519"` |
| `default_screen` | string | any screen name listed in `GET /api/admin/screens` |

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

## @params schema format

Screens can declare their accepted parameters in a Lua block comment at the top of the
`.lua` file. Byonk parses this block as YAML — it is never executed. The result is returned
by `GET /api/admin/screens` and validated on every write.

### Syntax

```lua
--[[ @params
<param-name>:
  type: <type>
  # … other keys …
]]
```

The block must appear before the first `]]` that follows `@params`. Everything else in the
file is ignored for schema purposes.

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

### Example — transit screen

The bundled `transit` screen uses this `@params` block:

```lua
--[[ @params
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
]]
```

Field order is preserved in the schema response. The script accesses these values via the
`params` Lua table (`params.station`, `params.limit`).

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
