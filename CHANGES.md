# Changelog

All notable changes to Byonk will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### New

### Changed

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
