# Authoring with an LLM (MCP)

Byonk exposes a [Model Context Protocol](https://modelcontextprotocol.io/) endpoint at
`/mcp`. It lets an assistant like Claude Code list, read, create, edit, validate and
render screens on a running byonk — including one running inside Home Assistant —
entirely over the network. There is no filesystem access involved: no Samba share,
no `SCREENS_DIR` mount, no SSH. Every tool call goes through the same screen store
that backs the rest of byonk, so what the assistant sees and changes is exactly
what byonk itself will render and serve.

## Prerequisite: an admin token

`/mcp` is gated by the same admin token as the [Admin API](../api/admin-api.md). If no
token is configured, the endpoint doesn't just refuse requests — it returns
`404 Not Found`, as if it didn't exist. Set a token first:

- `config.yaml`: `admin.token: <your-secret>`
- environment variable `BYONK_ADMIN_TOKEN` (takes precedence over `config.yaml`)
- the Home Assistant app's Options screen, which provisions the token automatically

See [Admin API — Enabling the API](../api/admin-api.md#enabling-the-api) for the full
rules; they apply here unchanged.

## Connecting

The endpoint is `http://<host>:<port>/mcp` — `http://localhost:3000/mcp` with byonk's
default port, or `http://homeassistant.local:3000/mcp` for the Home Assistant
app. Transport is streamable HTTP, stateless, with plain JSON responses (no
server-sent-events framing to worry about). Authenticate with the token as a Bearer
credential:

```
Authorization: Bearer <your-secret>
```

With the Claude Code CLI:

```bash
claude mcp add --transport http byonk http://localhost:3000/mcp \
  --header "Authorization: Bearer <your-secret>"
```

Or as a JSON config block (the shape most MCP clients accept):

```json
{
  "mcpServers": {
    "byonk": {
      "type": "http",
      "url": "http://localhost:3000/mcp",
      "headers": {
        "Authorization": "Bearer <your-secret>"
      }
    }
  }
}
```

## Tools

### Read

| Tool | What it does |
|------|----------------|
| `list_screens` | List every screen this server can resolve, with its repo, title and whether it is writable. |
| `read_screen_file` | Read one file inside a screen (`meta.yaml`, `script.lua`, `screen.svg`, or another asset). Binary files (not valid UTF-8) return no content — only the etag and `binary: true`. |
| `list_screen_repos` | List the configured screen repositories: handle, kind, writability. |
| `list_devices` | List known TRMNL devices: MAC, model, assigned screen. |
| `get_config` | Read this server's non-secret global configuration. |

### Edit

| Tool | What it does |
|------|----------------|
| `write_screen_file` | Write one file inside a screen, atomically (supports optimistic-concurrency `if_match`). UTF-8 text only — refuses to overwrite an existing binary asset. |
| `create_screen` | Scaffold a new screen from the minimal starter (`meta.yaml`, `script.lua`, `screen.svg`). |
| `copy_screen` | Fork any screen — including read-only builtins and examples — into a writable repo. |
| `rename_screen` | Move a screen to a different path within its repo. |
| `delete_screen` | Delete a screen and every file in its directory. |
| `delete_screen_file` | Delete one sibling asset from a screen directory. |

`create_screen`, `copy_screen` and `rename_screen` each take the screen's
location inside a repo — `path`, `to_path` and `new_path` respectively. These
are directory paths (`clock`, or `home/clock` to nest), **not** display titles:
a new screen is always scaffolded as "New Screen" and a copy keeps the source's
`meta.yaml` verbatim. Set the title by writing `meta.yaml`. The repo handle is
passed separately (`local`, never `local/clock`).

Binary assets (images, fonts, anything not valid UTF-8) can be read for their etag
but not their content, and cannot be written or overwritten over MCP at all —
`write_screen_file` refuses if the target already exists and is binary. Place binary
assets by another means (a writable local screen repo mounted via `SCREENS_DIR`, a
Samba share, or an `EXAMPLES_DIR`-seeded repo) and author text files around them.

### Render

| Tool | What it does |
|------|----------------|
| `render_screen` | Render a screen and return the dithered PNG plus diagnostics (log, data, error). |
| `validate_screen` | Statically check a screen — `meta.yaml`, Lua, and template — without running it. |

`render_screen` shows what the panel will *really* look like by default: when measured
colours are available (from a named `panel` or from its own `colors_actual` argument), the
returned PNG is drawn in them. This changes only how the PNG is drawn, never the dithering
itself — a screen's dithering targets measured colours whenever they resolve, regardless of
`use_actual`.

| Argument | Type | What it does |
|----------|------|----------------|
| `image` | `dithered` \| `raw` \| `both` \| `none` | Which image(s) to return. Default `dithered`. `both` returns the dithered image then the pre-dither one, each preceded by a text block naming it. |
| `image_max_width` | int, optional | Downscale returned image(s) to at most this width, preserving aspect ratio. Never upscales. |
| `include_data` | bool, default `true` | Return the table the script produced. |
| `use_actual` | bool, optional | Draw the returned PNG in the panel's measured colours instead of the spec colours. Defaults to on whenever measured colours are available. `true` with nothing measured is a no-op, not an error. |
| `colors_actual` | string, optional | Comma-separated hex, index-parallel to the palette (e.g. `#0A0A0A,#E8E6E0,#A83A30`). Lets you preview a calibration without adding a `panel` to `config.yaml`. A `colors_actual` returned by the screen's own script still wins over this; a length mismatch is ignored (with a warning in the diagnostics' `log`) rather than failing the render. |

#### Keeping the response small

`render_screen` is by far the most expensive tool here, and an LLM client pays
for every byte of it. Rendering the builtin default screen at 800×480 measures:

| Arguments | Response |
|-----------|----------|
| *(defaults)* | 65 KB |
| `image_max_width: 200` | 25 KB |
| `image: "none"` | 3 KB |
| `image: "none", include_data: false` | 0.3 KB |
| `image: "raw"` | 648 KB |
| `image: "both"` | 710 KB |
| `image: "both", image_max_width: 300` | 171 KB |

Note the raw pre-dither image: it is full-colour and *ten times* the size of
the dithered one, so `raw` and `both` are worth pairing with `image_max_width`
unless you specifically need its exact pixels.

Three arguments let the caller decide what a render is worth:

- **`image: "none"`** when you only need the script's `log`, `data` or `error` —
  e.g. checking that an edit still runs. The images dominate the response.
- **`image_max_width`** for a layout check at a fraction of the cost. Be aware
  that resampling destroys the dither pattern, so a scaled `dithered` image is
  fine for judging layout and tone and useless for judging dithering itself.
  Omit it when you need to inspect the real pixels.
- **`include_data: false`** when a script embeds an image. `image_process`
  returns a base64 data URI, that URI lands in `data`, and the diagnostics are
  serialised twice (once as text, once as structured content) — so the picture
  is carried three times in total unless you drop the table.

The diagnostics also include `measured_source`, naming which layer actually supplied the
measured colours for that render: `script`, `render_opts` (the `colors_actual` argument
above), `panel.colors_actual`, or `none`.

### Assign

| Tool | What it does |
|------|----------------|
| `assign_screen` | Assign a device to a screen (use `list_devices` first to find its MAC). |

## Resources

Byonk also publishes its own authoring references as MCP resources, so the assistant
works from this server's actual rules instead of guessing from stale training data:

- `byonk://reference/lua-api` — every global and function available to `script.lua`.
- `byonk://reference/svg-templates` — the `screen.svg` templating contract.
- `byonk://reference/authoring` — how screens, screen repos and writability fit together.
- `byonk://schema/meta.yaml` — the JSON Schema for `meta.yaml`, generated from the same
  type that parses it.
- `byonk://examples/<screen path>` — one resource per shipped example screen, with the
  full `meta.yaml` + `script.lua` + `screen.svg` source, known to render on this server.

Have the assistant read `byonk://reference/lua-api` before it writes a script — it
describes exactly what's injected into the Lua sandbox, which is not the same as
general-purpose Lua.

## A workflow that works

1. `list_screens` to see what's already there and which repos are writable.
2. `copy_screen` a builtin or an example into a writable repo as a starting point.
   The built-in screens (`byonk-builtin`) are read-only, so editing one in place
   fails; forking first is the way in. `examples` is writable directly, but
   copying still keeps the original example intact as a reference.
3. Edit with `write_screen_file`.
4. `render_screen` and read its `log` and `error.line` fields — Lua errors and
   template errors both surface there, pointing at what to fix.
5. Repeat steps 3–4 until it renders clean.
6. `assign_screen` to put it on a real device.

## Security note

The admin token this endpoint uses grants full screen-authoring rights — creating,
editing and deleting screens — and device-assignment rights, not just read access.
Treat it like any other credential.

`/mcp` also accepts requests for any `Host` header, unlike the loopback-only default
most MCP servers use — this is deliberate, so the endpoint can be reached at a LAN
hostname such as `homeassistant.local:3000` rather than only `localhost`. That means
the Bearer token is the *only* thing standing between this endpoint and anyone who
can reach the port. Don't expose it to the internet.
