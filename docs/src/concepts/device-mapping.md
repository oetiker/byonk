# Device Mapping

Byonk allows you to show different content on different TRMNL devices. This page explains how devices are identified, registered, and mapped to screens.

## How Devices Are Identified

Each TRMNL device has a unique **MAC address** that identifies it. This address is sent in the `ID` header with every request:

```
ID: 94:A9:90:8C:6D:18
```

Byonk uses this MAC address to:

1. Register new devices
2. Look up existing device configuration
3. Map devices to screens

## Device Registration Flow

```mermaid
sequenceDiagram
    participant Device
    participant Byonk

    Device->>Byonk: GET /api/setup<br/>Headers: ID, FW-Version, Model
    Byonk-->>Device: {api_key, friendly_id}
    Note right of Device: Store api_key

    Device->>Byonk: GET /api/display<br/>Headers: Access-Token, ID
    Byonk-->>Device: {image_url, refresh_rate}
```

### Setup Response

```json
{
  "status": 200,
  "api_key": "a1b2c3d4e5f6...",
  "friendly_id": "abc123def456"
}
```

- **api_key**: Authentication token for subsequent requests
- **friendly_id**: Human-readable identifier (12 hex characters)

## Configuration-Based Mapping

Devices are mapped to screens in `config.yaml`:

```yaml
devices:
  "94:A9:90:8C:6D:18":
    screen: transit
    params:
      station: "Olten, Bahnhof"

  "AA:BB:CC:DD:EE:FF":
    screen: weather
    params:
      city: "Zurich"

default_screen: default
```

### Lookup Order

When a device requests content:

1. **Exact MAC match** - Check if MAC is in `devices` section
2. **Default screen** - Use `default_screen` if no match
3. **Error** - Return error if no default configured

### MAC Address Format

MAC addresses in config must be:

- **Uppercase**: `"94:A9:90:8C:6D:18"` not `"94:a9:90:8c:6d:18"`
- **Colon-separated**: `"94:A9:90:8C:6D:18"` not `"94-A9-90-8C-6D-18"`
- **Quoted**: YAML requires quotes for strings with colons

## Device Parameters

Each device can have custom parameters passed to its screen's Lua script:

```yaml
devices:
  "94:A9:90:8C:6D:18":
    screen: transit
    params:
      station: "Olten, Südwest"
      limit: 8
      show_delays: true
```

In the Lua script:

```lua
local station = params.station      -- "Olten, Südwest"
local limit = params.limit          -- 8
local show_delays = params.show_delays  -- true
```

### Parameter Types

You can use any YAML type:

| Type | YAML | Lua |
|------|------|-----|
| String | `name: "Alice"` | `params.name` → `"Alice"` |
| Number | `count: 42` | `params.count` → `42` |
| Float | `temp: 21.5` | `params.temp` → `21.5` |
| Boolean | `enabled: true` | `params.enabled` → `true` |
| List | `items: [a, b]` | `params.items[1]` → `"a"` |
| Map | `user: {name: Bob}` | `params.user.name` → `"Bob"` |

### Same Screen, Different Parameters

Multiple devices can use the same screen with different parameters:

```yaml
devices:
  # Kitchen - shows nearby bus stop
  "94:A9:90:8C:6D:18":
    screen: transit
    params:
      station: "Olten, Südwest"

  # Office - shows train station
  "AA:BB:CC:DD:EE:FF":
    screen: transit
    params:
      station: "Olten, Bahnhof"
      limit: 10

  # Lobby - shows airport
  "BB:CC:DD:EE:FF:00":
    screen: transit
    params:
      station: "Zürich Flughafen"
      limit: 6
```

## Finding Your Device's MAC Address

The MAC address is shown:

1. **In Byonk logs** when the device connects:
   ```
   INFO Device registered device_id="94:A9:90:8C:6D:18"
   ```

2. **On the device** during setup (check TRMNL documentation)

3. **In your router's** connected devices list

## Default Screen

The `default_screen` provides a fallback for:

- Devices not yet configured
- New devices during testing
- Backup if config is incorrect

```yaml
default_screen: default
```

If no `default_screen` is set and a device isn't in the config, it receives an error response.

## Auto-Registration

Byonk automatically registers new devices on their first `/api/setup` call:

1. Generates a random API key
2. Generates a friendly ID
3. Stores device in registry

No pre-configuration is needed - just add the device to `config.yaml` to assign a custom screen.

## Multiple Screens per Device?

Currently, each device shows one screen. However, you can create a "dashboard" screen that combines multiple data sources:

```lua
-- dashboard.lua
local weather = fetch_weather()
local transit = fetch_transit()
local calendar = fetch_calendar()

return {
  data = {
    weather = weather,
    transit = transit,
    calendar = calendar
  },
  refresh_rate = 300
}
```

## Device Metadata

Byonk tracks additional device information from request headers:

| Header | Description |
|--------|-------------|
| `FW-Version` | Firmware version |
| `Model` | Device model (og, x) |
| `Battery-Voltage` | Battery level |
| `RSSI` | WiFi signal strength |
| `Width`, `Height` | Display dimensions |

This metadata is stored in the device registry and can be used for:

- Debugging connectivity issues
- Monitoring battery levels
- Adapting content to device model

## Persistence

> **Warning:** The current implementation stores device registrations in memory. Registrations are lost on server restart.
>
> Devices will automatically re-register on their next request, but any collected metadata is lost.

Future versions may add database persistence for device data.
