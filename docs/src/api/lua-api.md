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
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `battery_voltage` | number or nil | Battery voltage (e.g., 4.12) |
| `rssi` | number or nil | WiFi signal strength in dBm (e.g., -65) |

**Type:** `table`

> **Note:** Device fields may be `nil` if the device doesn't report them. Always check before using.

## HTTP Functions

### http_get(url)

Performs an HTTP GET request and returns the response body.

```lua
local response = http_get("https://api.example.com/data")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `url` | string | The URL to fetch |

**Returns:** `string` - The response body

**Throws:** Error if the request fails

**Example with error handling:**
```lua
local ok, response = pcall(function()
  return http_get("https://api.example.com/data")
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

**Example: Creating a data URI:**

```lua
local image_data = read_asset("icon.png")
local data_uri = "data:image/png;base64," .. base64_encode(image_data)
```

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
  skip_update = false  -- Optional: skip rendering, just check back later
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
