# Installation

Byonk can be installed via Docker container or pre-built binaries.

## Docker (Recommended)

The easiest way to run Byonk:

```bash
docker run -d \
  --name byonk \
  -p 3000:3000 \
  -v ./config.yaml:/app/config.yaml \
  -v ./screens:/app/screens \
  -v ./fonts:/app/fonts \
  ghcr.io/oetiker/byonk:latest
```

Available tags:
- `latest` - Latest stable release
- `0` - Latest v0.x release
- `0.3` - Latest v0.3.x release
- `0.3.0` - Specific version

### Docker Compose

```yaml
services:
  byonk:
    image: ghcr.io/oetiker/byonk:latest
    ports:
      - "3000:3000"
    volumes:
      - ./config.yaml:/app/config.yaml
      - ./screens:/app/screens
      - ./fonts:/app/fonts
    environment:
      - URL_SECRET=your-secret-here
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
cd byonk
./byonk
```

By default, Byonk listens on `0.0.0.0:3000`.

## Directory Structure

Byonk expects this directory structure:

```
byonk/
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
| `CONFIG_FILE` | `./config.yaml` | Path to configuration file |
| `SCREENS_DIR` | `./screens` | Directory containing Lua scripts and SVG templates |
| `URL_SECRET` | (random) | HMAC secret for signed image URLs |

> **Warning:** If `URL_SECRET` is not set, a random secret is generated on each startup. This means image URLs become invalid after a restart. For production, set a persistent secret.

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
ExecStart=/opt/byonk/byonk
Environment="BIND_ADDR=0.0.0.0:3000"
Environment="URL_SECRET=your-secret-here"
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
