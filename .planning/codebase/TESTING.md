# Testing Patterns

**Analysis Date:** 2026-02-05

## Test Framework

**Runner:**
- Tokio test framework for async tests: `#[tokio::test]`
- Default sync test framework for unit tests: `#[test]`
- Run via `cargo test` or `make check`

**Assertion Library:**
- `pretty_assertions` crate for detailed assertion failures
- Custom assertion helpers in `tests/common/assertions.rs`
- Standard Rust `assert!()` and `assert_eq!()` macros

**Run Commands:**
```bash
make check                          # Run fmt, clippy, and all tests
cargo test                          # Run all tests in debug mode
cargo test --release               # Run tests in release mode
cargo test -- --nocapture          # Show test output (logging)
cargo test api_display_test        # Run specific test file
```

## Test File Organization

**Location:**
- Integration tests in `/Users/oetiker/checkouts/byonk/tests/` directory (separate from source)
- Inline unit tests in source files under `#[cfg(test)]` blocks
- Example: `src/error.rs` contains unit tests for error types (lines 80-191)

**Naming:**
- Integration test files: `*_test.rs` suffix (e.g., `api_display_test.rs`, `lua_api_test.rs`)
- Inline test functions: `test_*` prefix (e.g., `test_api_error_missing_header`)
- Common test utilities: `tests/common/` directory with modules

**Structure:**
```
tests/
├── api_display_test.rs              # Display endpoint tests
├── api_image_test.rs                # Image endpoint tests
├── api_log_test.rs                  # Logging endpoint tests
├── api_setup_test.rs                # Setup endpoint tests
├── e2e_flow_test.rs                 # End-to-end workflow tests
├── lua_api_test.rs                  # Lua script API tests (2395 lines)
├── server_integration_test.rs        # Server initialization tests
└── common/                           # Shared test infrastructure
    ├── mod.rs                        # Module re-exports
    ├── app.rs                        # TestApp factory
    ├── assertions.rs                 # Custom assertion helpers
    ├── fixtures.rs                   # Test data and constants
    ├── mock_server.rs                # Mock HTTP server wrapper
    └── mock_https_server.rs          # Mock HTTPS server for TLS tests
```

## Test Structure

**Suite Organization:**
Test files are organized by endpoint/feature with `mod common;` to import shared utilities.

```rust
// From tests/api_display_test.rs
mod common;

use axum::http::StatusCode;
use byonk::services::DeviceRegistry;
use common::{fixtures, fixtures::macs, TestApp};

#[tokio::test]
async fn test_display_auto_registers_device() {
    // Test implementation
}
```

**Patterns:**

1. **Setup Pattern:** Using `TestApp::new()` factory
```rust
let app = TestApp::new();
let api_key = app.register_device(macs::HELLO_DEVICE).await;
```

2. **Request Pattern:** Using `get_with_headers()` helper
```rust
let headers = fixtures::display_headers(macs::HELLO_DEVICE, &api_key);
let response = app
    .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
    .await;
```

3. **Assertion Pattern:** Using custom helpers from `common::assertions`
```rust
common::assert_ok(&response);
let image_url = common::assert_valid_display_response(&response);
```

4. **Teardown:** Automatic via test app lifetime (no explicit cleanup needed)

## Test Types

