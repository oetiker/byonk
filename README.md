# Byonk

**Bring Your Own Ink** - A self-hosted content server for [TRMNL](https://usetrmnl.com) e-ink devices.

Byonk lets you create custom screens for your TRMNL device using Lua scripts and SVG templates. Fetch data from any source, render it beautifully, and display it on your e-ink screen.

## Features

- **Lua Scripting** - Fetch data from APIs, scrape websites, process JSON
- **SVG Templates** - Design screens with Tera templating (Jinja2-style)
- **Variable Fonts** - Full support for variable font weight/width via CSS
- **4-Level Grayscale** - Floyd-Steinberg dithering optimized for e-paper
- **Dynamic Refresh** - Scripts control when the device should refresh
- **Device Mapping** - Assign different screens to different devices

## Quick Start

```bash
# Clone and build
git clone https://github.com/oetiker/byonk.git
cd byonk
cargo build --release

# Run the server
./target/release/byonk
```

Point your TRMNL device to `http://your-server:3000` and it will start displaying content.

## Configuration

Edit `config.yaml` to define screens and map devices:

```yaml
screens:
  transit:
    script: transit.lua      # Lua script for data
    template: transit.svg    # SVG template
    default_refresh: 60      # Fallback refresh rate (seconds)

devices:
  "94:A9:90:8C:6D:18":       # Device MAC address
    screen: transit
    params:
      station: "Olten, Bahnhof"
      limit: 8

default_screen: default
```

## Creating Screens

### Lua Script (`screens/example.lua`)

Scripts fetch data and return it along with a refresh rate:

```lua
-- Fetch JSON from an API
local response = http_get("https://api.example.com/data")
local data = json_decode(response)

-- Return data for the template
return {
  data = {
    title = data.title,
    items = data.items,
    updated = time_format(time_now(), "%H:%M")
  },
  refresh_rate = 300  -- Refresh in 5 minutes
}
```

### SVG Template (`screens/example.svg`)

Templates use Tera syntax (similar to Jinja2):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480">
  <text x="20" y="40" font-size="24">{{ title }}</text>

  {% for item in items %}
  <text x="20" y="{{ 80 + loop.index0 * 30 }}">{{ item.name }}</text>
  {% endfor %}

  <text x="780" y="470" text-anchor="end" fill="gray">{{ updated }}</text>
</svg>
```

## Lua API

### HTTP
- `http_get(url)` - Fetch URL, returns body as string

### JSON
- `json_decode(str)` - Parse JSON string to Lua table
- `json_encode(table)` - Encode Lua table to JSON string

### HTML Parsing
- `html_parse(html)` - Parse HTML, returns document
- `doc:select(selector)` - CSS selector query, returns elements
- `elements:each(fn)` - Iterate over elements
- `element:text()` - Get inner text
- `element:attr(name)` - Get attribute value

### Time
- `time_now()` - Current Unix timestamp
- `time_parse(str, format)` - Parse date string
- `time_format(ts, format)` - Format timestamp

### Logging
- `log_info(msg)`, `log_warn(msg)`, `log_error(msg)`

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `SVG_DIR` | `./static/svgs` | Static SVG directory |
| `SCREENS_DIR` | `./screens` | Lua scripts and templates |
| `CONFIG_FILE` | `./config.yaml` | Configuration file |
| `URL_SECRET` | (random) | HMAC secret for signed URLs |

## API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /api/setup` | Device registration |
| `GET /api/display` | Get display content (JSON with image URL) |
| `GET /api/image/:device_id` | Get rendered PNG image |
| `POST /api/log` | Device log submission |
| `GET /swagger-ui` | OpenAPI documentation |
| `GET /health` | Health check |

## Dependencies

Byonk uses a [patched version of resvg](https://github.com/oetiker/resvg/tree/varfont-support) for variable font support via CSS `font-variation-settings`.

## License

MIT License - see [LICENSE](LICENSE)
