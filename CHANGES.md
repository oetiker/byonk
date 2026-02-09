# Changelog

All notable changes to Byonk will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### New

- **Perceptual dithering engine**: Vendored [eink-dither](crates/eink-dither/) crate with Oklab color matching, gamma-correct linear RGB processing, and automatic distance metric selection (HyAB+chroma for color palettes, Euclidean OKLab for greyscale). Eight dithering algorithms: `blue-noise` (default), `atkinson`/`photo`, `floyd-steinberg`, `jarvis-judice-ninke`/`jjn`, `sierra`, `sierra-two-row`, `sierra-lite`, `simplex`. All error diffusion algorithms use blue noise kernel jitter to break "worm" artifacts.
- **Panel profiles**: Define display panels in `config.yaml` with official and measured (actual) colors, per-algorithm dither tuning defaults, and auto-detection from firmware `Board` header. Panels can also be assigned per-device. Measured colors let the dithering engine model what the panel really shows. `Measured-Colors` firmware header can override panel profile.
- **Dither tuning**: Fine-tune `error_clamp`, `noise_scale`, and `chroma_clamp` per-panel, per-device, per-script, or via dev UI. Scripts can read pre-resolved tuning via `device.dither` table. Priority chain: dev UI > script > device config > panel defaults > algorithm defaults.
- **Dev mode overhaul**: Dither algorithm dropdown, tuning controls (serpentine, exact absorb, error clamp, noise scale, chroma clamp), panel preview with measured colors, click-to-tune HSL color popup, always-visible console. Dimensions and colors derive from panel profile. All changes auto-refresh. Dev overrides propagate to physical device renders.
- **`preserve_exact` option**: Disable exact color match preservation via Lua (`preserve_exact = false`) or dev UI — forces all pixels through enhancement + dithering.
- **Google Photos screen** (`gphoto`): Display random photos from a shared Google Photos album (HTML scraping, no OAuth).
- **Terminus font demo** (`fontdemo-terminus`): Showcases all 9 embedded bitmap sizes (12–32px) in regular, bold, italic, and bold-italic.
- **`fonts` global in Lua**: Discover available font families, styles, weights, and bitmap strike sizes at runtime.

### Changed

- **PNG output ~27% smaller**: oxipng post-processing with zopfli compression and adaptive filter selection.
- **Config errors are fatal**: `config.yaml` parse errors now abort startup with a clear message instead of silently falling back to defaults.
- **Documentation reorganized**: Consolidated duplicated sections, added panels, dither algorithms, dither tuning, and display calibration docs.

### Fixed

- Exact-match pixels (text, lines, borders) now absorb accumulated dither error, preventing artifacts from bleeding across hard boundaries.
- Device config entries work with auto-discovered screens (`.lua`+`.svg` files not listed in `config.yaml`).
- Dev mode clipboard copy works over plain HTTP.

## 0.11.0 - 2026-02-01

### New

- Added `layout.*` namespace to SVG template context — provides `layout.width`, `layout.height`, `layout.scale`, `layout.grey_count`, and other pre-computed layout values directly in templates without needing Lua to pass them through
- Reusable `components/hinting.svg` include — adaptive font hinting that switches between mono and smooth based on `layout.grey_count`. All built-in screens use it via `{% include "components/hinting.svg" %}`
- Added `layout.grey_count` to Lua API — counts palette colors where R=G=B, useful for conditional font hinting in SVG templates
- X11 bitmap fonts: 26 TTF files with embedded bitmap strikes and autotraced scalable outlines. Proportional families (X11Helv, X11LuSans, X11LuType, X11Term) and fixed-width families grouped by cell width (X11Misc5x–X11Misc12x). Use `font-family="X11Helv" font-size="14"` — the renderer selects the matching bitmap strike automatically.
- Hinting demo screen (`hintdemo`): 9-cell grid comparing hinting engines (auto, native, none) × targets (mono, normal, light)
- Dev mode screen selector now shows configured device IDs under their screens
### Changed

- Dev mode: removed separate Device ID input field, merged into screen selector dropdown
- Device lookup now supports case-sensitive IDs (e.g., `X11Helv`) in addition to MAC addresses

### Fixed

