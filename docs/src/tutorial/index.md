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

Byonk comes with several example screens you can learn from:

### Default Screen
A simple clock display showing time and date.

```
screens/default.lua   - Script
screens/default.svg   - Template
```

### Transit Departures
Real-time public transport departures from Swiss OpenData.

```
screens/transit.lua   - Fetches from transport.opendata.ch API
screens/transit.svg   - Displays departure list with colors
```

### Room Booking (Floerli)
Scrapes a web page to show room availability.

```
screens/floerli.lua   - HTML scraping example
screens/floerli.svg   - Shows current/upcoming bookings
```

### Gray Level Test
Demonstrates the 4 gray levels available on e-ink.

```
screens/graytest.lua  - Minimal script
screens/graytest.svg  - Four gray rectangles
```

## Quick Reference

### File Locations

| Type | Location |
|------|----------|
| Lua scripts | `screens/*.lua` |
| SVG templates | `screens/*.svg` |
| Configuration | `config.yaml` |
| Custom fonts | `fonts/` |

### Workflow

1. **Create** a Lua script and SVG template in `screens/`
2. **Define** a screen in `config.yaml`
3. **Assign** the screen to a device
4. **Test** by refreshing your device or checking `/swagger-ui`

> **Tip:** Lua scripts and SVG templates are loaded fresh on every request. Just save your changes and refresh!

## Ready to Start?

Head to [Your First Screen](first-screen.md) to create your first custom display!
