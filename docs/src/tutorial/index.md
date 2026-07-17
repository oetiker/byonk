# Tutorial

This tutorial series will teach you how to create custom screens for your TRMNL device using Byonk. You'll learn:

1. **[Your First Screen](first-screen.md)** - Create a simple "Hello World" screen
2. **[Lua Scripting](lua-scripting.md)** - Fetch data from APIs and process it
3. **[SVG Templates](svg-templates.md)** - Design beautiful layouts
4. **[Advanced Topics](advanced.md)** - HTML scraping, dynamic refresh, error handling

## Prerequisites

Before starting, make sure you have:

- Byonk [installed and running](../guide/installation.md)
- A text editor for writing Lua and SVG files
- Basic familiarity with programming concepts

## Example Screens

Byonk comes with several example screens you can learn from. Each screen is a
**folder** containing three fixed-name files — `meta.yaml` (title, description,
and its `params:` schema), `script.lua` (the data-fetch logic), and `screen.svg`
(the Tera template). Those folders live inside a **package**, and each screen is
referenced by its qualified `handle/path`. The bundled screens ship in the
embedded `byonk-builtin` package:

### Default Screen
A simple clock display showing time and date. Referenced as `byonk-builtin/default`.

```
screens/default/
├── meta.yaml     - Title, description, params
├── script.lua    - Script
└── screen.svg    - Template
```

### Transit Departures
Real-time public transport departures from Swiss OpenData. Referenced as
`byonk-builtin/useful/swiss-departure-board`.

```
screens/useful/swiss-departure-board/
├── meta.yaml     - Title, description, params
├── script.lua    - Fetches from transport.opendata.ch API
└── screen.svg    - Displays departure list with colors
```

### Room Booking (Web Scrape)
Scrapes a web page to show room availability. Referenced as
`byonk-builtin/example/webscrape`.

```
screens/example/webscrape/
├── meta.yaml     - Title, description, params
├── script.lua    - HTML scraping example
└── screen.svg    - Shows current/upcoming bookings
```

### Display Color Test
Demonstrates the display palette colors available on e-ink. Referenced as
`byonk-builtin/calibration/grey`.

```
screens/calibration/grey/
├── meta.yaml     - Title, description, params
├── script.lua    - Adapts to device palette
└── screen.svg    - Shows palette color swatches and dithering test
```

## Quick Reference

### File Locations

| Type | Location |
|------|----------|
| Package manifest | `screens/byonk-screens.yaml` |
| Screen folder | `screens/<path>/` (each with `meta.yaml`, `script.lua`, `screen.svg`) |
| Configuration | `config.yaml` |
| Custom fonts | `fonts/` |

### Workflow

1. **Create** a screen folder (`meta.yaml` + `script.lua` + `screen.svg`) inside a package
2. **Assign** the screen to a device by its `handle/path` reference in `config.yaml`
3. **Test** by refreshing your device or checking `/swagger-ui`

> **Tip:** `script.lua` and `screen.svg` are loaded fresh on every request. Just save your changes and refresh!

## Ready to Start?

Head to [Your First Screen](first-screen.md) to create your first custom display!
