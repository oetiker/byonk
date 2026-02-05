# Codebase Structure

**Analysis Date:** 2026-02-05

## Directory Layout

```
byonk/
├── src/                     # Rust source code
│   ├── api/                 # HTTP request handlers
│   ├── services/            # Business logic (pipeline, rendering, caching)
│   ├── models/              # Data structures and configuration
│   ├── rendering/           # SVG-to-PNG rendering
│   ├── main.rs              # CLI entry points
│   ├── server.rs            # HTTP server setup
│   ├── lib.rs               # Library root
│   ├── error.rs             # Error types
│   └── assets.rs            # Asset loading logic
├── crates/eink-dither/      # Workspace member: e-ink color dithering
├── screens/                 # Lua scripts and SVG templates
│   ├── *.lua                # Screen scripts
│   ├── *.svg                # Screen templates
│   ├── default/             # Built-in default screen assets
│   ├── components/          # Reusable SVG components
│   └── layouts/             # Reusable layout templates
├── fonts/                   # Font files (.ttf, .otf)
├── docs/                    # mdBook documentation
│   └── src/
│       ├── README.md        # Documentation home
│       ├── api/             # API documentation
│       ├── concepts/        # Architecture/design docs
│       ├── guide/           # User guides
│       └── tutorial/        # Tutorials
├── static/                  # Static assets (dev UI, etc.)
│   ├── dev/                 # Dev mode UI files
│   └── svgs/                # SVG assets
├── tests/                   # Integration tests
├── config.yaml              # Example configuration
└── Cargo.toml               # Workspace definition
```

## Directory Purposes

