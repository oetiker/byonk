# Coding Conventions

**Analysis Date:** 2026-02-05

## Naming Patterns

**Files:**
- Modules use snake_case: `lua_runtime.rs`, `content_pipeline.rs`, `device_registry.rs`
- Test files use `_test.rs` suffix: `api_display_test.rs`, `lua_api_test.rs`, `e2e_flow_test.rs`
- Inline module test files end with `#[cfg(test)]` blocks within source files

**Functions:**
- All functions use snake_case: `parse_colors_header()`, `handle_display()`, `run_render_command()`
- Public API handlers prefixed with `handle_`: `handle_display()`, `handle_image()`, `handle_log()`, `handle_setup()`
- Async functions marked with `pub async fn`

**Variables and Constants:**
- Variables use snake_case: `device_id_str`, `api_key_str`, `registration_code`
- Constants use UPPER_SNAKE_CASE: `MAX_DISPLAY_WIDTH`, `DEFAULT_COLORS`, `CODE_CHARS`
- Private module-level constants use all caps: `const DEFAULT_COLORS: &str = ...`

**Types and Enums:**
- Struct names use PascalCase: `ApiError`, `DeviceContext`, `RenderError`, `ContentPipeline`, `LuaRuntime`
- Enum variants use PascalCase: `DeviceModel::OG`, `DeviceModel::X`, `ApiError::MissingHeader`
- Trait names use PascalCase: `DeviceRegistry`, `InMemoryRegistry`

## Code Style

**Formatting:**
- Rust Edition 2021
- 4-space indentation for Rust code
- 2-space indentation for YAML, Lua, SVG (see `.editorconfig`)
- UTF-8 charset with LF line endings
- Trailing whitespace trimmed

**Linting:**
- Clippy is configured and enforced via `make check`
- Some vendored code in `crates/eink-dither/src/lib.rs` suppresses clippy with `#![allow(...)]` for generated content
- Allow decorators used sparingly for legitimate cases (e.g., `#[allow(clippy::arc_with_non_send_sync)]` in `lua_runtime.rs` for Arc in single-threaded context)

**Code Formatting Tools:**
- Built-in via Makefile: `make check` runs `cargo fmt --check && cargo clippy`
- `cargo fmt` is run before every `make build` and `make release`

## Import Organization

**Order:**
1. Standard library imports: `use std::...`
2. External crate imports: `use axum::...`, `use tokio::...`, `use serde::...`
3. Internal crate imports: `use crate::...`, `use super::...`
4. Grouped imports with `{...}` for multiple items from same module

**Example from `src/main.rs`:**
```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;

use byonk::api;
use byonk::models::AppConfig;
use byonk::server;
```

**Path Aliases:**
- No custom path aliases configured
- Full module paths used throughout: `byonk::api`, `byonk::services`, `crate::error`

## Error Handling

**Patterns:**
- Custom error enums derive from `thiserror::Error` for automatic `Display` and `Error` impl
- Error variants documented with `#[error("...")]` attribute
- Enum variants use `#[from]` for automatic conversions: `#[from] RenderError` converts `RenderError` → `ApiError`

**Example from `src/error.rs`:**
```rust
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Missing required header: {0}")]
    MissingHeader(&'static str),

    #[error("Rendering error: {0}")]
    Render(#[from] RenderError),

    #[error("Content error: {0}")]
    Content(String),
}
```

**Error Response Conversion:**
- `ApiError` implements `IntoResponse` to convert to HTTP responses with appropriate status codes
- Response format: JSON with `status` and `error` fields

**Result Types:**
- Use `anyhow::Result<T>` for CLI commands and initialization paths
- Use `Result<T, ApiError>` for HTTP endpoints and API operations
- Use `Result<T, ScriptError>` for Lua script execution

## Logging

**Framework:** `tracing` crate with `tracing-subscriber`

**Patterns:**
- Server initialization uses `tracing_subscriber::registry()` setup
- Different filters for different contexts:
  - Production: `"byonk=debug,tower_http=debug"` (from `src/main.rs`)
  - CLI: `"byonk=warn"` (minimalist)
- Macros used: `tracing::info!()`, `tracing::warn!()`, `tracing::debug!()`
- Structured logging with key=value pairs: `tracing::info!(addr = %bind_addr, "Server listening")`

