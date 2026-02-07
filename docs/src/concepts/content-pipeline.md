# Content Pipeline

The content pipeline is how Byonk transforms data into images for e-ink displays. This page explains each stage in detail.

## Pipeline Overview

```mermaid
flowchart TD
    A[Lua Script] -->|JSON data| B[SVG Template]
    B -->|SVG document| C[Cache]
    C -->|cached SVG| D[Renderer]
    D -->|dithered pixels| E[E-ink PNG]
```

| Stage | Input | Processing | Output |
|-------|-------|------------|--------|
| **Lua Script** | API endpoints, params | Fetch data, parse JSON/HTML | Structured data |
| **SVG Template** | Data + device context | Tera templating, layout | SVG document |
| **Cache** | SVG document | Hash content, store | Cached SVG + content hash |
| **Renderer** | Cached SVG | Rasterize, dither to palette | Pixel buffer |
| **E-ink PNG** | Pixel buffer | Encode as greyscale or indexed PNG | Palette PNG |

### Content Change Detection

TRMNL devices use the `filename` field in the `/api/display` response to detect content changes. Byonk computes a SHA-256 hash of the rendered SVG content and returns it as the filename. This means:

- **Same content = same filename**: If your Lua script returns identical data and the template produces the same SVG, the device knows nothing changed
- **Changed content = new filename**: Any change in the rendered SVG (data, template, or device context) produces a new hash

This is why template rendering happens during `/api/display` rather than `/api/image` - the hash must be known before the device decides whether to fetch the image.

## Stage 1: Lua Script Execution

Lua scripts fetch and process data from external sources.

### Input

The script receives a global `params` table from `config.yaml`:

```yaml
# config.yaml
devices:
  "94:A9:90:8C:6D:18":
    screen: transit
    params:
      station: "Olten, Bahnhof"
      limit: 8
```

```lua
-- In your script
local station = params.station  -- "Olten, Bahnhof"
local limit = params.limit      -- 8
```

### Processing

Scripts can:

- **Fetch HTTP data**: APIs, web pages, JSON endpoints
- **Parse content**: JSON decoding, HTML scraping
- **Transform data**: Filter, sort, calculate

```lua
local response = http_get("https://api.example.com/data")
local data = json_decode(response)

local filtered = {}
for _, item in ipairs(data.items) do
  if item.active then
    table.insert(filtered, item)
  end
end
```

### Output

Scripts must return a table with two fields:

```lua
return {
  data = {
    -- Any structure - passed to template
    title = "My Screen",
    items = filtered,
    updated_at = time_format(time_now(), "%H:%M")
  },
  refresh_rate = 300  -- Seconds until next update
}
```

### Refresh Rate

The `refresh_rate` controls when the device fetches new content:

- **Low values** (30-60s): Real-time data (transit, stocks)
- **Medium values** (300-900s): Regular updates (weather, calendar)
- **High values** (3600+s): Static content

> **Tip:** Calculate refresh rates dynamically. For transit, refresh after the next departure:
>
> ```lua
> local seconds_until_departure = departure_time - time_now()
> return {
>   data = departures,
>   refresh_rate = seconds_until_departure + 30
> }
> ```

## Stage 2: Template Rendering