- Dev mode now auto-discovers new screens from the filesystem without requiring a server restart
- Dev mode: fix YAML-to-JSON param conversion that silently dropped string device parameters
- Bitmap font spacing: update resvg to use bitmap strike advances instead of hmtx, fixing character overlap in proportional bitmap fonts
- **HTTP binary responses**: `http_get`/`http_request` now correctly handle binary response data (e.g., images) instead of forcing UTF-8 text decoding, enabling `base64_encode()` to work with fetched images (#3)

## 0.10.0 - 2026-01-30

### New

- **Color Palette Override**: Display colors can now be set per-device in `config.yaml` or per-script via Lua return value
  - Priority chain: Lua script `colors` > device config `colors` > firmware `Colors` header > system default (`#000000,#555555,#AAAAAA,#FFFFFF`)
  - Device config: add `colors: "#000000,#FFFFFF,#FF0000"` to any device entry in `config.yaml`
  - Script return: add `colors = { "#000000", "#FFFFFF", "#FF0000" }` to the Lua return table
- **Device Registration System**: Optional security feature to require explicit approval of new devices before they can display content
  - Enable with `registration.enabled: true` in config.yaml (enabled by default)
  - New devices show a 10-character registration code on screen
  - Registration codes are derived from a SHA256 hash of the device's API key (deterministic)
  - Works with any API key format (TRMNL-issued, custom, etc.) - no WiFi reset required
  - Add the code to `config.devices` section using hyphenated format (e.g., `"ABCDE-FGHJK"`)
  - Registration codes can be used interchangeably with MAC addresses for device identification
  - Registration code available as `device.registration_code` and `device.registration_code_hyphenated` in Lua and templates
  - Optionally set `registration.screen` to use a dedicated screen instead of the default
- **CLI render command**: Added `--colors` option to override display palette and `--registration-code` now simulates enrollment (shows registration screen for unregistered devices)
- **Ed25519 Authentication**: Optional cryptographic device authentication using Ed25519 signatures
  - New `/api/time` endpoint returns server timestamp for signature generation
  - Devices sign `timestamp_ms || public_key` and send `X-Public-Key`, `X-Signature`, `X-Timestamp` headers
  - Server verifies signature and checks timestamp is within ±60 seconds
  - `/api/display` accepts both Ed25519 and API key authentication (dual-accept)
  - Set `auth_mode: ed25519` in config.yaml to advertise Ed25519 mode to devices via `/api/setup`
- **Palette-Aware Color Support**: Full support for color and multi-grey e-ink displays
  - Parse `Colors` and `Board` HTTP headers from firmware for display palette detection
  - Palette-aware blue-noise-modulated Floyd-Steinberg dithering in RGB space
  - BT.709 luminance pre-conversion for greyscale palettes (perceptually correct grey mapping)
  - Smart PNG format: greyscale palettes produce native greyscale PNG; color palettes produce indexed PNG with PLTE
  - Palette exposed to Lua as `layout.colors`, `layout.color_count`, `device.colors`, `device.board`
  - Supports arbitrary palettes: 2-color, 3-color, 4-grey, 6-color, 16-grey, etc.
- **Dev UI Improvements**:
  - Device model dropdown with all known boards and their color palettes
  - Colors input field replaces grey levels selector
  - Device ID field accepts both MAC addresses and registration codes
  - 3D device bezel frame with embossed "BYONK — {device name}" label
  - Magnification lens on hover for all screen models (was X-only)
  - Sunken display effect with inset shadow
### Changed

- `DisplaySpec::from_dimensions` now uses exact requested dimensions instead of snapping to OG/X presets
- `layout.grey_levels` replaced by `layout.colors` and `layout.color_count` in Lua API
- OG display max PNG size increased from 90KB to 200KB to accommodate color indexed PNGs
- Default and graytest screens use luminance-based text contrast for palette swatches
- Small screen (< 400px width) adaptations: doubled title/tagline/footer sizes

### Fixed

- i16 overflow in dither error diffusion (now uses i32 with clamping)
- graytest.svg missing `value` field in bar data

## 0.9.0 - 2026-01-19

### New

- Layout helpers for responsive screen development:
  - Added `layout` global table with pre-computed responsive values (`width`, `height`, `scale`, `center_x`, `center_y`, `grey_levels`, `margin`, `margin_sm`, `margin_lg`)
  - Added `scale_font(value)` helper for scaling font sizes (returns float to preserve precision)
  - Added `scale_pixel(value)` helper for scaling pixel values (returns floored integer for pixel alignment)
  - Added `greys(levels)` helper for generating grey palettes matching device capabilities
- Dev mode: Run `byonk dev` to start server with live reload and device simulator
  - Web-based device simulator at `/dev` showing rendered screens
  - Select any screen from config dropdown
  - Switch between device models (OG 800x480, X 1872x1404) or set custom dimensions
  - Custom parameters JSON editor for testing Lua scripts
  - Live reload: screens automatically re-render when Lua/SVG files change (requires SCREENS_DIR)
  - Error display: Lua and template errors shown in UI
  - MAC address resolution: enter a device MAC to auto-load its configured screen and params
  - Device simulation: battery voltage, RSSI, and timestamp override for testing
  - Grey level selector: test screens with 4-level (OG) or 16-level (X) dithering
  - Pixel inspector lens: hover over rendered image to see magnified view
- TRMNL X 16-grey-level support:
  - Configurable grey levels per device (4 for OG, 16 for X)
  - 4-bit PNG output for 16-level displays
  - Device context includes `grey_levels` field
### Changed

- Updated all example screens (default, hello, transit, floerli, graytest) to use new layout helpers
- Dithering now preserves pixels at exact grey levels (solid colors not dithered), improving UI element quality

### Fixed

## 0.8.2 - 2026-01-16

### New

- Versioned documentation: Documentation now tracks releases with version selector dropdown
- Documentation shows latest stable release by default, with option to switch to dev or older versions
- Dev documentation includes warning banner indicating unreleased content
- Backfill support: Run docs workflow with "backfill" option to rebuild docs for past releases
### Changed

### Fixed

## 0.8.1 - 2026-01-16

## 0.8.0 - 2026-01-16

### New

- Template inheritance: Use `{% extends "layouts/base.svg" %}` to create reusable base layouts with overridable blocks
- Template includes: Use `{% include "components/header.svg" %}` to embed reusable SVG components
- Built-in layout and components: `layouts/base.svg`, `components/header.svg`, `components/footer.svg`, `components/status_bar.svg`
- Lua `url_encode(string)` function: URL-encode strings for safe use in URLs
- Lua `url_decode(string)` function: Decode URL-encoded strings
- HTTP response caching: New `cache_ttl` option for `http_request`/`http_get` to cache responses (LRU cache with max 100 entries)
- Request tracing: Each HTTP request now gets a unique request ID for log correlation
- Header parsing utilities: Internal `HeaderMapExt` trait for cleaner API handler code
### Changed

- Content cache now uses synchronous `std::sync::RwLock` instead of `tokio::sync::RwLock` to avoid nested runtime blocking when called from `spawn_blocking` contexts
- Simplified header parsing in API handlers using new `HeaderMapExt` utilities

### Fixed

- Fixed unbounded cache growth: content cache now uses LRU eviction with max 100 entries to prevent memory leaks in long-running deployments
- Fixed nested runtime blocking in display handler: removed `block_on()` calls inside `spawn_blocking()` which could cause deadlocks under load
- Fixed regex recompilation: image href regex in template service is now compiled once using `OnceLock` instead of on every render call
- Fixed trailing slash compatibility: API endpoints now accept URLs with or without trailing slashes (e.g., `/api/setup/`) for TRMNL firmware 1.6.9+ compatibility
- Removed unused `handle_display_json` function (dead code cleanup)

## 0.7.1 - 2026-01-14

### New

- Integration tests verifying actual TCP connection closure behavior
### Changed

- Refactored server setup into shared `server` module used by both production and tests

### Fixed

- Disabled HTTP keep-alive to prevent connection accumulation from ESP32 clients that request keep-alive but never reuse connections ([firmware PR #274](https://github.com/usetrmnl/trmnl-firmware/pull/274))

## 0.7.0 - 2026-01-13

### New

- New `http_request()` function with full HTTP method control (GET, POST, PUT, DELETE, PATCH, HEAD) and comprehensive options
- New `http_post()` convenience function for POST requests with `body` or `json` options
- `http_get()` and `http_request()` now support: `params` (auto URL-encoded query parameters), `headers`, `body`, `json` (auto-serialized with Content-Type), `basic_auth`, `timeout`, `follow_redirects`, `max_redirects`, and `danger_accept_invalid_certs` (for self-signed/expired certificates)
- TLS certificate options for HTTP functions: `ca_cert` (custom CA for server verification), `client_cert` and `client_key` (mTLS client authentication)
- Comprehensive test suite with 194 tests covering API endpoints, Lua functions, error paths, TLS/HTTPS scenarios, unit tests, and end-to-end flows
- Coverage reporting via `make coverage`, `make coverage-text`, and `make coverage-ci` (lcov format for CI)
- Mock HTTP server infrastructure for testing Lua HTTP functions without external requests
- Mock HTTPS server infrastructure for testing TLS certificate handling (self-signed certs, custom CA, mTLS)
- Unit tests for core modules: config parsing, display specs, error handling, template rendering, asset loading

## 0.6.1 - 2026-01-11

### Changed

- **Breaking:** `qr_svg()` now uses margin-based positioning (`top`, `left`, `right`, `bottom`) instead of absolute `x`, `y` coordinates. Screen dimensions are automatically read from device context.

## 0.6.0 - 2026-01-08

### New

- Lua `qr_svg()` function: Generate pixel-aligned QR codes with anchor-based positioning for embedding in SVG templates. Supports `anchor` option ("top-left", "top-right", "bottom-left", "bottom-right", "center") so you don't need to calculate QR code size for positioning.
### Changed

- Hello screen now includes QR code example demonstrating the `qr_svg()` function with anchor-based positioning
- Documentation: Moved "Understanding the Result" section after QR code instructions in first-screen tutorial
- Documentation: Docker commands now include `--pull always` to ensure users get the latest container image

### Fixed

- Hello tutorial screen now uses bundled Outfit font with explicit weights to render correctly on systems without sans-serif fonts (fixes blank screenshot in docs)

## 0.5.3 - 2026-01-05

### Changed

- Simplified image URLs: removed `?w=...&h=...` query parameters, dimensions are now stored in cache
- Tutorial now includes Step 0 explaining how to set up a workspace with environment variables
### Fixed

- CLI help no longer claims `serve` is the default command (status display is the default)
- Docker container now defaults to `serve` command, so `docker run` starts the server as expected
- Documentation: updated architecture docs to reflect content hash URLs (removed outdated signed URL references)
- Documentation: removed obsolete `w` and `h` query parameters from HTTP API docs

## 0.5.1 - 2026-01-01

### New

- Extended device context: `device.model`, `device.firmware_version`, `device.width`, and `device.height` now available in Lua scripts and SVG templates
- CLI render options: `--battery`, `--rssi`, and `--firmware` flags for testing templates with device data
### Changed

### Fixed

## 0.5.0 - 2026-01-01

### Changed

- Redesigned default screen: cleaner layout with full-bleed background image, large centered BYONK logo with drop shadow, vertical grayscale swatches on left edge, vertical gradient bar on right edge, and compact info bar at bottom
### New

- Embedded assets: All screens, fonts, and config are now embedded in the binary using rust-embed
- Zero-config operation: Binary works standalone without any external files
- `byonk init` command: Extract embedded assets to filesystem for customization (`--screens`, `--fonts`, `--config`, `--all`, `--list`)
- Auto-seeding: When `SCREENS_DIR`, `FONTS_DIR`, or `CONFIG_FILE` env vars are set and the path is empty/missing, embedded assets are automatically extracted there
- Merge behavior: External files override embedded assets, with fallback to embedded for missing files
- New `FONTS_DIR` environment variable for configurable font directory
- Lua `base64_encode(data)` function: Encode binary data to base64 strings
- Lua `read_asset(path)` function: Read files from screen asset directories (`screens/<screen>/`)
- Automatic SVG image resolution: `<image href="logo.png"/>` in templates automatically resolves to `screens/<screen>/logo.png` and embeds as data URI
- Default command shows status: Running `byonk` without arguments displays environment variables and asset sources instead of starting the server
- Simplified image URLs: Use content hash in path (`/api/image/{hash}.png`) instead of signed URLs with expiration

### Changed

- PNG compression improved: uses maximum compression with Paeth filter for smaller file sizes

### Removed

- `URL_SECRET` environment variable and URL signing (replaced by content hash-based URLs)

### Fixed

## 0.4.1 - 2025-12-30

### New

- Content change detection: `/api/display` now returns a content-based hash as the `filename`, allowing TRMNL devices to detect when screen content has actually changed
### Changed

- SVG template rendering now happens during `/api/display` instead of `/api/image`, enabling the content hash to be computed before the device fetches the image
- Content cache now stores pre-rendered SVG instead of raw script data, making `/api/image` faster
- PNG compression improved: uses maximum compression with Paeth filter for smaller file sizes

### Removed

- `URL_SECRET` environment variable and URL signing (replaced by content hash-based URLs)

### Fixed

## 0.4.0 - 2025-12-30

### Changed

- Improved dithering quality: switched to blue-noise-modulated error diffusion with serpentine scanning, reducing visible "worm" artifacts while preserving sharp edges for UI content
### Fixed

## 0.3.2 - 2025-12-30

### Fixed

- Docker image now works: added Cross.toml and RUSTFLAGS for truly static musl binaries
## 0.3.1 - 2025-12-30

### New

- CLI `render` subcommand: `byonk render --mac XX:XX:XX:XX:XX:XX --output file.png` renders screens directly without starting a server
- Hello world tutorial screen (`screens/hello.lua`, `screens/hello.svg`) with screenshot in documentation
### Changed

- `make docs-samples` now uses CLI render command (no server needed)
- PNG output now uses native 2-bit grayscale instead of indexed color for faster firmware decoding
- Standardized documentation diagrams to use appropriate Mermaid types (flowchart, sequenceDiagram)
- Split complex architecture sequence diagram into focused phase-specific diagrams
- Upgraded mdBook to v0.5.2 and mdbook-mermaid to v0.17.0 in CI workflow
- Simplified mermaid setup: removed manual theme/mermaid.js, now managed by mdbook-mermaid

### Fixed

- Release builds now use static musl binaries, fixing glibc/musl mismatch in Docker images

## 0.3.0 - 2025-12-30

### New

- Default screen is now a TV-style test pattern showing device MAC, battery voltage, RSSI, 4 gray levels, gradient for dithering demo, and resolution test bars
- Device MAC address (`device.mac`) now available in both Lua scripts and templates
- Docker image tags for major (`0`), minor (`0.2`), and patch (`0.2.1`) versions
- Documentation migrated to mdBook with mermaid diagrams
- Updated installation docs for Docker and pre-built binaries
- Added CLAUDE.md with project guidelines
### Changed

- Renamed project branding from "TRMNL BOYS" to "Byonk - Bring Your Own Ink"
- Makefile now runs fmt and clippy before build targets
- Updated Makefile for mdBook (removed old rspress/npm targets)
- Faster container builds using pre-built binaries instead of compiling from source
- Switched from OpenSSL to rustls for better cross-platform compatibility
- Refactored content caching: `/api/display` now caches Lua script output (JSON data) instead of rendered PNGs; PNG rendering happens on-demand in `/api/image` when requested
- Removed unused static fallback SVG (`static/svgs/default.svg`) and ContentProvider

### Fixed

## 0.2.2 - 2025-12-29

## 0.2.1 - 2025-12-28

## 0.2.0 - 2025-12-28

## 0.1.0 - 2025-12-28

### New

- Device context (`device.battery_voltage`, `device.rssi`) available in templates and Lua scripts
- Template namespacing: `data.*` (Lua), `device.*` (device info), `params.*` (config)
- `skip_update` support: scripts can return `skip_update = true` to tell device to check back later without rendering new content
- Script-controlled refresh rate: the `refresh_rate` returned by Lua scripts is now properly sent to the device
### Changed

- Content rendering now happens in `/api/display` instead of `/api/image` for better control over refresh timing
- Rendered content is cached between `/api/display` and `/api/image` requests

### Fixed

- Fixed refresh_rate being hardcoded to 900 seconds instead of using script-returned value

## 0.1.0 - 2025-12-27

- Initial release
- Lua scripting support with HTTP, JSON, and HTML parsing
- Tera-based SVG templating
- Variable font support via patched resvg
- Floyd-Steinberg dithering for 4-level grayscale e-ink displays
- Device-specific configuration via config.yaml
- HMAC-SHA256 signed URLs for security
