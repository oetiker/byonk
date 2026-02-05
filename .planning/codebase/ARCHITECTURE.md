# Architecture

**Analysis Date:** 2026-02-05

## Pattern Overview

**Overall:** Three-stage content pipeline with modular, layered architecture

**Key Characteristics:**
- Request-driven, stateless HTTP server (Axum web framework)
- Declarative content routing via YAML configuration
- Three-stage rendering: Lua script → Jinja2 template → SVG to PNG
- Asset embedding with optional filesystem override
- Device registration with cryptographic authentication support

## Layers

**HTTP API Layer:**
- Purpose: Handle TRMNL device requests and dev mode UI
- Location: `src/api/`
- Contains: Request handlers, validation, error responses
- Depends on: Services (content pipeline, caching, rendering)
- Used by: Axum router in `src/server.rs`
- Key endpoints: `/api/setup`, `/api/display`, `/api/image/:hash`, `/api/log`, `/api/time`

**Service Layer:**
- Purpose: Orchestrate content generation and manage state
- Location: `src/services/`
- Contains: Content pipeline, Lua runtime, template rendering, caching, file watching
- Depends on: Models, assets, rendering
- Used by: API handlers, main.rs CLI commands

**Rendering Layer:**
- Purpose: Convert SVG to PNG with palette-aware dithering
- Location: `src/rendering/`
- Contains: SVG rasterization via resvg, pixel dithering via eink-dither
- Depends on: Font database, color palettes
- Used by: RenderService

**Configuration & Models:**
- Purpose: Define application structure and device state
- Location: `src/models/`
- Contains: Config parsing, device registry, display specs
- Depends on: serde, serde_yaml
- Used by: All layers

**Asset Management:**
- Purpose: Load and serve embedded or external assets
- Location: `src/assets.rs`
- Contains: Embedded file loading, directory seeding, asset enumeration
- Depends on: rust-embed
- Used by: All layers for screens, fonts, config

## Data Flow

**Content Request Flow (Device Display):**

1. Device sends HTTP GET to `/api/display` with MAC address and hardware info
2. `handle_display` validates headers and looks up device in registry/config
3. ContentPipeline looks up screen config by device MAC or registration code
4. LuaRuntime executes Lua script with device config parameters, returns data + metadata
5. TemplateService renders Jinja2 SVG template with data from script
6. SvgRenderer rasterizes SVG to RGBA pixels using resvg with loaded fonts
7. EinkDitherer (from eink-dither crate) dithers RGBA to palette indices
8. PNG encoder packs palette + indexed pixel data
9. PNG cached with content hash, image URL returned in JSON response
10. Device fetches PNG from `/api/image/{hash}`

**Configuration State Management:**
- AppConfig loaded from `config.yaml` at startup (or from CLI)
- Config contains screen definitions, device mappings, registration settings
- Config immutable during runtime (Arc<AppConfig>)
- Device registry tracks sessions (MAC → API key → registration code mapping)

**Caching Strategy:**
- ContentCache stores rendered SVG by content hash (before PNG conversion)
- LRU eviction when cache exceeds 100 entries (oldest first)
- HttpCache stores HTTP responses from external APIs
- Device registration codes memoized via ApiKey derivation

## Key Abstractions

**ContentPipeline:**
- Purpose: Orchestrates the three-stage rendering process for a device
- Examples: `src/services/content_pipeline.rs`
- Pattern: Facade pattern; hides Lua runtime, template service, and renderer complexity
- Public methods: `run_script_for_device()`, `render_svg_from_script()`, `render_png_from_svg()`
- State: Immutable references to config, lua_runtime, template_service, renderer

**DeviceContext:**
- Purpose: Represents device hardware state passed through the pipeline
- Examples: `src/services/content_pipeline.rs` lines 34-56
- Pattern: Value object carrying battery, signal, firmware, dimensions, colors
- Used by: Lua scripts (via globals), templates (via Jinja variables), PNG rendering

**ScriptResult:**
- Purpose: Encapsulates output of Lua script execution
- Examples: `src/services/content_pipeline.rs` lines 10-31
- Pattern: Data transfer object between script and template stages
- Contains: JSON data, refresh_rate, skip_update flag, color/dither overrides

