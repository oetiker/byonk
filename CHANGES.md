# Changelog

All notable changes to Byonk will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### New

- TV-style test pattern (graytest.svg) with 4 gray levels, gradients for dithering demo, and resolution test bars
- Docker image tags for major (`0`), minor (`0.2`), and patch (`0.2.1`) versions
- Documentation migrated to mdBook with mermaid diagrams
- Updated installation docs for Docker and pre-built binaries

### Changed

- Renamed project branding from "TRMNL BOYS" to "Byonk - Bring Your Own Ink"
- Faster container builds using pre-built binaries instead of compiling from source
- Switched from OpenSSL to rustls for better cross-platform compatibility

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