SVG templates use [Tera](https://tera.netlify.app/) syntax (similar to Jinja2).

### Input

The template receives a structured context with three namespaces:

### Template Namespaces

| Namespace | Source | Description |
|-----------|--------|-------------|
| `data.*` | Lua script `data` return | Your script's output |
| `device.*` | Device headers | Battery voltage, RSSI |
| `params.*` | config.yaml | Device-specific params |

### Device Context Variables

These are automatically available under `device.*` (when reported by the device):

| Variable | Type | Description |
|----------|------|-------------|
| `device.battery_voltage` | float | Battery voltage (e.g., 4.12) |
| `device.rssi` | integer | WiFi signal strength in dBm (e.g., -65) |

```svg
<!-- Show battery voltage in header -->
<text x="780" y="30" text-anchor="end">
  {% if device.battery_voltage %}{{ device.battery_voltage | round(precision=2) }}V{% endif %}
</text>
```

> **Note:** Device info is also available in Lua scripts via the `device` global table.

### Syntax

**Variables**:
```svg
<text>{{ data.title }}</text>
<text>{{ data.user.name }}</text>
<text>{{ device.battery_voltage }}V</text>
<text>{{ params.station }}</text>
```

**Loops**:
```svg
{% for item in data.items %}
<text y="{{ 100 + loop.index0 * 30 }}">{{ item.name }}</text>
{% endfor %}
```

**Conditionals**:
```svg
{% if data.error %}
<text fill="red">{{ data.error }}</text>
{% else %}
<text>All good!</text>
{% endif %}
```

### Built-in Filters

| Filter | Usage | Description |
|--------|-------|-------------|
| `truncate` | `{{ data.text \| truncate(length=30) }}` | Truncate with ellipsis |
| `format_time` | `{{ data.ts \| format_time(format="%H:%M") }}` | Format Unix timestamp |
| `length` | `{{ data.items \| length }}` | Get array/object length |

### Output

A complete SVG document ready for rendering.

## Stage 3: SVG to PNG Conversion

The renderer converts SVG to a PNG optimized for e-ink displays.

### Font Handling

1. **Custom fonts** from `fonts/` directory (loaded first)
2. **System fonts** as fallback
3. **Variable fonts** supported via CSS `font-variation-settings`

```svg
<style>
  .title {
    font-family: Outfit;
    font-variation-settings: "wght" 700;
  }
</style>
```

### Scaling

SVGs are scaled to fit the display while maintaining aspect ratio:

- TRMNL OG: 800 × 480 pixels
- TRMNL X: 1872 × 1404 pixels

The image is centered if the aspect ratio doesn't match exactly.

### Palette-Aware Dithering

E-ink displays support a limited color palette (typically 4 grey levels, but also color palettes like black/white/red/yellow). Dithering creates the illusion of more shades by distributing quantization error to neighboring pixels.

Byonk uses the [eink-dither](https://github.com/oetiker/byonk/tree/main/crates/eink-dither) engine which performs color matching in the perceptually uniform **Oklab** color space and processes pixels in **gamma-correct linear RGB**. This produces more accurate color reproduction than naive RGB-space dithering.

### Dither Algorithms

Byonk supports 7 dithering algorithms, selectable per-device or per-script via the `dither` option:

| Algorithm | Value | Best for |
|-----------|-------|----------|
| Blue noise (default) | `"graphics"` | UI content: text, icons, charts |
| Atkinson | `"photo"` or `"atkinson"` | Photographs with moderate detail |
| Floyd-Steinberg | `"floyd-steinberg"` | General-purpose, smooth gradients |
| Jarvis-Judice-Ninke | `"jarvis-judice-ninke"` | Sparse chromatic palettes (least oscillation) |
| Sierra | `"sierra"` | Good quality/speed balance |
| Sierra Two-Row | `"sierra-two-row"` | Lighter weight error diffusion |
| Sierra Lite | `"sierra-lite"` | Fastest error diffusion |

All error diffusion algorithms use blue noise jitter to break "worm" artifacts. Color matching is performed in perceptually uniform **Oklab** space with gamma-correct linear RGB processing.

Set the dither mode per-device in `config.yaml`:

```yaml
devices:
  "ABCDE-FGHJK":
    screen: gphoto
    dither: photo
```

Or per-script by returning `dither` in the Lua result table:

```lua
return {
  data = { ... },
  refresh_rate = 300,
  dither = "photo"
}
```

The priority chain is: **dev UI override** > **script `dither`** > **device config `dither`** > **default (`graphics`)**.

### Dither Tuning

Fine-tune dithering behavior with three parameters, settable per-device in `config.yaml` or per-script in the Lua return table:

| Parameter | Description | Typical range |
|-----------|-------------|---------------|
| `error_clamp` | Limits error diffusion amplitude. Lower values reduce oscillation. | 0.05 – 0.5 |
| `noise_scale` | Blue noise jitter scale. Higher values break worm artifacts more aggressively. | 0.3 – 1.0 |
| `chroma_clamp` | Limits chromatic error propagation. Prevents color bleeding. | 0.5 – 5.0 |

Use [dev mode](../guide/dev-mode.md) to find optimal values interactively, then commit them to `config.yaml` or your Lua script for production use.

Priority chain: **dev UI override** > **script return** > **device config** > **algorithm defaults**.

### Output Format

The final PNG format is chosen automatically based on the palette:

- **Grey palette (≤4 colors)**: Native 2-bit greyscale PNG (4 pixels per byte)
- **Grey palette (5-16 colors)**: Native 4-bit greyscale PNG (2 pixels per byte)
- **Color palette**: Indexed PNG with PLTE chunk (bit depth chosen by palette size)
- **Size validated** against device limits (90KB for OG, 750KB for X)

## Error Handling

If any stage fails, Byonk generates an error screen:

```svg
<svg>
  <rect fill="white" stroke="red" stroke-width="5"/>
  <text>Error: Failed to fetch data</text>
  <text>Will retry in 60 seconds</text>
</svg>
```

This ensures:

- Device always receives valid content
- Error is visible for debugging
- Automatic retry on next refresh

## Performance Considerations

### What's Fast

- Lua script execution (milliseconds)
- Template rendering (milliseconds)
- Simple SVG rendering (10-50ms)

### What's Slower

- HTTP requests (network dependent)
- Complex SVG with many elements (100-500ms)
- Large images or gradients

### Optimization Tips

1. **Minimize HTTP calls** - Cache data in script if possible
2. **Simplify SVG** - Fewer elements = faster rendering
3. **Avoid gradients** - They're converted to dithered patterns anyway
4. **Use appropriate refresh rates** - Don't refresh more often than needed
