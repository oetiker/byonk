# Configuration

Byonk embeds all screens, fonts, and configuration in the binary itself. This means you can run Byonk with zero configuration - it works out of the box.

For customization, Byonk uses a YAML configuration file to define screens and map devices to them.

## Configuration Structure

```yaml
# Screen definitions
screens:
  transit:
    script: transit.lua        # Lua script in screens/
    template: transit.svg      # SVG template in screens/
    default_refresh: 60        # Fallback refresh rate (seconds)

  weather:
    script: weather.lua
    template: weather.svg
    default_refresh: 900

# Device-to-screen mapping
devices:
  "94:A9:90:8C:6D:18":         # Device MAC address
    screen: transit             # Which screen to display
    params:                     # Parameters passed to Lua script
      station: "Olten, Bahnhof"
      limit: 8

  "AA:BB:CC:DD:EE:FF":
    screen: weather
    params:
      city: "Zurich"

# Default screen for unmapped devices
default_screen: default
```

## Screens Section

Each screen definition has three properties:

| Property | Required | Description |
|----------|----------|-------------|
| `script` | Yes | Lua script filename (relative to `screens/`) |
| `template` | Yes | SVG template filename (relative to `screens/`) |
| `default_refresh` | No | Fallback refresh rate in seconds (default: 900) |

The `default_refresh` is used when the Lua script returns `refresh_rate = 0` or omits it entirely.

## Devices Section

Each device entry maps a MAC address to a screen:

