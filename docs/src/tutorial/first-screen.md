# Your First Screen

Let's create a simple screen that displays a greeting and the current time. This will introduce you to the basic workflow of creating Byonk screens.

## Step 0: Set Up Your Workspace

Byonk embeds all assets in the binary. To customize screens, you must set environment variables pointing to external directories.

**For binary users:**
```bash
# Set paths and start server (auto-seeds empty directories)
export SCREENS_DIR=./screens
export CONFIG_FILE=./config.yaml
byonk serve
```

**For Docker users:**
```bash
docker run -d -p 3000:3000 \
  -e SCREENS_DIR=/data/screens \
  -e CONFIG_FILE=/data/config.yaml \
  -v ./data:/data \
  ghcr.io/oetiker/byonk
```

On first run, empty directories are automatically populated with defaults. You can then edit the files in `screens/` and `config.yaml`.

> **Tip:** Keep the server running in a terminal. Lua scripts and SVG templates are reloaded on every request - just save and refresh!

## Step 1: Create the Lua Script

Create a new file `screens/hello.lua`:

```lua
-- Hello World screen
-- Displays a greeting with the current time

local now = time_now()

return {
  data = {
    greeting = "Hello, World!",
    time = time_format(now, "%H:%M:%S"),
    date = time_format(now, "%A, %B %d, %Y")
  },
  refresh_rate = 60  -- Refresh every minute
}
```

**What this does:**
- `time_now()` gets the current Unix timestamp
- `time_format()` formats it into readable strings
- The returned `data` table is passed to the template
- `refresh_rate` tells the device to check back in 60 seconds

## Step 2: Create the SVG Template

Create a new file `screens/hello.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480">
  <style>
    .greeting {
      font-family: sans-serif;
      font-size: 48px;
      font-weight: bold;
      fill: black;
    }
    .time {
      font-family: sans-serif;
      font-size: 72px;
      font-weight: bold;
      fill: black;
    }
    .date {
      font-family: sans-serif;
      font-size: 24px;
      fill: #555;
    }
  </style>

  <!-- White background -->
  <rect width="800" height="480" fill="white"/>

  <!-- Greeting -->
  <text class="greeting" x="400" y="120" text-anchor="middle">
    {{ data.greeting }}
  </text>

  <!-- Large time display -->
  <text class="time" x="400" y="260" text-anchor="middle">
    {{ data.time }}
  </text>

  <!-- Date below -->
  <text class="date" x="400" y="320" text-anchor="middle">
    {{ data.date }}
  </text>

  <!-- Footer -->
  <text x="400" y="450" text-anchor="middle" font-family="sans-serif" font-size="14" fill="#999">
    My first Byonk screen!
  </text>
</svg>
```

**Template features used:**
- `{{ data.variable }}` - Inserts values from the Lua script's `data` table
- CSS styling for fonts and colors
- `text-anchor="middle"` for centered text

## Step 3: Add the Screen to Configuration

Edit `config.yaml` to add your new screen:

```yaml
screens:
  # ... existing screens ...

  hello:
    script: hello.lua
    template: hello.svg
    default_refresh: 60
```

## Step 4: Assign to a Device

Still in `config.yaml`, assign the screen to your device:

```yaml
devices:
  "YOUR:MAC:AD:DR:ES:S0":
    screen: hello
    params: {}
```

Replace `YOUR:MAC:AD:DR:ES:S0` with your device's actual MAC address.

> **Tip:** Check the Byonk server logs when your device connects - the MAC address is printed there.

## Step 5: Test It

1. **Restart Byonk** (config.yaml changes require restart)

2. **Check the API** at `http://localhost:3000/swagger-ui`:
   - Use the `/api/display` endpoint with your device's MAC
   - You'll get an image URL with a content hash
   - Open that URL to see your screen!

3. **Or wait for your device** to refresh automatically

## Understanding the Result

Your screen should look like this:

![Hello World screen](../images/hello.png)

## Adding Parameters

Let's make the greeting customizable. Update your files:

**screens/hello.lua:**
```lua
local now = time_now()

-- Get name from params, default to "World"
local name = params.name or "World"

return {
  data = {
    greeting = "Hello, " .. name .. "!",
    time = time_format(now, "%H:%M:%S"),
    date = time_format(now, "%A, %B %d, %Y")
  },
  refresh_rate = 60
}
```

**config.yaml:**
```yaml
devices:
  "YOUR:MAC:AD:DR:ES:S0":
    screen: hello
    params:
      name: "Alice"
```

Now your screen will say "Hello, Alice!" instead of "Hello, World!".

## Troubleshooting

### Screen shows error

Check the Byonk logs for script errors:

```bash
byonk serve
# Look for ERROR or WARN lines
```

### Template variables not replaced

Make sure your Lua script returns a `data` table with the expected keys:

```lua
return {
  data = {
    greeting = "Hello"  -- Must match {{ greeting }} in template
  },
  refresh_rate = 60
}
```

### Device not updating

- Check that the device MAC in config matches exactly (uppercase, with colons)
- Verify the device is pointing to your Byonk server
- Check device WiFi connectivity

## Real-World Example: Transit Departures

Here's what a more complex screen looks like - the built-in transit departure display:

![Transit departures screen](../images/transit.png)

This screen demonstrates:
- Fetching live data from an API
- Processing JSON responses
- Dynamic refresh rates (updates after each bus departs)
- Styled table layout with alternating rows
- Color-coded line badges

Check out `screens/transit.lua` and `screens/transit.svg` in the Byonk source for the complete implementation.

## What's Next?

Now that you have a basic screen working, learn more about:

- [Lua Scripting](lua-scripting.md) - Fetch data from APIs
- [SVG Templates](svg-templates.md) - Create complex layouts
