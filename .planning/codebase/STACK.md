# Technology Stack

**Analysis Date:** 2026-02-05

## Languages

**Primary:**
- Rust 1.88+ (edition 2021) - HTTP server, rendering, Lua runtime, SVG processing

**Secondary:**
- Lua 5.4 - Screen scripts for data fetching and content control
- YAML - Configuration (config.yaml)
- SVG - Display templates
- HTML - E-ink display output

## Runtime

**Environment:**
- Rust + Tokio async runtime

**Package Manager:**
- Cargo
- Lockfile: `Cargo.lock` (present)

## Frameworks

**Core:**
- Axum 0.7 - HTTP server framework
- Tokio 1 - Async runtime with full features
- Tower/Tower-HTTP 0.5-0.6 - HTTP middleware and utilities (filesystem serving, tracing, headers)

**Rendering:**
- Resvg 0.46 (patched from `github.com/oetiker/resvg` branch `skrifa`) - SVG to PNG rendering with variable font support
- Tiny-skia 0.11 - 2D graphics library for rendering
- Fontdb 0.23 (from patched resvg) - Font management with variable font support
- PNG 0.17 - PNG encoding
- Oxipng 10.1.0 - PNG optimization with zopfli and parallel compression

**Scripting:**
- MLua 0.10 with Lua 5.4 (vendored) - Lua runtime for screen scripts
- Tera 1 - Templating for SVG generation from Lua data
- Scraper 0.22 - HTML parsing for web scraping (used in Google Photos integration)

**HTTP Client:**
- Reqwest 0.12 - HTTP client library (blocking API, rustls-tls)
  - Supports TLS, mTLS, custom CA certificates, timeouts, redirects
  - Used by Lua runtime for HTTP requests

**Documentation:**
- Utoipa 5 - OpenAPI schema generation (with Axum extras)
- Utoipa-Swagger-UI 8 - Swagger UI documentation server

**Testing:**
- Wiremock 0.6 - Mock HTTP server for Lua API tests
- Rcgen 0.13 - TLS certificate generation for tests
- Rustls 0.23 - TLS library (with ring) for test HTTPS servers
- Tokio-rustls 0.26 - Tokio async TLS support for tests
- Pretty_assertions 1 - Better test output
- Tempfile 3 - Temporary files/directories for tests

## Key Dependencies

**Critical:**
- Axum 0.7 - Core HTTP server functionality
- MLua 0.10 - Lua script execution for content generation
- Resvg (patched) 0.46 - SVG rendering to raster images
- Tokio 1 - Async runtime enabling concurrent requests
- Tera 1 - Template rendering (SVG generation)

**Serialization:**
- Serde 1 - Serialization framework (derive macros)
- Serde_json 1 - JSON support
- Serde_yaml 0.9 - YAML parsing for config.yaml

**Utilities:**
- Chrono 0.4 - Date/time handling (with serde support)
- Thiserror 2 - Error handling macros
- Anyhow 1 - Error propagation
- Rand 0.8 - Random number generation
- Async-trait 0.1 - Async trait support

**Security & Cryptography:**
- HMAC 0.12 - HMAC authentication
- SHA2 0.10 - SHA-256 hashing
- ED25519-dalek 2 - Ed25519 digital signatures for device authentication
- Hex 0.4 - Hex encoding/decoding
- Rustls-pemfile 2 - PEM certificate parsing

**File System & Watching:**
- Notify 8 - File watching for dev mode live reload
- Rust-embed 8 - Asset embedding with conditional inclusion/exclusion
- Base64 0.22 - Base64 encoding

**Dev Tools:**
- Clap 4 - CLI argument parsing with derive macros
- Tracing 0.1 - Structured logging
- Tracing-subscriber 0.3 - Logging backend with env-filter
- Tokio-stream 0.1 - Tokio utilities for streaming
- Futures-util 0.3 - Async utilities
- Regex 1 - Regular expressions
- Fast_qr 0.13 - QR code generation (SVG output)
- Percent-encoding 2 - URL encoding utilities

**Workspace:**
- Eink-dither (local crate) - High-quality dithering algorithms for e-ink displays

## Configuration

**Environment Variables:**
- `BIND_ADDR` - Server bind address (default: "0.0.0.0:3000")
- `CONFIG_FILE` - Path to config.yaml (default: embedded)
- `SCREENS_DIR` - Directory for custom screen files (default: embedded)
- `FONTS_DIR` - Directory for custom fonts (default: embedded)
- `RUST_LOG` - Logging level (default: "byonk=debug,tower_http=debug")

**Build Configuration:**
- Cargo.toml - Workspace manifest with feature flags
  - MLua feature `lua54` with `vendored` Lua library
  - Reqwest feature `rustls-tls` for TLS
  - Tower-HTTP features: `fs`, `trace`, `set-header`

**Patches (Cargo.lock):**
```toml
[patch.crates-io]
resvg = { git = "https://github.com/oetiker/resvg.git", branch = "skrifa" }
usvg = { git = "https://github.com/oetiker/resvg.git", branch = "skrifa" }
fontdb = { git = "https://github.com/oetiker/resvg.git", branch = "skrifa" }
```

## Platform Requirements

**Development:**
- Rust toolchain 1.88+ (edition 2021)
- Standard POSIX environment (Linux, macOS, Unix)
- For Windows: requires WSL or native support via cargo-win

**Production:**
- Docker container (multi-stage build in `Dockerfile`)
  - Builder stage: `rust:1.88-alpine` with `musl-dev` for static linking
  - Runtime stage: `scratch` (minimal container with only binary, CA certs, and assets)
  - Static binary linked with musl for maximum portability
  - Exposes port 3000

**Runtime Requirements:**
- CA certificates for HTTPS (included in Docker runtime stage)
- ~500MB RAM recommended for concurrent Lua script execution
- Disk space for asset caching (depends on screen complexity)

---

*Stack analysis: 2026-02-05*
