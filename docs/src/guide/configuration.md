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

## Embedded Assets

Byonk includes default screens, fonts, and configuration embedded in the binary. This enables zero-config operation:

```bash
# Just run it - embedded defaults work immediately
byonk serve
```

To see what's embedded:

```bash
byonk init --list
```

### Customization Modes

**1. Zero-config (embedded only):**
```bash
byonk serve
# Uses embedded screens, fonts, and config
```

**2. Full customization (env vars + volume mounts):**
```bash
export SCREENS_DIR=/data/screens
export FONTS_DIR=/data/fonts
export CONFIG_FILE=/data/config.yaml
byonk serve
# Empty paths are auto-seeded with embedded defaults
# Then uses external files (with embedded fallback)
```

**3. Extract for editing:**
```bash
byonk init --all
# Extracts embedded assets to ./screens/, ./fonts/, ./config.yaml
```

### Init Command

The `byonk init` command extracts embedded assets to the filesystem:

```bash
# List embedded assets
byonk init --list

# Extract everything
byonk init --all

# Extract specific categories
byonk init --screens
byonk init --fonts
byonk init --config

# Force overwrite existing files
byonk init --all --force

# Extract to custom locations (via env vars)
SCREENS_DIR=/my/screens byonk init --screens
```

### Auto-Seeding

When you set an environment variable pointing to an empty or missing directory, Byonk automatically seeds it with embedded assets on startup:

```bash
# This creates /data/screens with embedded screens on first run
SCREENS_DIR=/data/screens byonk serve
```

### Merge Behavior

External files take precedence over embedded assets:

1. If external file exists → use it
2. If external file is missing → fall back to embedded

This lets you customize individual screens while keeping embedded defaults for others.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SCREENS_DIR` | *(embedded)* | Directory for Lua scripts and SVG templates |
| `FONTS_DIR` | *(embedded)* | Directory for font files |
| `CONFIG_FILE` | *(embedded)* | Path to config.yaml |
| `BIND_ADDR` | `0.0.0.0:3000` | Server listen address |

When a path env var is not set, embedded assets are used exclusively (no filesystem access).

## File Locations

| File | Location | Hot Reload |
|------|----------|------------|
| Configuration | `$CONFIG_FILE` or embedded | No (restart required) |
| Lua scripts | `$SCREENS_DIR/*.lua` or embedded | Yes |
| SVG templates | `$SCREENS_DIR/*.svg` or embedded | Yes |
| Fonts | `$FONTS_DIR/` or embedded | No (restart required) |

## Docker Usage

For Docker, mount volumes and set env vars to enable customization:

```yaml
services:
  byonk:
    image: ghcr.io/oetiker/byonk
    ports:
      - "3000:3000"
    environment:
      - SCREENS_DIR=/data/screens
      - FONTS_DIR=/data/fonts
      - CONFIG_FILE=/data/config.yaml
    volumes:
      - ./data:/data  # Empty on first run = auto-seeded
```

Or run without volumes for pure embedded mode:

```yaml
services:
  byonk:
    image: ghcr.io/oetiker/byonk
    ports:
      - "3000:3000"
    # No volumes = uses embedded assets only
```

## Next Steps

- [Understand the architecture](../concepts/architecture.md)
- [Create your first screen](../tutorial/first-screen.md)