**src/api/**
- Purpose: HTTP request handlers and OpenAPI documentation
- Contains: Handler functions for /api/setup, /api/display, /api/image, /api/log, /api/time endpoints
- Key files:
  - `mod.rs`: Re-exports public handlers
  - `display.rs`: Device display content endpoint (largest, most complex)
  - `setup.rs`: Device registration endpoint
  - `dev.rs`: Development mode UI and hot-reload endpoints
  - `headers.rs`: Header parsing utilities
  - `log.rs`: Device log submission
  - `time.rs`: Time synchronization endpoint

**src/services/**
- Purpose: Core business logic and orchestration
- Contains: Content pipeline, script runtime, template rendering, caching, file watching
- Key files:
  - `content_pipeline.rs`: Main orchestrator (Lua → template → render)
  - `lua_runtime.rs`: Lua script execution with API surface
  - `template_service.rs`: Jinja2 template rendering
  - `renderer.rs`: High-level rendering service wrapper
  - `content_cache.rs`: Rendered SVG caching with LRU eviction
  - `device_registry.rs`: Device session tracking
  - `http_cache.rs`: HTTP response caching for external APIs
  - `file_watcher.rs`: Monitor screen directory for live reload

**src/models/**
- Purpose: Data structures and configuration loading
- Contains: Config parsing, device models, display specifications, auth
- Key files:
  - `config.rs`: AppConfig, ScreenConfig, DeviceConfig parsing from YAML
  - `device.rs`: Device model, ApiKey, Ed25519 signature verification
  - `display_spec.rs`: DisplaySpec enum (OG vs X device dimensions)

**src/rendering/**
- Purpose: SVG to PNG conversion with e-ink palette support
- Contains: SVG rasterization via resvg, color quantization via eink-dither
- Key files:
  - `svg_to_png.rs`: SvgRenderer implementation (SVG parse → RGBA → dither → palette PNG)
  - `mod.rs`: Re-export

**screens/**
- Purpose: User-customizable content definitions
- Contains: Lua data-fetching scripts and SVG rendering templates
- File structure: Screen files pair Lua script with SVG template by name
  - Example: `transit.lua` + `transit.svg` form a complete screen
- Subdirectories:
  - `components/`: Reusable SVG fragments (status bar, icons)
  - `layouts/`: Reusable layout templates
  - `default/`: Assets for the built-in registration screen
- Naming: Kebab-case for directories, no naming convention enforced for screen pairs

**fonts/**
- Purpose: Custom font files available to SVG templates
- Contains: TrueType (.ttf) and OpenType (.otf) files
- Loading: Loaded by AssetLoader at startup, passed to resvg fontdb
- Fallback: System fonts loaded if custom fonts don't cover all requests

**docs/src/**
- Purpose: User and developer documentation
- Built with: mdBook
- Organization:
  - `concepts/`: Architecture, design patterns, content pipeline
  - `guide/`: Configuration, customization, API usage
  - `api/`: Lua API reference, HTTP API reference
  - `tutorial/`: Getting started, examples

**static/**
- Purpose: Web assets served by the HTTP server
- Contains: Dev mode UI HTML/CSS/JS, embedded SVG files
- Subdirectories:
  - `dev/`: Development mode interface (file browser, live rendering preview)
  - `svgs/`: SVG assets for registration screen or examples

**tests/**
- Purpose: Integration tests
- Contains: End-to-end test scenarios, common test utilities
- Key files:
  - `e2e_flow_test.rs`: Full request-response cycles
  - `api_display_test.rs`: Display endpoint behavior
  - `api_setup_test.rs`: Device registration
  - `lua_api_test.rs`: Lua scripting API (comprehensive)
  - `server_integration_test.rs`: Server initialization
  - `common/`: Shared test helpers and fixtures

**crates/eink-dither/**
- Purpose: Workspace member for e-ink color dithering
- Contains: Palette quantization and dithering algorithms
- Integration: Used by src/rendering/svg_to_png.rs
- Supports: Atkinson error diffusion (photo) and blue noise ordered dithering (graphics)

## Key File Locations

**Entry Points:**
- `src/main.rs`: CLI argument parsing and command dispatch
  - `run_server()`: Production server (0.0.0.0:3000 default)
  - `run_dev_server()`: Development server with file watching and UI
  - `run_render_command()`: One-shot rendering for debugging
  - `run_init_command()`: Asset extraction
  - `run_status_command()`: Display configuration info

**Configuration:**
- `config.yaml`: Runtime configuration (screens, devices, registration, auth mode)
- `Cargo.toml`: Project metadata, dependencies, workspace members
- `src/main.rs` (lines 176-189): Environment variable handling (SCREENS_DIR, FONTS_DIR, CONFIG_FILE, BIND_ADDR)

**Core Logic:**
- `src/services/content_pipeline.rs`: Three-stage rendering orchestration
- `src/api/display.rs`: Device display request handling (palette, dithering, caching logic)
- `src/rendering/svg_to_png.rs`: SVG→PNG conversion with dithering
- `src/server.rs`: HTTP router and middleware (tracing, Connection headers)

**Testing:**
- `tests/lua_api_test.rs`: Comprehensive Lua API coverage (78KB file)
- `tests/api_display_test.rs`: Display endpoint scenarios
- `tests/e2e_flow_test.rs`: Full content pipeline tests
- `tests/common/`: Test setup, fixture builders, mock servers

## Naming Conventions

**Files:**
- Source files: `snake_case.rs`
- Test files: `*_test.rs` or in parallel `tests/` directory
- Config files: `config.yaml`, uppercase for generated files (`CHANGES.md`)
- Scripts: `kebab-case.lua` (e.g., `gphoto.lua`)
- Templates: `kebab-case.svg` (e.g., `transit.svg`)

**Directories:**
- Rust modules: `snake_case` (e.g., `src/services/`)
- Screen subdirectories: `kebab-case` (e.g., `screens/components/`, `screens/default/`)
- Generated docs: `book/` (mdBook output)

**Functions:**
- Public API: `snake_case`
- Internal: `snake_case`
- Handler functions: `handle_*` (e.g., `handle_display`, `handle_setup`)
- Constructor methods: `new()`, `with_*()` (e.g., `with_fonts()`)
- Getter/setter: `is_*()`, `get_*()`, no setter convention
- Builder: Method chaining (e.g., `CachedContent::new().with_colors().with_dither()`)

**Types:**
- Public structs: `PascalCase`
- Enums: `PascalCase`
- Trait names: `PascalCase`
- Error types: `*Error` suffix (e.g., `ApiError`, `RenderError`, `ScriptError`)
- Result types: `Result<T, E>` or specialized `type Alias = Result<T, E>`

**Variables:**
- Local variables: `snake_case`
- Constants: `UPPER_SNAKE_CASE`
- Module-level statics: `UPPER_SNAKE_CASE` or lazy static names (OnceLock usage)

## Where to Add New Code

**New Screen (Lua script + SVG template):**
- Script: `screens/my_screen.lua`
- Template: `screens/my_screen.svg`
- Register in: `config.yaml` under `screens:`
  ```yaml
  screens:
    my_screen:
      script: my_screen.lua
      template: my_screen.svg
      default_refresh: 900
  ```
- Test: `tests/lua_api_test.rs` (add test for Lua API if using new globals)

**New API Endpoint:**
- Handler: `src/api/my_endpoint.rs` or add to existing module
- Registration: `src/server.rs` in `build_router()`, add `.route()`
- OpenAPI: Add `#[utoipa::path(...)]` attribute to handler
- Error type: Use `ApiError` or define new variant if needed
- Test: `tests/api_*_test.rs` or new file

**New Service:**
- Implementation: `src/services/my_service.rs`
- Export: Add to `src/services/mod.rs` public use statements
- Integration: Use in ContentPipeline or create wrapper
- Test: In service file with `#[cfg(test)]` module or `tests/`

**Shared Utilities:**
- Helper functions: `src/` root level or in relevant module
- Traits: Define in module where used (e.g., HeaderMapExt in api/headers.rs)
- No dedicated `utils/` directory; keep utilities close to usage

## Special Directories

**src/ root:**
- Purpose: Entry points and module re-exports
- Generated: No
- Committed: Yes
- Files:
  - `main.rs`: Binary entry point
  - `lib.rs`: Library root (re-exports public modules)
  - `server.rs`: HTTP server setup
  - `assets.rs`: Asset loading
  - `error.rs`: Error types

**screens/ and fonts/:**
- Purpose: User-editable assets
- Generated: No (bundled, can be extracted with `byonk init`)
- Committed: Yes (embedded in binary, can override with env vars)
- External sources: SCREENS_DIR, FONTS_DIR env vars allow filesystem override

**static/**
- Purpose: Web assets
- Generated: Dev mode UI is pre-built, not generated at runtime
- Committed: Yes
- Served: By Axum at /static/ prefix in production

**docs/book/**
- Purpose: Generated HTML documentation
- Generated: Yes (from docs/src/ by mdBook)
- Committed: No (generated on demand)
- Build: `cd docs && mdbook serve` or `mdbook build`

**tests/common/**
- Purpose: Shared test infrastructure
- Generated: No
- Committed: Yes
- Contains: Helper functions, mock servers (wiremock), test fixtures

**target/**
- Purpose: Rust build artifacts
- Generated: Yes (cargo build output)
- Committed: No (.gitignore)

