# Lua API Reference

This page documents all functions available to Lua scripts in Byonk.

## Global Variables

### params

A table containing device-specific parameters from `config.yaml`.

```lua
local station = params.station  -- From config.yaml
local limit = params.limit or 10  -- With default
```

**Type:** `table`

### device

A table containing device information (when available).

```lua
-- Check battery level
if device.battery_voltage and device.battery_voltage < 3.3 then
  log_warn("Low battery: " .. device.battery_voltage .. "V")
end

-- Check signal strength
if device.rssi and device.rssi < -80 then
  log_warn("Weak WiFi signal: " .. device.rssi .. " dBm")
end

-- Responsive layout based on device type
if device.width == 1872 then
  -- TRMNL X layout
else
  -- TRMNL OG layout
end
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `mac` | string | Device MAC address (e.g., "AC:15:18:D4:7B:E2") |
| `battery_voltage` | number or nil | Battery voltage (e.g., 4.12) |
| `rssi` | number or nil | WiFi signal strength in dBm (e.g., -65) |
| `model` | string or nil | Device model ("og" or "x") |
| `firmware_version` | string or nil | Firmware version string |
| `width` | number or nil | Display width in pixels (800 or 1872) |
| `height` | number or nil | Display height in pixels (480 or 1404) |
| `board` | string or nil | Board identifier (e.g., "trmnl_og_4clr") |
| `colors` | table or nil | Display palette as hex RGB strings (e.g., {"#000000", "#FFFFFF"}) |

**Type:** `table`

> **Note:** Device fields may be `nil` if the device doesn't report them. Always check before using.

### layout

A table containing pre-computed responsive layout values. These values are automatically calculated based on the device dimensions, making it easy to create screens that work on both TRMNL OG (800×480) and TRMNL X (1872×1404).

```lua
-- Use pre-computed values directly
local margin = layout.margin        -- pixel-aligned margin
local center = layout.center_x      -- screen center X

