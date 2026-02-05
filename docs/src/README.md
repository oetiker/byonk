# Byonk

**Bring Your Own Ink** - Self-hosted content server for TRMNL e-ink devices

## Features

- **Lua Scripting** - Fetch data from any API, scrape websites, process JSON - all with simple Lua scripts.
- **SVG Templates** - Design pixel-perfect screens using SVG with Tera templating (Jinja2-style syntax).
- **Variable Fonts** - Full support for variable font weights via CSS font-variation-settings.
- **Smart Refresh** - Scripts control when devices refresh - optimize for fresh data and battery life.
- **Palette-Aware Dithering** - Perceptually correct Oklab dithering with two rendering intents (Graphics and Photo), supporting greyscale and color palettes.
- **Device Mapping** - Assign different screens to different devices via simple YAML configuration.

## Quick Start

```bash
# Run with Docker
docker run -d -p 3000:3000 ghcr.io/oetiker/byonk:latest
```

Or download a [pre-built binary](https://github.com/oetiker/byonk/releases) for your platform.

Point your TRMNL device to `http://your-server:3000` and it will start displaying content.

![Default screen](images/default.png)

## How It Works

```mermaid
flowchart LR
    A[Lua Script] --> B[SVG Template] --> C[Dithering] --> D[TRMNL PNG]
```

1. **Lua scripts** fetch data from APIs or scrape websites
2. **SVG templates** render the data into beautiful layouts
3. **Renderer** converts SVG to dithered PNG optimized for e-ink
4. **Device** displays the content and sleeps until next refresh

## Example: Transit Departures

**Lua Script** fetches real-time data:
```lua
local response = http_get("https://transport.opendata.ch/v1/stationboard?station=Olten")
local data = json_decode(response)

return {
  data = { departures = data.stationboard },
  refresh_rate = 60
}
```

**SVG Template** renders the display:
```svg
<svg viewBox="0 0 800 480">
  {% for dep in departures %}
  <text y="{{ 100 + loop.index0 * 40 }}">
    {{ dep.category }}{{ dep.number }} → {{ dep.to }}
  </text>
  {% endfor %}
</svg>
```

**Result on e-ink display:**

![Transit departures screen](images/transit.png)

## Next Steps

- [Installation Guide](guide/installation.md) - Set up Byonk on your server
- [Architecture](concepts/architecture.md) - Understand how Byonk works
- [Create Your First Screen](tutorial/first-screen.md) - Build a custom display
- [API Reference](api/http-api.md) - HTTP and Lua API documentation