**SvgRenderer:**
- Purpose: High-level SVG→PNG with palette dithering
- Examples: `src/rendering/svg_to_png.rs`
- Pattern: Strategy pattern for dither mode selection (photo vs. graphics)
- Uses: resvg for rasterization, eink-dither for color quantization

**AssetLoader:**
- Purpose: Abstracts embedded vs. filesystem assets
- Examples: `src/assets.rs`
- Pattern: Strategy pattern; selects embedded or external source by env var
- Supports: Screens (Lua + SVG), fonts, config.yaml

## Entry Points

**HTTP Server (`run_server`):**
- Location: `src/main.rs` lines 525-590
- Triggers: `byonk serve` CLI command
- Responsibilities: Create AppState, initialize tracing, bind Axum router, serve requests
- Creates: AssetLoader → AppConfig → RenderService → ContentPipeline → HTTP listener

**Dev Server (`run_dev_server`):**
- Location: `src/main.rs` lines 592-696
- Triggers: `byonk dev` CLI command
- Responsibilities: Same as serve + FileWatcher + Dev UI routes
- Dev features: Live reload on screen file changes, device simulator, browser-based UI

**Render Command (`run_render_command`):**
- Location: `src/main.rs` lines 162-316
- Triggers: `byonk render --mac X --output Y` CLI command
- Responsibilities: Single-shot rendering without server; useful for debugging
- Creates: Same pipeline as server, renders once, exits

**Init Command (`run_init_command`):**
- Location: `src/main.rs` lines 319-395
- Triggers: `byonk init --screens --fonts --all` CLI command
- Responsibilities: Extract embedded assets to filesystem for customization
- Idempotent: Can extract multiple asset categories, skip existing files with `--force`

## Error Handling

**Strategy:** Explicit error types with thiserror for domain errors, anyhow for CLI

**Patterns:**

1. **ApiError (HTTP responses):**
   - Location: `src/error.rs`
   - Variants: MissingHeader, DeviceNotFound, NotFound, Render, Content, Internal
   - Returns: JSON response with HTTP status code
   - Example: Missing "ID" header → 400 Bad Request

2. **RenderError (SVG/PNG failures):**
   - Variants: SvgParse, UnsupportedDimensions, ImageTooLarge, PixmapAllocation, PngEncode, Dither, Io
   - Converted to: ApiError::Render → 500 Internal Server Error
   - Example: Dithering fails → error logged, 500 returned to device

3. **ContentError (Lua/Template):**
   - Variants: Script, Template, Render, ScreenNotFound
   - Converted to: ApiError::Content → 500 Internal Server Error
   - Wrapped script errors preserve underlying Lua error messages

4. **ScriptError (Lua execution):**
   - Variants: Lua (mlua::Error), NotFound (script file)
   - Includes: Line numbers and error messages from Lua interpreter
   - Caught in: LuaRuntime::run_script()

5. **TemplateError (Jinja2 rendering):**
   - Variants: Tera template errors (missing variables, syntax errors)
   - Caught in: TemplateService::render()

## Cross-Cutting Concerns

**Logging:**
- Framework: tracing + tracing-subscriber with env-filter
- Setup: `src/main.rs` lines 529-536 (server), 603-609 (dev)
- Default levels: `byonk=debug,tower_http=debug`
- Request IDs: Custom RequestIdSpan generates 8-char hex ID per request (server.rs lines 24-41)
- Dev mode: File watcher logs when watching activated/deactivated

**Validation:**
- Header parsing: Safe extraction with require_str(), get_parsed_filtered() (api/headers.rs)
- Display dimensions: Bounds checked (800-2000 pixels) to prevent DoS
- Color palette: Parsed from hex strings, validated for RGB ranges
- Registration code: Derived cryptographically from API key + timestamp (models/device.rs)

**Authentication:**
- Methods: API key derivation (default) OR Ed25519 signature verification
- API key: SHA256 hash of "mac:api_key_string:timestamp_ms", first 8 bytes as registration code
- Ed25519: Public key + signature + timestamp sent in headers; verified against known devices
- Both methods: Optional; /api/display accepts whichever is provided
- Setup announces preferred mode: `auth_mode` config field

**Device Registration:**
- Flow: Device sends unknown MAC → shows registration screen with 4-char code
- Code: Derived from API key (mac-specific, time-based derivation)
- User action: Adds device to config.devices with MAC or registration code
- Bypass: Set `registration.enabled: false` in config