**Unit Tests:**
- Located in `#[cfg(test)]` blocks within source files
- Test individual functions and error handling
- Example from `src/error.rs`: Tests for error type conversions and Display impl
- Scope: Single function or small closely-related functions
- Focus: Error types, parsing, utility functions

Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_missing_header() {
        let error = ApiError::MissingHeader("Authorization");
        assert_eq!(error.to_string(), "Missing required header: Authorization");
    }
}
```

**Integration Tests:**
- Located in `/Users/oetiker/checkouts/byonk/tests/` directory
- Test endpoint handlers with real or mock dependencies
- Use `TestApp` factory to create application instances
- Example files: `api_display_test.rs`, `api_setup_test.rs`
- Scope: HTTP endpoints, request/response validation, authentication
- Focus: API contracts, error handling, header validation

Example:
```rust
#[tokio::test]
async fn test_display_with_registered_device() {
    let app = TestApp::new();
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    let headers = fixtures::display_headers(macs::HELLO_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_ok(&response);
}
```

**End-to-End Tests:**
- File: `/Users/oetiker/checkouts/byonk/tests/e2e_flow_test.rs` (219 lines)
- Tests complete user workflows across multiple endpoints
- Scope: Device registration → display request → image fetch
- Focus: Full request/response cycle, cross-endpoint consistency

Example:
```rust
#[tokio::test]
async fn test_complete_device_flow() {
    let app = TestApp::new();

    // Step 1: Device registration
    let setup_headers = [("ID", macs::HELLO_DEVICE), ...];
    let setup_response = app.get_with_headers("/api/setup", &setup_headers).await;

    // Step 2: Request display
    let api_key = setup_json["api_key"].as_str().unwrap();
    let display_headers = fixtures::display_headers(macs::HELLO_DEVICE, api_key);

    // Step 3: Fetch rendered image
    let image_response = app.get(image_path).await;
    common::assert_png(&image_response);
}
```

**Lua Script Tests:**
- File: `/Users/oetiker/checkouts/byonk/tests/lua_api_test.rs` (2395 lines)
- Tests Lua API functions exposed to screen scripts
- Two approaches:
  1. **Integration approach:** Run embedded screens through full app
  2. **Unit approach:** Run custom Lua scripts with mock HTTP servers
- Scope: `http_get()`, `http_post()`, `time_now()`, `qr_svg()`, color handling
- Focus: Lua function contracts, error handling, data transformation

Example:
```rust
#[tokio::test]
async fn test_lua_http_get_json() {
    let server = MockHttpServer::start().await;

    server
        .mock_get_json(
            "/api/data",
            serde_json::json!({
                "message": "Hello from mock",
                "count": 42
            }),
        )
        .await;
}
```

## Mocking

**Framework:** `wiremock` crate (version 0.6)

**Mock HTTP Server Wrapper:**
Located in `tests/common/mock_server.rs`, provides convenience methods:

```rust
pub struct MockHttpServer {
    pub server: MockServer,
}

impl MockHttpServer {
    pub async fn start() -> Self { ... }
    pub fn url(&self) -> String { ... }
    pub async fn mock_get_json(&self, endpoint: &str, response: serde_json::Value) { ... }
    pub async fn mock_post_json(&self, endpoint: &str, response: serde_json::Value) { ... }
    pub async fn mock_get_with_params(&self, endpoint: &str, param_name: &str, param_value: &str, response: serde_json::Value) { ... }
    pub async fn mock_with_basic_auth(&self, endpoint: &str, expected_auth: &str, response: serde_json::Value) { ... }
}
```

**Mock HTTPS Server:**
Located in `tests/common/mock_https_server.rs` for TLS testing:
- Uses `rcgen` for certificate generation
- `tokio-rustls` for TLS handling
- Enables testing Lua scripts that make HTTPS requests

**Patterns:**

1. **Mock JSON Response:**
```rust
let server = MockHttpServer::start().await;
server.mock_get_json("/api/data", serde_json::json!({"key": "value"})).await;
```

2. **Mock with Query Parameters:**
```rust
server.mock_get_with_params(
    "/search",
    "q",
    "test",
    serde_json::json!({"results": ["item1", "item2"]})
).await;
```

3. **Get Mock URL:**
```rust
let url = server.url_for("/api/data");  // e.g., "http://127.0.0.1:12345/api/data"
```

**What to Mock:**
- External HTTP APIs (weather, quotes, etc.)
- Third-party services
- Slow/unreliable endpoints
- Services requiring authentication

**What NOT to Mock:**
- Internal HTTP handlers (test with real app)
- File system operations (use `tempfile` crate)
- Lua runtime execution (integration test instead)
- Rendering pipeline (integration test instead)

## Fixtures and Factories

**Test Data:**
Located in `tests/common/fixtures.rs`:

```rust
pub mod macs {
    pub const HELLO_DEVICE: &str = "aa:bb:cc:dd:ee:ff";
    pub const GRAY_DEVICE: &str = "11:22:33:44:55:66";
    pub const TEST_DEVICE: &str = "aa:bb:cc:dd:ee:00";
    pub const UNKNOWN_DEVICE: &str = "ff:ff:ff:ff:ff:ff";
}

pub fn display_headers(mac: &str, api_key: &str) -> HashMap<&'static str, String> {
    // Returns headers for /api/display request
}

pub fn as_str_pairs(headers: &HashMap<&str, String>) -> Vec<(&str, &str)> {
    // Converts HashMap to str pairs for request builder
}
```

**TestApp Factory:**
Located in `tests/common/app.rs`:

```rust
pub struct TestApp {
    router: axum::Router,
    pub registry: Arc<InMemoryRegistry>,
    pub content_cache: Arc<ContentCache>,
}

