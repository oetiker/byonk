# Installation

Byonk can be installed via Docker container or pre-built binaries. All screens, fonts, and configuration are embedded in the binary, so it works out of the box with zero configuration.

## Quick Start

```bash
# Just run it - embedded assets work immediately
docker run --pull always -p 3000:3000 ghcr.io/oetiker/byonk:latest
```

That's it! The server is running with embedded default screens.

## Docker (Recommended)

### Zero-Config Mode

The simplest way to run Byonk:

```bash
docker run -d --pull always \
  --name byonk \
  -p 3000:3000 \
  ghcr.io/oetiker/byonk:latest
```

This uses embedded screens, fonts, and config - no volumes needed.

### Customization Mode

To customize screens and config, mount volumes and set environment variables:

```bash
docker run -d --pull always \
  --name byonk \
  -p 3000:3000 \
  -e SCREENS_DIR=/data/screens \
  -e FONTS_DIR=/data/fonts \
  -e CONFIG_FILE=/data/config.yaml \
  -v ./data:/data \
  ghcr.io/oetiker/byonk:latest
```

On first run with empty directories, Byonk automatically seeds them with embedded defaults.

Available tags:
- `latest` - Latest stable release
- `0` - Latest v0.x release
- `0.4` - Latest v0.4.x release
- `0.4.0` - Specific version

### Docker Compose

**Zero-config:**

```yaml
services:
  byonk:
    image: ghcr.io/oetiker/byonk:latest
    ports:
      - "3000:3000"
    restart: unless-stopped
```

**With customization:**

```yaml
services:
  byonk:
    image: ghcr.io/oetiker/byonk:latest
    ports:
      - "3000:3000"
    environment:
      - SCREENS_DIR=/data/screens
      - FONTS_DIR=/data/fonts
      - CONFIG_FILE=/data/config.yaml
    volumes:
      - ./data:/data  # Empty on first run = auto-seeded
    restart: unless-stopped
```

## Pre-built Binaries

Download the latest release from [GitHub Releases](https://github.com/oetiker/byonk/releases).

Available platforms:
- `x86_64-unknown-linux-gnu` - Linux (Intel/AMD 64-bit)
- `aarch64-unknown-linux-gnu` - Linux (ARM 64-bit, e.g., Raspberry Pi 4)
- `x86_64-apple-darwin` - macOS (Intel)
- `aarch64-apple-darwin` - macOS (Apple Silicon)
- `x86_64-pc-windows-msvc` - Windows

Extract and run:

```bash
tar -xzf byonk-*.tar.gz
./byonk
```

By default, Byonk listens on `0.0.0.0:3000` and uses embedded assets.

### Extracting Embedded Assets

To customize the embedded screens and config:

```bash
# See what's embedded
./byonk init --list

# Extract everything for editing
./byonk init --all

# Extract specific categories
./byonk init --screens
./byonk init --config
```

## Directory Structure (When Customizing)

When using external files (via env vars), Byonk expects:

```
data/
├── config.yaml          # Device and screen configuration
├── screens/             # Lua scripts and SVG templates
│   ├── default.lua
│   ├── default.svg
│   └── ...
└── fonts/               # Custom fonts (optional)
    └── Outfit-Variable.ttf
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `CONFIG_FILE` | *(embedded)* | Path to configuration file |
| `SCREENS_DIR` | *(embedded)* | Directory containing Lua scripts and SVG templates |
| `FONTS_DIR` | *(embedded)* | Directory containing font files |

When path variables are not set, Byonk uses embedded assets (no filesystem access).

## Running as a Service (systemd)

Create `/etc/systemd/system/byonk.service`:

```ini
[Unit]
Description=Byonk Content Server
After=network.target

[Service]
Type=simple
User=byonk
WorkingDirectory=/opt/byonk
ExecStart=/opt/byonk/byonk serve
Environment="BIND_ADDR=0.0.0.0:3000"
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl enable byonk
sudo systemctl start byonk
```

## CLI Commands

### Status (Default)

Running `byonk` without arguments shows current configuration:

```bash
./byonk
```

### Server

Start the HTTP server:

```bash
./byonk serve
```

### Render

Render a screen directly to PNG (useful for testing):

```bash
./byonk render --mac "00:00:00:00:00:00" --output test.png
```

**Options:**

| Option | Description |
|--------|-------------|
| `-m, --mac` | Device MAC address (required) |
| `-o, --output` | Output PNG file path (required) |
| `-d, --device` | Device type: "og" (800x480) or "x" (1872x1404) |
| `-b, --battery` | Battery voltage for testing (e.g., 4.12) |
| `-r, --rssi` | WiFi signal strength for testing (e.g., -67) |
| `-f, --firmware` | Firmware version string for testing |

**Example with all device info:**

```bash
./byonk render -m "AC:15:18:D4:7B:E2" -o test.png \
  --battery=4.12 --rssi=-67 --firmware="1.2.3"
```

> **Note:** Use `=` syntax for negative numbers (e.g., `--rssi=-67`).

### Init

Extract embedded assets for customization:

```bash
./byonk init --all        # Extract everything
./byonk init --screens    # Extract screens only
./byonk init --list       # List embedded assets
```

## Verifying Installation

1. Open `http://your-server:3000/health` - should return "OK"
2. Open `http://your-server:3000/swagger-ui` - shows API documentation
3. Point a TRMNL device to your server to test

## Configuring Your TRMNL Device

To use Byonk with your TRMNL device, configure the device to point to your server instead of the default TRMNL cloud service.

> **Note:** Refer to TRMNL documentation for instructions on configuring a custom server URL.

## Next Steps

- [Configure](configuration.md) your screens and devices
- [Create your first screen](../tutorial/first-screen.md)
