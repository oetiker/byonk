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

This will show you a short usage message. If you want to directly test the
server, try

```bash
./byonk serve
```

By default, Byonk listens on `0.0.0.0:3000` and uses embedded assets.

### Extracting Embedded Assets

```bash
# See what's embedded (built-in screens, examples, fonts, config)
./byonk init --list

# Extract everything for editing
./byonk init --all

# Extract specific categories
./byonk init --screens
./byonk init --config
```

`./byonk init --screens` initializes `SCREENS_DIR` as your writable `local`
screen repo: it writes a `byonk-screens.yaml` manifest there (nothing else).
It does **not** copy the built-in or example screens — those stay embedded
and read-only (built-ins) or get seeded separately (examples), as described
in [Screen Repos Section](configuration.md#screen-repos-section) and
[Screen Authoring](authoring.md). Use `./byonk init --config` to get an
editable `config.yaml` to start from.

## Directory Structure (When Customizing)

When using external files (via env vars), Byonk expects:

```
data/
├── config.yaml              # Device and screen configuration
├── screens/                 # Your writable `local` screen repo
│   ├── byonk-screens.yaml   # Repo manifest (name, description, author, license)
│   ├── my-clock/            # One screen = one folder
│   │   ├── meta.yaml        # Title, description, params schema
│   │   ├── script.lua       # Data-fetch logic
│   │   └── screen.svg       # Tera template
│   └── ...
├── examples/                # Shipped worked examples, seeded once (editable)
│   ├── byonk-screens.yaml
│   ├── hello/
│   └── ...
└── fonts/                   # Custom fonts (optional)
    └── Outfit-Variable.ttf
```

See [Screen Authoring](authoring.md) for how the built-in, example, and
your-own-screens layers relate to each other.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `CONFIG_FILE` | *(embedded)* | Path to configuration file |
| `SCREENS_DIR` | *(embedded)* | Your own writable screen repo (auto-registers as the `local` handle) |
| `FONTS_DIR` | *(embedded)* | Directory containing font files |
| `EXAMPLES_DIR` | `<SCREENS_DIR>/../examples` | Where the shipped worked-example screens are seeded (auto-registers as the `examples` handle). Only takes effect once — an existing, non-empty directory is left alone. |

When path variables are not set, Byonk uses embedded assets (no filesystem access).

On first run, an empty/missing `SCREENS_DIR` gets seeded with only a
`byonk-screens.yaml` manifest (no screen files — `byonk-builtin`'s `default` +
`calibration/*` screens stay embedded-only and are never copied there). An
empty/missing examples directory separately gets the full shipped `examples`
set (worked examples like `hello`, `gphoto`, `swiss-departure-board`) plus its
own manifest. Both seed once; your edits and deletions afterward are never
touched again.

**Docker note:** the default `EXAMPLES_DIR` is derived as a *sibling* of
`SCREENS_DIR` (`<SCREENS_DIR>/../examples`), one level up from the directory
you actually mount. If you only mount `SCREENS_DIR` itself (e.g. `-v
./screens:/screens -e SCREENS_DIR=/screens`), the derived examples directory
falls outside any mounted volume — ephemeral, and unwritable on a read-only
container root. Either mount a parent directory and point `SCREENS_DIR` at a
subdirectory of it (as in the example above, `-v ./data:/data -e
SCREENS_DIR=/data/screens`, which keeps the derived `/data/examples` inside
the same volume), or set `EXAMPLES_DIR` explicitly to a path you've mounted.

**Config vs. seeding:** if `screen_repos.examples` is set explicitly in
`config.yaml`, it wins for *registration* — the `examples` handle resolves to
that configured path instead of the auto-registered `EXAMPLES_DIR`/derived
default. Seeding (writing the shipped example files to disk) is unaffected by
this and always follows `EXAMPLES_DIR`/the derived default, since seeding runs
before `config.yaml` is parsed. In practice this only matters if you both
override `screen_repos.examples.path` *and* still want the shipped examples
copied to disk — in that case, set `EXAMPLES_DIR` to the same path.

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
./byonk init --screens    # Initialize SCREENS_DIR as your writable `local` repo (manifest only)
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