impl TestApp {
    pub fn new() -> Self { ... }
    pub fn new_without_registration() -> Self { ... }
    pub async fn get(&self, path: &str) -> TestResponse { ... }
    pub async fn get_with_headers(&self, path: &str, headers: &[(&str, &str)]) -> TestResponse { ... }
    pub async fn post_json(&self, path: &str, headers: &[(&str, &str)], body: &str) -> TestResponse { ... }
    pub async fn register_device(&self, mac: &str) -> String { ... }
}
```

**Location:**
- Fixtures: `tests/common/fixtures.rs` - Device MACs, header builders, constants
- Factories: `tests/common/app.rs` - TestApp and TestResponse types
- Both modules imported via `mod common; use common::...;` in test files

## Coverage

**Requirements:** Not enforced (no coverage threshold set)

**View Coverage:**
```bash
# Generate coverage report
cargo tarpaulin --out Html

# Or with llvm-cov
cargo llvm-cov

# Generate with specific output
cargo llvm-cov --html
```

**Coverage Notes:**
- Integration tests in `tests/` cover API endpoints and workflows
- Inline unit tests in `src/` cover error types and utility functions
- Lua script tests cover runtime API exposure
- CLI command path (`main.rs` render/init) lightly tested via integration approach

## Async Testing

**Pattern:**
Use `#[tokio::test]` attribute for async test functions. Automatically sets up runtime.

```rust
#[tokio::test]
async fn test_async_display_request() {
    let app = TestApp::new();
    let response = app.get("/api/display").await;  // await HTTP request
    common::assert_ok(&response);
}
```

**Concurrent Tests:**
- Tests run in parallel by default
- TestApp creates isolated application instances
- No shared state between tests (each creates fresh registry and cache)

**Await Points:**
- HTTP requests: `app.get()`, `app.get_with_headers()`, `app.post_json()`
- Mock server operations: `server.mock_get_json()`, etc.
- Registry operations: `app.register_device()`

## Error Testing

**Pattern:**
Test error conditions using header validation and mock failures.

```rust
#[tokio::test]
async fn test_display_missing_access_token() {
    let app = TestApp::new();

    // Missing Access-Token header
    let headers = [
        ("ID", macs::TEST_DEVICE),
        ("Width", "800"),
    ];

    let response = app.get_with_headers("/api/display", &headers).await;

    common::assert_status(&response, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = response.json();
    assert!(json["error"].as_str().unwrap().contains("Missing required header"));
}
```

**Assertion Helpers from `tests/common/assertions.rs`:**

```rust
pub fn assert_status(response: &TestResponse, expected: StatusCode) { ... }
pub fn assert_ok(response: &TestResponse) { ... }
pub fn assert_json_status(response: &TestResponse, expected_status: u16) { ... }
pub fn assert_valid_display_response(response: &TestResponse) -> String { ... }
pub fn assert_valid_setup_response(response: &TestResponse) { ... }
pub fn assert_skip_update_response(response: &TestResponse) { ... }
pub fn assert_png(response: &TestResponse) { ... }
```

## Response Validation

**Custom TestResponse Type:**

```rust
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    pub fn text(&self) -> String { ... }
    pub fn json(&self) -> serde_json::Value { ... }
    pub fn is_png(&self) -> bool { ... }
}
```

**Patterns:**
- Check HTTP status: `assert_status(&response, StatusCode::OK)`
- Parse JSON: `let json: serde_json::Value = response.json()`
- Validate PNG: `common::assert_png(&response)`
- Check headers: `response.headers.get("content-type")`

## Test Configuration

**Cargo.toml Dependencies:**
```toml
[dev-dependencies]
hyper = "1"
http-body-util = "0.1"
wiremock = "0.6"
rcgen = "0.13"
rustls = { version = "0.23", features = ["ring"] }
rustls-pemfile = "2"
tokio-rustls = "0.26"
hyper-util = { version = "0.1", features = ["server", "tokio"] }
ed25519-dalek = { version = "2", features = ["std", "rand_core"] }
pretty_assertions = "1"
tempfile = "3"
base64 = "0.22"
```

**Test Modules:**
- Each test file declares `mod common;` to import shared utilities
- Tests compile independently but share `common/` module code
- `#![allow(dead_code)]` in `tests/common/mod.rs` to suppress warnings for utilities used in other test files

---

*Testing analysis: 2026-02-05*
