# External Integrations

**Analysis Date:** 2026-02-05

## APIs & External Services

**Data Fetching (via Lua):**
- Generic HTTP support via `http_request()`, `http_get()`, `http_post()` Lua functions
- No hard-coded API integrations; screens implement their own API calls
- HTML scraping capability via Scraper library for parsing fetched content
- **Google Photos** - Shared album scraping (no OAuth)
  - Script: `screens/gphoto.lua`
  - Method: HTML scraping of shared album public pages
  - No authentication required for shared albums
  - Returns random photos from album with error handling and caching

**HTTP Client Features (Lua-accessible):**
- Methods: GET, POST, PUT, DELETE, PATCH, HEAD
- Query parameters with automatic URL encoding
- Request headers customization
- Request/response body handling
- JSON serialization/deserialization
- Basic HTTP authentication
- Configurable timeouts (default 30s)
- Redirect following (configurable, max 10 by default)
- Self-signed certificate acceptance (insecure mode)
- Custom CA certificate verification (`ca_cert` option)
- mutual TLS support (`client_cert`, `client_key` options)
- Response caching with TTL configuration (`cache_ttl` option)

**SDK/Client:**
- Reqwest 0.12 (blocking API) - HTTP client with Rustls TLS
- Scraper 0.22 - HTML parsing
- Custom `http_cache` module for response caching

## Device Communication

**Authentication Schemes:**
1. **API Key** (default mode) - Random hex string per device
   - Generated during `/api/setup` registration
   - Sent as `Access-Token` header in subsequent requests
   - Hyphenated "registration code" format for user display

2. **Ed25519 Digital Signatures** (alternative mode)
   - Public key from device
   - Signature headers: `X-Public-Key`, `X-Signature`, `X-Timestamp`
   - Verified server-side
   - Device firmware must support Ed25519 signing

**Device Discovery:**
- Device registration via `/api/setup` endpoint
- MAC address as unique identifier
- Device model detection ("og" or "x")
- Firmware version tracking
- Board identifier support

**Device Information Reporting:**
- Battery voltage (optional)
- WiFi signal strength (RSSI in dBm, optional)
- Display dimensions (width/height, optional)
- Color palette capability (as hex RGB strings)

## Data Storage

**Databases:**
- Not detected - No persistent database
- In-memory device registry during server runtime
- Device registrations stored in memory (per-instance)

**File Storage:**
- Local filesystem only
- Embedded assets in binary (via rust-embed)
- Optional external paths:
  - Screens: `SCREENS_DIR` env var → falls back to embedded
  - Fonts: `FONTS_DIR` env var → falls back to embedded
  - Config: `CONFIG_FILE` env var → falls back to embedded

**Caching:**
- In-memory response cache for HTTP requests
- Cache key based on URL + method + params + headers + body
- Per-request TTL configuration via Lua `cache_ttl` option
- Implementation: `src/services/http_cache.rs`
- Content cache for rendered SVG/PNG (in-memory during request)
- Location: `src/services/content_cache.rs`

## Authentication & Identity

**Auth Provider:**
- Custom implementation (self-hosted)
- Two supported modes: API Key or Ed25519 signatures
- Device registration in YAML config (`config.devices` map by MAC)
- Optional device registration enforcement via `registration.enabled` flag

**Device Key Storage:**
- In-memory during runtime (lost on restart)
- Optional persistence: Device registry can be backed by external storage (trait-based)
- Location: `src/services/device_registry.rs` (trait interface)

## Monitoring & Observability

**Error Tracking:**
- Not detected - No external error tracking service
- Application-level error handling in `src/error.rs`

**Logs:**
- Structured logging via Tracing framework
- Configurable via `RUST_LOG` env var
- Log levels: debug, info, warn, error
- Output: stdout
- Request tracking with unique request ID per call

**Endpoints for Device Logs:**
- `/api/log` - Device can submit logs
- `/api/time` - Device can query server time
- Location: `src/api/log.rs`, `src/api/time.rs`

## CI/CD & Deployment

**Hosting:**
- Docker container (scratch-based, minimal)
- Multi-stage build for static binary
- Requires: BIND_ADDR environment variable for configuration

**CI Pipeline:**
- GitHub Actions (repository at `github.com/oetiker/byonk`)
- Workflows: `.github/workflows/`
  - `release.yml` - Release workflow triggered via workflow_dispatch
    - Version bumping (major/feature/bugfix)
    - Cargo build and test
    - Binary cross-compilation
    - Docker image build
    - GitHub release creation
  - `ci.yml` - Continuous integration checks
  - `docs.yml` - Documentation building (mdBook)

## Environment Configuration

**Required Environment Variables:**
- `BIND_ADDR` - Server listen address (optional, default: "0.0.0.0:3000")

**Optional Environment Variables:**
- `CONFIG_FILE` - Path to YAML configuration (default: embedded config.yaml)
- `SCREENS_DIR` - Directory with custom Lua/SVG screen files (default: embedded screens/)
- `FONTS_DIR` - Directory with custom font files (default: embedded fonts/)
- `RUST_LOG` - Logging level filter (default: "byonk=debug,tower_http=debug")

**Secrets Location:**
- API keys generated per device (random hex strings)
- Ed25519 keypairs managed by device firmware (not server-side)
- No credential files required; everything env-var driven

## Webhooks & Callbacks

**Incoming Webhooks:**
- Not applicable - Byonk is a content pull server, not webhook-based
- Devices pull content via `/api/display` polling

**Outgoing Webhooks:**
- Not detected - No outbound webhook notifications
- Device requests are stateless (pull model)

## Device Polling & Refresh

**Device Update Cycle:**
1. Device registers via `/api/setup` (once per boot)
2. Device polls `/api/display` on schedule
3. Server returns `refresh_rate` (seconds) and content hash
4. Device fetches image via `/api/image/{hash}` if needed
5. Device can submit logs via `/api/log`

**Refresh Rate Control:**
- Server-side: Script return value `refresh_rate`
- Device-side: Can override via `Refresh-Rate` header
- Fallback: 15 minutes (900 seconds) default from config

## File Format Support

**Supported Script/Template Inputs:**
- Lua 5.4 (scripts): `*.lua` files
- SVG (templates): `*.svg` files
- YAML (configuration): `config.yaml`
- TrueType fonts: `*.ttf`, `*.otf`
- HTML (for scraping): Generic HTML documents

**Output Formats:**
- PNG (8-bit palette) - Sent to devices
- SVG (intermediate) - Generated by templating, input to renderer
- JSON - API responses

---

*Integration audit: 2026-02-05*