-- Access display palette
local colors = layout.colors         -- {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local count = layout.color_count     -- 4
local greys = layout.grey_count      -- 4 (colors where R=G=B)
```

**Fields:**

| Field | Type | Description | Default (OG) | Example (X) |
|-------|------|-------------|--------------|-------------|
| `width` | integer | Device width in pixels | 800 | 1872 |
| `height` | integer | Device height in pixels | 480 | 1404 |
| `scale` | number | Scale factor: `min(width/800, height/480)` | 1.0 | 2.34 |
| `center_x` | integer | Horizontal center: `floor(width/2)` | 400 | 936 |
| `center_y` | integer | Vertical center: `floor(height/2)` | 240 | 702 |
| `colors` | table | Display palette as hex RGB strings | {"#000000","#555555","#AAAAAA","#FFFFFF"} | 16 grey values |
| `color_count` | integer | Number of palette colors | 4 | 16 |
| `grey_count` | integer | Number of grey levels (colors where R=G=B) | 4 | 16 |
| `margin` | integer | Standard margin: `floor(20 * scale)` | 20 | 46 |
| `margin_sm` | integer | Small margin: `floor(10 * scale)` | 10 | 23 |
| `margin_lg` | integer | Large margin: `floor(40 * scale)` | 40 | 93 |

**Type:** `table`

> **Note:** All margin values are pre-floored for pixel-aligned positioning.

### fonts

A table of all available font families and their faces. Keyed by family name, each value is an array of face records.

```lua
-- List all font families
for family, faces in pairs(fonts) do
  print(family)  -- "X11Helv", "TerminusTTF", "Outfit", ...
end

-- Query a specific family
for _, face in ipairs(fonts["X11Helv"]) do
  print(face.style)           -- "Normal", "Italic", "Oblique"
  print(face.weight)          -- 400 (number)
  print(face.stretch)         -- "Normal", "Condensed", ...
  print(face.monospaced)      -- true/false
  print(face.post_script_name)-- "X11Helv"
  -- Bitmap strike sizes (sorted ppem values), empty for outline-only fonts
  for _, ppem in ipairs(face.bitmap_strikes) do
    print(ppem)               -- 8, 10, 11, 12, ...
  end
end
```

**Face fields:**

| Field | Type | Description |
|-------|------|-------------|
| `style` | string | `"Normal"`, `"Italic"`, or `"Oblique"` |
| `weight` | number | CSS-style weight (100–900, 400 = normal, 700 = bold) |
| `stretch` | string | `"Normal"`, `"Condensed"`, `"Expanded"`, etc. |
| `monospaced` | boolean | Whether the face is monospaced |
| `post_script_name` | string | PostScript name of the face |
| `bitmap_strikes` | table | Sorted array of available bitmap ppem sizes (empty if none) |

**Type:** `table`

## Layout Helper Functions

These functions help scale values appropriately for different device resolutions.

### scale_font(value)

Scales a font size value by the layout scale factor. Returns a float to preserve precision for font rendering.

```lua
local title_size = scale_font(48)    -- 48.0 on OG, 112.32 on X
local body_size = scale_font(24)     -- 24.0 on OG, 56.16 on X
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `value` | number | Base font size (designed for 800×480) |

**Returns:** `number` - Scaled font size (float)

### scale_pixel(value)

Scales a pixel value by the layout scale factor and floors the result for pixel-aligned positioning.

```lua
local header_y = scale_pixel(70)     -- 70 on OG, 163 on X
local icon_size = scale_pixel(32)    -- 32 on OG, 74 on X
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `value` | number | Base pixel value (designed for 800×480) |

**Returns:** `integer` - Scaled and floored pixel value

### greys(levels)

Generates a grey palette with the specified number of levels. Useful for creating gradients or color swatches that match the device's grey level capability.

```lua
-- Generate palette matching device capability
local palette = greys(layout.grey_levels)

for i, entry in ipairs(palette) do
  print(entry.value)       -- 0-255 grey value
  print(entry.color)       -- "#000000" to "#ffffff"
  print(entry.text_color)  -- "#ffffff" for dark, "#000000" for light
end
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `levels` | integer | Number of grey levels (typically 4 or 16) |

**Returns:** `table` - Array of palette entries

**Palette entry fields:**

| Field | Type | Description |
|-------|------|-------------|
| `value` | integer | Grey value from 0 (black) to 255 (white) |
| `color` | string | Hex color string (e.g., "#808080") |
| `text_color` | string | Contrasting text color ("#ffffff" or "#000000") |

**Example with 4 levels:**

```lua
local palette = greys(4)
-- palette[1] = {value=0,   color="#000000", text_color="#ffffff"}
-- palette[2] = {value=85,  color="#555555", text_color="#ffffff"}
-- palette[3] = {value=170, color="#aaaaaa", text_color="#000000"}
-- palette[4] = {value=255, color="#ffffff", text_color="#000000"}
```

## Example: Responsive Screen

Here's how to create a screen that works on both TRMNL OG and TRMNL X:

```lua
-- Before (manual boilerplate):
local width = device and device.width or 800
local height = device and device.height or 480
local scale = math.min(width / 800, height / 480)
local font_size = math.floor(48 * scale)  -- Wrong: shouldn't floor fonts
local header_y = math.floor(70 * scale)   -- Correct: pixel-aligned

-- After (using helpers):
local font_size = scale_font(48)     -- Preserves precision for fonts
local header_y = scale_pixel(70)     -- Pixel-aligned position
local margin = layout.margin         -- Pre-computed pixel margin
local colors = layout.colors                 -- Display palette colors
```

## HTTP Functions

Byonk provides three HTTP functions: `http_request` (full control), `http_get` (GET shorthand), and `http_post` (POST shorthand).

### http_request(url, options?)

Core HTTP function with full control over the request method and options.

```lua
-- GET request (default)
local response = http_request("https://api.example.com/data")

-- POST with JSON body
local response = http_request("https://api.example.com/users", {
  method = "POST",
  json = { name = "Alice", email = "alice@example.com" }
})

-- PUT request with headers
local response = http_request("https://api.example.com/users/123", {
  method = "PUT",
  headers = { ["Authorization"] = "Bearer " .. params.token },
  json = { name = "Alice Updated" }
})

-- DELETE request
local response = http_request("https://api.example.com/users/123", {
  method = "DELETE",
  headers = { ["Authorization"] = "Bearer " .. params.token }
})
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `url` | string | The URL to fetch |
| `options` | table (optional) | Request options (see below) |

**Options:**
| Name | Type | Default | Description |
|------|------|---------|-------------|
| `method` | string | "GET" | HTTP method: "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD" |
| `params` | table | none | Query parameters (automatically URL-encoded) |
| `headers` | table | none | Key-value pairs of HTTP headers |
| `body` | string | none | Request body as string |
| `json` | table | none | Request body as JSON (auto-serializes, sets Content-Type) |
| `basic_auth` | table | none | Basic auth: `{ username = "...", password = "..." }` |
| `timeout` | number | 30 | Request timeout in seconds |
| `follow_redirects` | boolean | true | Whether to follow HTTP redirects |
| `max_redirects` | number | 10 | Maximum number of redirects to follow |
| `danger_accept_invalid_certs` | boolean | false | Accept self-signed/expired certificates (insecure!) |
| `ca_cert` | string | none | Path to CA certificate PEM file for server verification |
| `client_cert` | string | none | Path to client certificate PEM file for mTLS |
| `client_key` | string | none | Path to client private key PEM file for mTLS |
| `cache_ttl` | number | none | Cache response for N seconds (LRU cache, max 100 entries) |

**Returns:** `string` - The response body

**Throws:** Error if the request fails

**JSON option details:**

The `json` option supports complex nested structures. Tables with sequential integer keys (starting at 1) become JSON arrays; tables with string keys become JSON objects. Use bracket syntax for keys with spaces or special characters:

```lua
http_post("https://api.example.com/data", {
  json = {
    -- Nested objects and arrays
    users = {
      { name = "Alice", tags = {"admin", "user"} },
      { name = "Bob", roles = { level = 2, active = true } }
    },
    -- Keys with spaces or special characters
    ["Content-Type"] = "application/json",
    ["my key with spaces"] = "works fine",
    -- Mixed types
    count = 42,
    enabled = true,
    optional = nil  -- becomes JSON null
  }
})
```

### http_get(url, options?)

Convenience wrapper for GET requests. Same as `http_request` with `method = "GET"`.

```lua
-- Simple usage
local response = http_get("https://api.example.com/data")

-- With query parameters (auto URL-encoded)
local response = http_get("https://api.example.com/search", {
  params = {
    query = "hello world",  -- becomes ?query=hello%20world&limit=10
    limit = 10
  }
})

-- With authentication header
local response = http_get("https://api.example.com/data", {
  headers = { ["Authorization"] = "Bearer " .. params.api_token }
})

-- With basic auth
local response = http_get("https://api.example.com/data", {
  basic_auth = { username = params.user, password = params.pass }
})

-- Accept self-signed certificates (for internal APIs)
local response = http_get("https://internal.example.com/data", {
  danger_accept_invalid_certs = true
})

-- Use custom CA certificate for server verification
local response = http_get("https://internal.example.com/data", {
  ca_cert = "/path/to/ca.pem"
})

-- Mutual TLS (mTLS) with client certificate
local response = http_get("https://secure-api.example.com/data", {
  ca_cert = "/path/to/ca.pem",
  client_cert = "/path/to/client.pem",
  client_key = "/path/to/client-key.pem"
})

-- Cache response for 5 minutes (300 seconds)
-- Useful for APIs with rate limits or data that doesn't change frequently
local response = http_get("https://api.weather.com/current", {
  params = { city = "Zurich" },
  cache_ttl = 300  -- Cache for 5 minutes
})
```

**Response Caching:**

The `cache_ttl` option enables response caching with LRU (Least Recently Used) eviction:

- Responses are cached in memory for the specified number of seconds
- Cache key is based on URL, method, params, headers, and body
- Maximum 100 cached entries; oldest entries are evicted when full
- Cache is shared across all script executions
- Useful for reducing API calls to rate-limited services or slow APIs

```lua
-- First call fetches from API, subsequent calls within 60s use cache
local data = http_get("https://api.example.com/data", { cache_ttl = 60 })
```

### http_post(url, options?)

Convenience wrapper for POST requests. Same as `http_request` with `method = "POST"`.

```lua
-- POST with JSON body
local response = http_post("https://api.example.com/data", {
  json = { key = "value", count = 42 }
})

-- POST with form-like body
local response = http_post("https://api.example.com/data", {
  headers = { ["Content-Type"] = "application/x-www-form-urlencoded" },
  body = "key=value&count=42"
})

-- POST with authentication
local response = http_post("https://api.example.com/data", {
  headers = { ["Authorization"] = "Bearer " .. params.token },
  json = { action = "update" }
})
```

**Example with error handling:**
```lua
local ok, response = pcall(function()
  return http_get("https://api.example.com/data", {
    headers = { ["Authorization"] = "Bearer " .. params.token }
  })
end)

if not ok then
  log_error("Request failed: " .. tostring(response))
end
```

## JSON Functions

### json_decode(str)

Parses a JSON string into a Lua table.

```lua
local data = json_decode('{"name": "Alice", "age": 30}')
print(data.name)  -- "Alice"
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `str` | string | JSON string to parse |

**Returns:** `table` - The parsed JSON as a Lua table

**Notes:**
- JSON arrays become 1-indexed Lua tables
- JSON `null` becomes Lua `nil`

### json_encode(table)

Converts a Lua table to a JSON string.

```lua
local json = json_encode({name = "Bob", items = {1, 2, 3}})
-- '{"name":"Bob","items":[1,2,3]}'
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `table` | table | Lua table to encode |

**Returns:** `string` - JSON representation

**Notes:**
- Tables with sequential integer keys become arrays
- Tables with string keys become objects

## HTML Parsing Functions

### html_parse(html)

Parses an HTML string and returns a document object.

```lua
local doc = html_parse("<html><body><h1>Hello</h1></body></html>")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `html` | string | HTML string to parse |

**Returns:** `Document` - Parsed document object

## Document Methods

### doc:select(selector)

Queries elements using a CSS selector.

```lua
local links = doc:select("a.nav-link")
local items = doc:select("ul > li")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `selector` | string | CSS selector |

**Returns:** `Elements` - Collection of matching elements

**Supported selectors:**
- Tag: `div`, `a`, `span`
- Class: `.classname`
- ID: `#idname`
- Attribute: `[href]`, `[data-id="123"]`
- Combinators: `div > p`, `ul li`, `h1 + p`
- Pseudo-classes: `:first-child`, `:nth-child(2)`

### doc:select_one(selector)

Returns only the first matching element.

```lua
local title = doc:select_one("h1")
if title then
  print(title:text())
end
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `selector` | string | CSS selector |

**Returns:** `Element` or `nil` - First matching element

## Elements Methods

### elements:each(fn)

Iterates over all elements in the collection.

```lua
doc:select("li"):each(function(el)
  print(el:text())
end)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `fn` | function | Callback receiving each element |

## Element Methods

### element:text()

Gets the inner text content.

```lua
local heading = doc:select_one("h1")
local text = heading:text()  -- "Welcome"
```

**Returns:** `string` - Text content

### element:attr(name)

Gets an attribute value.

```lua
local link = doc:select_one("a")
local href = link:attr("href")  -- "https://..."
local class = link:attr("class")  -- "nav-link" or nil
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `name` | string | Attribute name |

**Returns:** `string` or `nil` - Attribute value

### element:html()

Gets the inner HTML.

```lua
local div = doc:select_one("div.content")
local inner = div:html()  -- "<p>Paragraph</p><p>Another</p>"
```

**Returns:** `string` - Inner HTML

### element:select(selector)

Queries descendants of this element.

```lua
local table = doc:select_one("table.data")
local rows = table:select("tr")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `selector` | string | CSS selector |

**Returns:** `Elements` - Matching descendants

### element:select_one(selector)

Returns first matching descendant.

```lua
local row = doc:select_one("tr")
local first_cell = row:select_one("td")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `selector` | string | CSS selector |

**Returns:** `Element` or `nil`

## Time Functions

### time_now()

Returns the current Unix timestamp.

```lua
local now = time_now()  -- e.g., 1703672400
```

**Returns:** `number` - Unix timestamp (seconds since 1970)

### time_format(timestamp, format)

Formats a timestamp into a string using the server's local timezone.

```lua
local now = time_now()
time_format(now, "%H:%M")      -- "14:32"
time_format(now, "%Y-%m-%d")   -- "2024-12-27"
time_format(now, "%A, %B %d")  -- "Friday, December 27"
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `timestamp` | number | Unix timestamp |
| `format` | string | strftime format string |

**Returns:** `string` - Formatted date/time

**Format codes:**

| Code | Description | Example |
|------|-------------|---------|
| `%Y` | Year (4 digit) | 2024 |
| `%y` | Year (2 digit) | 24 |
| `%m` | Month (01-12) | 12 |
| `%d` | Day (01-31) | 27 |
| `%H` | Hour 24h (00-23) | 14 |
| `%I` | Hour 12h (01-12) | 02 |
| `%M` | Minute (00-59) | 32 |
| `%S` | Second (00-59) | 05 |
| `%A` | Weekday name | Friday |
| `%a` | Weekday short | Fri |
| `%B` | Month name | December |
| `%b` | Month short | Dec |
| `%p` | AM/PM | PM |
| `%Z` | Timezone | CET |
| `%%` | Literal % | % |

### time_parse(str, format)

Parses a date string into a Unix timestamp.

```lua
local ts = time_parse("2024-12-27 14:30", "%Y-%m-%d %H:%M")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `str` | string | Date string to parse |
| `format` | string | strftime format string |

**Returns:** `number` - Unix timestamp

**Note:** Uses local timezone for interpretation.

## Asset Functions

### read_asset(path)

Reads a file from the current screen's asset directory.

```lua
-- From hello.lua, reads screens/hello/logo.png
local logo_bytes = read_asset("logo.png")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | string | Relative path within the screen's asset directory |

**Returns:** `string` - Binary file contents

**Throws:** Error if the file cannot be read

**Asset directory convention:**

```
screens/
├── hello.lua         # Script at top level
├── hello.svg         # Template at top level
└── hello/            # Assets for "hello" screen
    ├── logo.png
    └── icon.svg
```

When `read_asset("logo.png")` is called from `hello.lua`, it reads `screens/hello/logo.png`.

**Example: Embedding an image in data:**

```lua
local logo = read_asset("logo.png")
local logo_b64 = base64_encode(logo)

return {
    data = {
        logo_src = "data:image/png;base64," .. logo_b64
    },
    refresh_rate = 3600
}
```

### base64_encode(data)

Encodes binary data (string) to a base64 string.

```lua
local encoded = base64_encode(raw_bytes)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `data` | string | Binary data to encode |

**Returns:** `string` - Base64-encoded string

**Example: Creating a data URI from a local asset:**

```lua
local image_data = read_asset("icon.png")
local data_uri = "data:image/png;base64," .. base64_encode(image_data)
```

**Example: Embedding a remote image:**

```lua
local image_bytes = http_get("https://example.com/photo.png", { cache_ttl = 3600 })
local image_src = "data:image/png;base64," .. base64_encode(image_bytes)
```

See [Embedding Remote Images](../tutorial/advanced.md#embedding-remote-images) for a complete example with error handling.

## URL Encoding Functions

### url_encode(str)

URL-encodes a string for safe use in URLs (query parameters, path segments).

```lua
local encoded = url_encode("hello world")  -- "hello%20world"
local station = url_encode("Zürich, HB")   -- "Z%C3%BCrich%2C%20HB"
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `str` | string | String to URL-encode |

**Returns:** `string` - URL-encoded string

**Example: Building a URL with special characters:**

```lua
local station = params.station  -- "Zürich, HB"
local url = "https://api.example.com/departures?station=" .. url_encode(station)
-- Result: https://api.example.com/departures?station=Z%C3%BCrich%2C%20HB
```

**Note:** When using the `params` option in `http_get`/`http_request`, parameters are automatically URL-encoded. Use `url_encode` only when building URLs manually.

### url_decode(str)

Decodes a URL-encoded string.

```lua
local decoded = url_decode("hello%20world")  -- "hello world"
local station = url_decode("Z%C3%BCrich%2C%20HB")  -- "Zürich, HB"
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `str` | string | URL-encoded string to decode |

**Returns:** `string` - Decoded string

**Throws:** Error if the string contains invalid UTF-8 after decoding

## QR Code Functions

### qr_svg(data, options)

Generates a pixel-aligned QR code as an SVG fragment for embedding in templates. Uses anchor-based positioning with edge margins, so you don't need to calculate the QR code size.

```lua
-- Position QR code in bottom-right corner with 10px margins
local qr = qr_svg("https://example.com", {
  anchor = "bottom-right",
  right = 10,
  bottom = 10,
  module_size = 4
})

-- Centered QR code
local qr = qr_svg("https://example.com", {
  anchor = "center",
  module_size = 5
})

-- Top-left with custom margins
local qr = qr_svg("https://example.com", {
  anchor = "top-left",
  left = 20,
  top = 20,
  module_size = 4,
  ec_level = "H"
})
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `data` | string | Content to encode (URL, text, etc.) |
| `options` | table | Positioning and rendering options (see below) |

**Options:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `anchor` | string | "top-left" | Which corner to anchor: "top-left", "top-right", "bottom-left", "bottom-right", "center" |
| `top` | integer | 0 | Margin from top edge in pixels (for top-* anchors) |
| `left` | integer | 0 | Margin from left edge in pixels (for *-left anchors) |
| `right` | integer | 0 | Margin from right edge in pixels (for *-right anchors) |
| `bottom` | integer | 0 | Margin from bottom edge in pixels (for bottom-* anchors) |
| `module_size` | integer | 4 | Size of each QR module in pixels (recommended: 3-6) |
| `ec_level` | string | "M" | Error correction level: "L" (7%), "M" (15%), "Q" (25%), "H" (30%) |
| `quiet_zone` | integer | 4 | QR quiet zone in modules |

**Anchor and margin combinations:**

| Anchor | Relevant margins |
|--------|------------------|
| `top-left` | `top`, `left` |
| `top-right` | `top`, `right` |
| `bottom-left` | `bottom`, `left` |
| `bottom-right` | `bottom`, `right` |
| `center` | (centered, margins ignored) |

**Returns:** `string` - SVG fragment (`<g>` element with `<rect>` elements)

**Throws:** Error if QR code generation fails or if an invalid anchor is specified.

**Example in template:**

```lua
-- hello.lua
return {
  data = {
    -- QR code anchored to bottom-right with 10px margin
    qr_code = qr_svg("https://www.youtube.com/watch?v=dQw4w9WgXcQ", {
      anchor = "bottom-right",
      right = 10,
      bottom = 10,
      module_size = 4
    })
  },
  refresh_rate = 3600
}
```

```svg
<!-- hello.svg -->
{{ data.qr_code | safe }}
```

**Notes:**
- Screen dimensions are automatically read from `device.width` and `device.height` (defaults to 800x480)
- Use integer values for margins and `module_size` for crisp rendering on e-ink displays
- Module size 3-6 pixels works well for 800x480 displays
- Higher error correction allows the QR code to remain scannable even if partially obscured

## Logging Functions

### log_info(message)

Logs an informational message.

```lua
log_info("Processing request for: " .. station)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `message` | string | Message to log |

**Server output:**
```
INFO script=true: Processing request for: Olten
```

### log_warn(message)

Logs a warning message.

```lua
log_warn("API response was empty")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `message` | string | Message to log |

### log_error(message)

Logs an error message.

```lua
log_error("Failed to parse response: " .. err)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `message` | string | Message to log |

## Script Return Value

Every script must return a table with this structure:

```lua
return {
  data = {
    -- Any data structure
    -- Available in template as data.*
    title = "My Title",
    items = { ... }
  },
  refresh_rate = 300,  -- Seconds until next refresh
  skip_update = false, -- Optional: skip rendering, just check back later
  colors = { "#000000", "#FFFFFF", "#FF0000" },  -- Optional: override display palette
  dither = "photo"     -- Optional: "photo" or "graphics" (default)
}
```

### data

| Field | Type | Description |
|-------|------|-------------|
| `data` | table | Data passed to the Tera template under `data.*` namespace |

The `data` table can contain any Lua values:
- Strings, numbers, booleans
- Nested tables (become objects)
- Arrays (1-indexed tables with sequential keys)

In templates, access this data with the `data.` prefix:
```svg
<text>{{ data.title }}</text>
{% for item in data.items %}...{% endfor %}
```

### refresh_rate

| Field | Type | Description |
|-------|------|-------------|
| `refresh_rate` | number | Seconds until device should refresh |

**Guidelines:**
- **30-60**: Real-time data (transit, stocks)
- **300-900**: Regular updates (weather, calendar)
- **3600+**: Static or slow-changing content

If `refresh_rate` is 0 or omitted, the screen's `default_refresh` from config is used.

### colors

| Field | Type | Description |
|-------|------|-------------|
| `colors` | table or nil | Optional array of hex RGB color strings to override the display palette |

When `colors` is returned by a script, it takes the **highest priority** in the color palette chain:

1. **Script `colors`** (strongest) — returned in the script result table
2. **Device config `colors`** — set per-device in `config.yaml`
3. **Firmware `Colors` header** — sent by device hardware
4. **System default** — `#000000,#555555,#AAAAAA,#FFFFFF`

```lua
-- Force a 3-color palette for this screen
return {
  data = { ... },
  refresh_rate = 300,
  colors = { "#000000", "#FFFFFF", "#FF0000" }
}
```

### dither

| Field | Type | Description |
|-------|------|-------------|
| `dither` | string or nil | Optional dithering mode: `"photo"` or `"graphics"` |

Controls the rendering intent used when converting SVG to the e-ink PNG:

- **`"graphics"`** (default) — Blue noise ordered dithering, optimized for UI content (text, icons, charts)
- **`"photo"`** — Atkinson error diffusion with saturation/contrast boost, optimized for photographs

The dither mode follows a priority chain:

1. **Script `dither`** (strongest) — returned in the script result table
2. **Device config `dither`** — set per-device in `config.yaml`
3. **Default** — `"graphics"`

```lua
-- Use photo dithering for a screen that displays images
return {
  data = { image_url = "..." },
  refresh_rate = 3600,
  dither = "photo"
}
```

### skip_update

| Field | Type | Description |
|-------|------|-------------|
| `skip_update` | boolean | If true, don't update the display - just tell device to check back later |

When `skip_update` is `true`:
- No new image is rendered
- The device keeps its current display content
- The device will check back after `refresh_rate` seconds

This is useful when your data source hasn't changed:

```lua
-- Check if data has changed since last update
local cached_hash = get_data_hash()
local current_data = fetch_data()
local new_hash = compute_hash(current_data)

if cached_hash == new_hash then
  -- No changes - tell device to check back in 5 minutes
  return {
    data = {},
    refresh_rate = 300,
    skip_update = true
  }
end

-- Data changed - render new content
return {
  data = current_data,
  refresh_rate = 300,
  skip_update = false  -- or just omit it
}
```

> **Note:** When `skip_update` is true, the `data` table is ignored since no rendering occurs.

## Standard Lua Functions

Byonk uses Lua 5.4. Standard library functions available include:

### String
- `string.format`, `string.sub`, `string.find`
- `string.match`, `string.gmatch`, `string.gsub`
- `string.upper`, `string.lower`, `string.len`

### Table
- `table.insert`, `table.remove`
- `table.sort`, `table.concat`
- `ipairs`, `pairs`

### Math
- `math.floor`, `math.ceil`, `math.abs`
- `math.min`, `math.max`
- `math.random`

### Other
- `tonumber`, `tostring`, `type`
- `pcall` (for error handling)

**Not available:** File I/O, OS functions, network (except `http_get`)
