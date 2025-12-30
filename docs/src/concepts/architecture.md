# Architecture Overview

Byonk is designed as a content server that bridges dynamic data sources with e-ink displays. This page explains how the system is structured and how requests flow through it.

## System Overview

```mermaid
flowchart LR
    Display[TRMNL Display]

    subgraph Server[Byonk Server]
        Router[HTTP Router]
        Registry[(Device Registry)]
        Signer[URL Signer]
        Lua[Lua Runtime]
        Template[Template Service]
        Renderer[SVG Renderer]
    end

    Display --> Router
    Router --> Registry
    Router --> Signer
    Router --> Lua
    Lua --> Template
    Template --> Renderer
```

## Core Components

### HTTP Router

The entry point for all device requests. Built with [Axum](https://github.com/tokio-rs/axum), it handles:

- **Device registration** (`/api/setup`)
- **Content requests** (`/api/display`, `/api/image/:id`)
- **Logging** (`/api/log`)
- **API documentation** (`/swagger-ui`)

### Device Registry

Stores device information in memory:

- MAC address to API key mapping
- Device metadata (firmware version, model, battery level)
- Last seen timestamps

> **Note:** The current implementation uses an in-memory store. Device registrations are lost on restart. The architecture supports adding database persistence in the future.

### URL Signer

Provides security for image URLs using HMAC-SHA256:

- Signs image URLs with expiration timestamps
- Validates signatures on image requests
- Prevents unauthorized access to device content

### Content Pipeline

The heart of Byonk - orchestrates content generation:

1. Looks up screen configuration for the device
2. Executes Lua script with device parameters
3. Renders SVG template with script data
4. Converts SVG to PNG with dithering

### Lua Runtime

Executes Lua scripts in a sandboxed environment:

- HTTP client for fetching external data
- JSON/HTML parsing utilities
- Time functions
- Logging

### Template Service

Renders SVG templates using [Tera](https://tera.netlify.app/):

- Jinja2-style syntax
- Custom filters (`truncate`, `format_time`)
- Fresh loading on each request (hot reload)

### SVG Renderer

Converts SVG to PNG optimized for e-ink:

- Uses [resvg](https://github.com/RazrFalcon/resvg) for rendering
- Loads custom fonts from `fonts/` directory
- Floyd-Steinberg dithering to 4 gray levels
- Outputs 2-bit indexed PNG

## Request Flow

The device-server interaction happens in three phases:

### Phase 1: Device Registration

```mermaid
sequenceDiagram
    participant Device as E-ink Display
    participant Router as HTTP Router
    participant Registry as Device Registry

    Device->>+Router: GET /api/setup
    Router->>Registry: lookup/create device
    Registry-->>Router: api_key
    Router-->>-Device: {api_key, friendly_id}
    Note right of Device: Store api_key
```

### Phase 2: Data Fetching

```mermaid
sequenceDiagram
    participant Device
    participant Router
    participant Lua
    participant API as External API
    participant Cache

    Device->>+Router: GET /api/display
    Router->>+Lua: execute script
    Lua->>+API: http_get(url)
    API-->>-Lua: JSON data
    Lua-->>-Router: {data, refresh_rate}
    Router->>Cache: store data
    Router-->>-Device: {image_url, refresh_rate}
```

### Phase 3: Image Rendering

```mermaid
sequenceDiagram
    participant Device
    participant Router
    participant Cache
    participant Template
    participant Renderer

    Device->>+Router: GET /api/image/:id
    Router->>Cache: get cached data
    Cache-->>Router: {data, context}
    Router->>+Template: render SVG with data
    Template-->>-Router: SVG document
    Router->>+Renderer: convert to PNG
    Renderer-->>-Router: dithered PNG
    Router-->>-Device: PNG image
    Note right of Device: Display and sleep
```

### Request Details

| Phase | Endpoint | Purpose |
|-------|----------|---------|
| **1. Setup** | `GET /api/setup` | Device registers, receives API key |
| **2. Display** | `GET /api/display` | Runs Lua script, caches data, returns signed image URL |
| **3. Image** | `GET /api/image/:id` | Renders SVG template to PNG, returns image |

**Phase 2** (data fetching):
1. Load and execute Lua script with `params` and `device` context
2. Script fetches external data via `http_get()`
3. Cache script output (JSON data)
4. Sign image URL and return to device

**Phase 3** (image rendering):
1. Verify URL signature
2. Retrieve cached data
3. Render SVG template with data
4. Convert SVG to PNG with Floyd-Steinberg dithering
5. Return PNG to device

## Technology Stack

| Component | Technology |
|-----------|------------|
| Web framework | Axum |
| Async runtime | Tokio |
| Scripting | mlua (Lua 5.4) |
| Templating | Tera |
| SVG rendering | resvg (patched for variable fonts) |
| HTTP client | reqwest |
| HTML parsing | scraper |

## Design Principles

### Fresh Loading

Lua scripts and SVG templates are loaded from disk on every request. This enables:

- Live editing during development
- No restart needed for content changes
- Simple deployment (just copy files)

### Blocking Isolation

CPU-intensive operations run in a blocking task pool:

- Lua HTTP requests
- SVG rendering
- Image encoding

This prevents blocking the async event loop.

### Graceful Degradation

If content generation fails, devices receive an error screen rather than nothing. The error message helps debugging while keeping the device functional.

## Security Model

### Signed URLs

Image URLs are signed with HMAC-SHA256:

- 1-hour expiration
- Prevents URL enumeration
- Protects against unauthorized access

### No Authentication Required

The `/api/setup` endpoint is open - any device can register. This matches TRMNL's design where devices self-register.

### Script Sandboxing

Lua scripts run in a controlled environment:

- Only exposed functions are available
- No filesystem access
- No arbitrary code execution