**When to Log:**
- Server startup/shutdown events
- Configuration loading and validation
- Asset sourcing (embedded vs filesystem)
- File watcher status
- Error conditions with context

## Comments

**When to Comment:**
- File-level documentation as module comments `//!`
- Complex algorithm explanations before logic blocks
- API behavior documentation in doc comments `///`
- Non-obvious design decisions or workarounds
- Avoid redundant comments that just repeat the code

**JSDoc/TSDoc:**
- Uses Rust doc comments `///` for public items
- Examples from `src/models/device.rs`:

```rust
/// Derive a 10-character registration code from any API key by hashing
///
/// The code is deterministic: the same API key always produces the same code.
/// Uses SHA256 hash converted to a 10-character string using CODE_CHARS alphabet.
pub fn registration_code(&self) -> String {
```

- Module-level documentation uses `//!` comments

## Function Design

**Size:**
- Functions are generally 30-80 lines
- Longer functions (like `run_render_command` at 160+ lines) are rare and justified by sequential setup
- Async functions often longer due to state setup and error handling

**Parameters:**
- Use type-level constraints for async generic handlers: `pub async fn handle_display<R: DeviceRegistry>(...)`
- Named struct fields preferred over positional parameters when multiple related values needed
- Derive builder patterns for complex initialization (`DeviceContext` uses field construction)

**Return Values:**
- Return `Result<T, E>` for fallible operations
- Use tuple returns for multiple related outputs: `(svg_content, final_palette)`
- Implement `IntoResponse` for response types to enable `?` operator in handlers

## Module Design

**Exports:**
- `pub mod` declarations in `lib.rs` expose major modules: `api`, `assets`, `error`, `models`, `rendering`, `server`, `services`
- Selective `pub use` in module files for important types: `pub use display::{handle_display, handle_image, DisplayJsonResponse}`
- Private helper functions use `fn` without `pub`

**Barrel Files:**
- `src/api/mod.rs` uses barrel pattern to re-export handlers and response types
- `src/models/mod.rs` exports all model types through single module
- `src/services/mod.rs` exports service types for integration tests

**Example from `src/api/mod.rs`:**
```rust
pub mod dev;
pub mod display;
pub mod log;
pub mod setup;
pub mod time;

pub use dev::DevState;
pub use display::{handle_display, handle_image, DisplayJsonResponse};
pub use log::{handle_log, LogRequest, LogResponse};
```

## Attribute Macros

**Derive Macros:**
- `#[derive(Debug, Clone, Serialize, Deserialize)]` for data structures
- `#[derive(Debug, Error)]` for error types
- Commonly combined: `#[derive(Debug, Deserialize, Clone)]` for config structs

**Serde Attributes:**
- `#[serde(default)]` for optional config fields with default values
- `#[serde(default = "function_name")]` for custom defaults: `#[serde(default = "default_screen")]`

**Axum/Web Attributes:**
- `#[openapi(...)]` for OpenAPI schema generation on the main ApiDoc struct
- `#[derive(OpenApi)]` with paths() and components() for API documentation

**Testing Attributes:**
- `#[cfg(test)]` blocks for inline unit tests
- `#[tokio::test]` for async integration tests
- `#[test]` for simple sync tests

## Visibility and Privacy

**Public Interfaces:**
- API handlers are `pub async fn` for HTTP endpoint mapping
- Model types are `pub struct` or `pub enum` for serialization
- Service constructors are `pub fn new(...)` for dependency injection
- Test utilities in `tests/common/` are `pub` for use across test files

**Private Helpers:**
- Non-public helper functions use `fn` without `pub`
- Private module state in `Self { ... }` struct fields
- Internal implementation details kept in single files when possible

## Code Organization

**Modules by Responsibility:**
- `api/` - HTTP endpoint handlers
- `models/` - Data structures and config
- `services/` - Business logic (Lua runtime, content pipeline, rendering)
- `rendering/` - SVG to PNG conversion
- `error.rs` - Error types and conversion
- `server.rs` - Router and app state setup
- `assets.rs` - Embedded asset loading

---

*Convention analysis: 2026-02-05*
