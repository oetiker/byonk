# Changelog

All notable changes to Byonk will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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