| Property | Required | Description |
|----------|----------|-------------|
| `screen` | Yes | Name of the screen definition to use |
| `params` | No | Key-value pairs passed to the Lua script |
| `colors` | No | Override display palette (comma-separated hex RGB, e.g. `"#000000,#FFFFFF,#FF0000"`) |
| `dither` | No | Dithering algorithm (see [Dither Algorithms](#dither-algorithms) below) |
| `panel` | No | Panel profile name (references `panels` section) |
| `error_clamp` | No | Error clamp for dithering (e.g. `0.08`). Limits error diffusion amplitude. |
| `noise_scale` | No | Blue noise jitter scale (e.g. `0.6`). Controls noise modulation strength. |
| `chroma_clamp` | No | Chroma clamp for dithering. Limits chromatic error propagation. |
| `strength` | No | Error diffusion strength (0.0–2.0, default 1.0). Lower = less dithering texture. |

### MAC Address Format

- Use uppercase letters with colons: `"94:A9:90:8C:6D:18"`
- The MAC address must be quoted (it's a YAML string)

### Parameters

The `params` section can contain any YAML values:

```yaml
params:
  # Strings
  station: "Olten, Bahnhof"

  # Numbers
  limit: 8
  temperature_offset: -2.5

  # Booleans
  show_delays: true

  # Lists
  rooms:
    - "Rosa"
    - "Flora"
```

These are available in Lua as the global `params` table:

```lua
local station = params.station or "Default Station"
local limit = params.limit or 10
```

### Dither Algorithms

The `dither` option selects which dithering algorithm to use. All algorithms perform color matching in perceptually uniform Oklab space and process pixels in gamma-correct linear RGB.

| Algorithm | Value | Description |
|-----------|-------|-------------|
| Atkinson (default) | `"atkinson"` | Error diffusion (75% propagation). Good general-purpose default. |
| Atkinson Hybrid | `"atkinson-hybrid"` | Hybrid propagation: 100% achromatic, 75% chromatic. Fixes color drift on chromatic palettes. |
| Floyd-Steinberg | `"floyd-steinberg"` | Error diffusion with blue noise jitter. Smooth gradients, good general-purpose. |
| Jarvis-Judice-Ninke | `"jarvis-judice-ninke"` or `"jjn"` | Wide 12-neighbor kernel. Least oscillation on sparse chromatic palettes. |
| Sierra | `"sierra"` | 10-neighbor kernel. Good balance of quality and speed. |
| Sierra Two-Row | `"sierra-two-row"` | 7-neighbor kernel. Lighter weight than full Sierra. |
| Sierra Lite | `"sierra-lite"` | 3-neighbor kernel. Fastest error diffusion. |
| Stucki | `"stucki"` | Wide 12-neighbor kernel similar to JJN. |
| Burkes | `"burkes"` | 7-neighbor kernel. Good balance of speed and quality. |

For most screens, the default `"atkinson"` works well. Use `"atkinson-hybrid"` for chromatic palettes where Atkinson shows color drift. Use `"floyd-steinberg"` for photographic content. For sparse chromatic palettes (e.g. black/white/red/yellow), try `"jarvis-judice-ninke"` or `"sierra"` to reduce oscillation artifacts.

## Default Screen

The `default_screen` specifies which screen to show for devices not listed in the `devices` section:

```yaml
default_screen: default
```

If omitted, unknown devices receive an error response.

## Device Registration

Byonk supports optional device registration for enhanced security. When enabled, new devices must be explicitly approved before they can display content.

```yaml
registration:
  enabled: true

devices:
  # Register using the code shown on the device screen
  "ABCDE-FGHJK":
    screen: transit
    params:
      station: "Olten"
```

### How It Works

1. **New device connects** - Shows the default screen with a 10-character registration code
2. **Admin reads code** - The code is displayed in 2x5 format on the e-ink screen
3. **Admin adds code to devices** - Add the code (hyphenated format) to the `devices` section
4. **Device refreshes** - Now shows the configured screen

![Registration screen showing device code](../images/registration.png)

**Note:** The registration code is derived from the device's API key via a hash function. This means:
- Devices keep their existing API key (including TRMNL-issued keys) - no WiFi reset required
- The same API key always produces the same registration code
- The config shows only the derived code, not the actual API key

### Registration Settings

| Property | Required | Description |
|----------|----------|-------------|
| `enabled` | No | Enable device registration (default: true) |
| `screen` | No | Custom screen for registration (default: uses default_screen) |

### Registration Code Format

- 10 uppercase letters displayed in 2 rows of 5: `A B C D E` / `F G H J K`
- Written in config as hyphenated: `"ABCDE-FGHJK"`
- Uses unambiguous letters only (excludes I, L, O)
- Can be used interchangeably with MAC addresses in the `devices` section
- Deterministic: same API key always produces the same code

### Example

```yaml
registration:
  enabled: true

devices:
  # By registration code (read from device screen)
  "ABCDE-FGHJK":
    screen: transit
    params:
      station: "Olten"

  # By MAC address (found in logs)
  "AA:BB:CC:DD:EE:FF":
    screen: weather
```

### Custom Registration Screen

The registration code is available to your default screen as `device.registration_code` and `device.registration_code_hyphenated`. Your default.svg can conditionally show it:

```svg
{% if device.registration_code %}
<text>Register: {{ device.registration_code_hyphenated }}</text>
{% endif %}
```

See [Device Mapping](../concepts/device-mapping.md#device-registration-security-feature) for more details.

## Authentication Mode

Byonk supports optional Ed25519 cryptographic authentication for devices. When enabled, devices use Ed25519 signatures instead of plain API keys.

```yaml
auth_mode: ed25519  # or "api_key" (default)
```

The `auth_mode` setting controls what `/api/setup` tells devices. The `/api/display` endpoint always accepts both authentication methods, so existing devices continue to work during migration.

### Ed25519 Flow

1. Device calls `GET /api/time` to get the server timestamp
2. Device signs `timestamp_ms (8 bytes BE) || public_key (32 bytes)` with its Ed25519 private key
3. Device sends `X-Public-Key`, `X-Signature`, `X-Timestamp` headers along with the normal `Access-Token` and `ID` headers
4. Server verifies the signature and checks the timestamp is within ±60 seconds

### Settings

| Property | Default | Description |
|----------|---------|-------------|
| `auth_mode` | `api_key` | Authentication mode advertised to devices (`api_key` or `ed25519`) |

## Hot Reloading

Byonk loads Lua scripts and SVG templates fresh on every request. You can edit these files without restarting the server.

However, `config.yaml` is only loaded at startup. Changes to device mappings or screen definitions require a server restart.

## Example: Complete Configuration

```yaml
# Byonk Configuration

screens:
  # Default screen - shows time and a message
  default:
    script: default.lua
    template: default.svg
    default_refresh: 300

  # Public transport departures
  transit:
    script: transit.lua
    template: transit.svg
    default_refresh: 60

  # Room booking display
  floerli:
    script: floerli.lua
    template: floerli.svg
    default_refresh: 900

devices:
  # Kitchen display - bus departures
  "94:A9:90:8C:6D:18":
    screen: transit
    params:
      station: "Olten, Südwest"
      limit: 8

  # Office display - room booking
  "AA:BB:CC:DD:EE:FF":
    screen: floerli
    params:
      room: "Rosa"

  # Lobby display - different bus stop
  "BB:CC:DD:EE:FF:00":
    screen: transit
    params:
      station: "Olten, Bahnhof"
      limit: 6

default_screen: default
```

## Panels Section

Panel profiles define the physical characteristics and measured colors of your e-ink displays. They are used for accurate dithering — the ditherer models what the panel *really* displays, producing better output.

```yaml
panels:
  trmnl_og_4grey:
    name: "TRMNL OG (4-grey)"
    match: "trmnl_og_4grey"
    width: 800
    height: 480
    colors: "#000000,#555555,#AAAAAA,#FFFFFF"
    colors_actual: "#383838,#787878,#B8B8B0,#D8D8C8"

  trmnl_og_4clr:
    name: "TRMNL OG (4-color)"
    match: "trmnl_og_4clr"
    width: 800
    height: 480
    colors: "#000000,#FFFFFF,#FF0000,#FFFF00"
    colors_actual: "#303030,#D0D0C8,#C04040,#D0D020"
```

### Panel Properties

| Property | Required | Description |
|----------|----------|-------------|
| `name` | Yes | Human-readable display name |
| `match` | No | Exact string match against firmware `Board` header for auto-detection |
| `width` | No | Display width in pixels |
| `height` | No | Display height in pixels |
| `colors` | Yes | Official palette colors (comma-separated hex) |
| `colors_actual` | No | Measured/actual colors the panel really displays |
| `dither` | No | Per-panel dither tuning defaults (see below) |

### Panel Dither Defaults

Panels can carry default dither tuning values that apply to all devices using that panel. This avoids repeating the same tuning in every device config entry.

```yaml
panels:
  trmnl_og_4clr:
    name: "TRMNL OG (4-color)"
    colors: "#000000,#FFFFFF,#FF0000,#FFFF00"
    colors_actual: "#303030,#D0D0C8,#C04040,#D0D020"
    dither:
      error_clamp: 0.1         # flat default for all algorithms
      noise_scale: 5.0
      floyd-steinberg:          # per-algorithm override
        error_clamp: 0.08
        noise_scale: 4.0
      atkinson:
        error_clamp: 0.12
```

The `dither` section supports:
- **Flat keys** (`error_clamp`, `noise_scale`, `chroma_clamp`, `strength`): default values for all algorithms
- **Algorithm sub-sections**: per-algorithm overrides that take priority over flat defaults

Resolution within a panel: per-algorithm value > flat default > None.

Algorithm names accept aliases (e.g. `jjn` for `jarvis-judice-ninke`).

The overall tuning priority chain is:

| Priority | Source |
|----------|--------|
| 1 (highest) | Dev UI overrides |
| 2 | Lua script return values |
| 3 | Device config (`error_clamp`, `noise_scale`, `chroma_clamp`, `strength`) |
| 4 | Panel dither defaults |
| 5 (lowest) | Built-in per-algorithm defaults |

### Panel Assignment

Panels are assigned to devices in three ways (highest priority first):

1. **Device config `panel`** — explicit assignment in the `devices` section
2. **Board header auto-detection** — firmware sends a `Board` header, matched against panel `match` patterns
3. **None** — firmware palette header or system defaults

```yaml
devices:
  "ABCDE-FGHJK":
    screen: transit
    panel: trmnl_og_4grey  # explicit panel assignment
```

When a panel has `colors_actual`, the ditherer uses these measured values to model what the display really shows. Use [dev mode](dev-mode.md) to calibrate and find the right measured colors for your panel.

## Customization & File Locations

See [Installation](installation.md) for embedded assets, environment variables,
the `byonk init` command, Docker volume mounts, and file locations.

## Next Steps

- [Understand the architecture](../concepts/architecture.md)
- [Create your first screen](../tutorial/first-screen.md)
