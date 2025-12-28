# Architecture Overview

Byonk is designed as a content server that bridges dynamic data sources with e-ink displays. This page explains how the system is structured and how requests flow through it.

## System Overview

```mermaid
architecture-beta
    group server(server)[Byonk Server]

    service display(internet)[E-ink Display]
    service router(server)[HTTP Router] in server
    service registry(database)[Device Registry] in server
    service signer(disk)[URL Signer] in server
    service lua(server)[Lua Runtime] in server
    service template(disk)[Template Service] in server
    service renderer(disk)[SVG Renderer] in server

    display:R -- L:router
    router:R -- L:registry
    router:R -- L:signer
    router:B -- T:lua
    lua:R -- L:template
    template:R -- L:renderer
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

```mermaid
sequenceDiagram
    box LightBlue TRMNL Device
        participant Device as E-ink Display
    end
    box LightGray Byonk Server
        participant Router as HTTP Router
        participant Registry as Device Registry
        participant Signer as URL Signer
        participant Cache as Content Cache
        participant Lua as Lua Runtime
        participant Template as Template Service
        participant Renderer as SVG Renderer
    end
    box LightYellow External
        participant API as External APIs
    end

    Note over Device,Registry: Phase 1 - Device Registration
    Device->>+Router: GET /api/setup
    Router->>Registry: lookup/create device
    Registry-->>Router: api_key
    Router-->>-Device: {api_key, friendly_id}
    Note right of Device: Store api_key

    Note over Device,Renderer: Phase 2 - Content Generation
    Device->>+Router: GET /api/display
    Router->>Registry: validate token
    Registry-->>Router: device config

    rect rgb(230,245,230)
        Note over Lua,Renderer: Content Pipeline
        Router->>+Lua: execute script
        Lua->>+API: http_get(url)
        API-->>-Lua: JSON data
        Lua-->>-Router: {data, refresh_rate}

        Router->>+Template: render SVG
        Template-->>-Router: SVG document

        Router->>+Renderer: convert to PNG
        Renderer-->>-Router: dithered PNG
    end

    Router->>Cache: store content
    Router->>Signer: sign URL
    Signer-->>Router: signed URL
    Router-->>-Device: {image_url, refresh_rate}

    Note over Device,Cache: Phase 3 - Image Download
    Device->>+Router: GET /api/image/:id
    Router->>Signer: verify signature
    Router->>Cache: get content
    Cache-->>Router: PNG bytes
    Router-->>-Device: PNG image

    Note right of Device: Display and sleep
```

### Request Details

| Phase | Endpoint | Purpose |
|-------|----------|---------|
| **1. Setup** | `GET /api/setup` | Device registers, receives API key |
| **2. Display** | `GET /api/display` | Triggers content generation, returns signed image URL |
| **3. Image** | `GET /api/image/:id` | Downloads cached PNG using signed URL |

The content pipeline (Phase 2) executes these steps:

1. **Load script** — Read `screens/{screen}.lua` from disk
2. **Execute Lua** — Run script with `params` and `device` context
3. **Fetch external data** — Script calls `http_get()` to fetch APIs
4. **Load template** — Read `screens/{screen}.svg` from disk
5. **Render SVG** — Apply `data`, `device`, `params` to Tera template
6. **Rasterize** — Convert SVG to pixels at target resolution
7. **Dither** — Floyd-Steinberg to 4 gray levels
8. **Encode PNG** — 2-bit indexed color, ~10-90KB
9. **Cache** — Store for image fetch request
10. **Sign URL** — HMAC-SHA256 with 1-hour expiry

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
