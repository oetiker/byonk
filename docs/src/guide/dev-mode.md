# Dev Mode

Byonk includes a development mode that provides a web-based device simulator with live reload capabilities, making it easier to develop and test screens.

## Starting Dev Mode

```bash
# Start with dev mode enabled
byonk dev

# With external screens directory for live reload
SCREENS_DIR=./screens byonk dev
```

Once started, open your browser to `http://localhost:3000/dev` to access the device simulator.

![Dev Mode Screenshot](images/dev-mode-screenshot.png)

## Features

### Device Simulator

The simulator displays your rendered screens in a visual frame resembling a TRMNL device. You can:

- **Select a screen** from the dropdown (populated from your config.yaml)
- **Enter a MAC address** to auto-load the configured screen and parameters for that device
- **Choose device model**: OG (800x480) or X (1872x1404)
- **Set custom dimensions** for testing different screen sizes
- **Set display colors**: customize the palette (e.g., `#000000,#FFFFFF,#FF0000,#FFFF00`) for testing color dithering
- **Simulate device context**: battery voltage, WiFi RSSI, and current time
- **View the rendered PNG** exactly as it would appear on the device
- **Pixel inspector**: hover over the image to see a magnified view

### Live Reload

When `SCREENS_DIR` is set to an external directory, the dev mode watches for changes to `.lua` and `.svg` files. When you save a file:

1. The file watcher detects the change
2. An event is sent to connected browsers via Server-Sent Events (SSE)
3. The screen automatically re-renders with the latest code

This allows for rapid iteration without manual refreshing.

### Custom Parameters

The dev UI includes a JSON editor for passing custom parameters to your Lua scripts. This lets you test different configurations without modifying `config.yaml`:

```json
{
  "api_key": "test-key",
  "location": "San Francisco"
}
```

These parameters are available in your Lua script via the `params` table.

### Error Display

When your Lua script or SVG template has errors, they're displayed in a collapsible error panel below the device simulator. Errors include:

- **Lua syntax errors**: Parse errors in your script
- **Lua runtime errors**: Errors during script execution
- **Template errors**: Tera templating issues
- **Render errors**: SVG to PNG conversion failures

## Configuration

Dev mode uses the same environment variables as the normal server:

| Variable | Description | Default |
|----------|-------------|---------|
| `BIND_ADDR` | Server bind address | `0.0.0.0:3000` |
| `SCREENS_DIR` | External screens directory (enables live reload) | (embedded) |
| `FONTS_DIR` | External fonts directory | (embedded) |
| `CONFIG_FILE` | External config file | (embedded) |

## Example Workflow

1. Extract embedded assets to work with:
   ```bash
   byonk init --all
   ```

2. Start dev mode with external screens:
   ```bash
   SCREENS_DIR=./screens CONFIG_FILE=./config.yaml byonk dev
   ```

3. Open `http://localhost:3000/dev` in your browser

4. Select the screen you want to work on

5. Edit your Lua script or SVG template - changes appear automatically

6. Use the custom parameters field to test different configurations

7. Check the error panel if something goes wrong

## Differences from Production

Dev mode includes a few differences from the production `byonk serve` command:

- Additional `/dev/*` routes for the simulator UI
- File watching enabled (when using external SCREENS_DIR)
- No content caching - always renders fresh content
- More verbose logging by default
