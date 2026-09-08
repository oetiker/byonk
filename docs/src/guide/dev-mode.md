# Dev Mode

Dev mode is Byonk's built-in screen editor: a three-column web page where you
pick a screen, edit its files with syntax checking as you type, and watch the
rendered result next to them. It also carries the display calibration tools —
dither algorithm, tuning knobs and measured panel colours — and syncs what you
tune there to the physical device.

It is part of the normal server. There is no separate command to start: `byonk
serve` mounts it, behind a login, at `/dev/`.

![Dev Mode Screenshot](images/dev-mode-screenshot.png)

## Opening the editor

### Set a username and password

The editor is invisible until you give it credentials. With none configured,
the editor page and its login form both answer `404 Not Found`, so a server
that was never meant to be edited over the web does not offer a login to guess
at. (The JSON API under `/dev/api/` answers `401` rather than `404` in that
case.)

Set both halves, in any one of these places:

| Where | Keys |
|-------|------|
| Environment | `BYONK_DEV_USERNAME`, `BYONK_DEV_PASSWORD` |
| `config.yaml` | `admin.dev_username`, `admin.dev_password` |
| Home Assistant app | the **Dev UI username** / **Dev UI password** options |

```yaml
admin:
  dev_username: tobi
  dev_password: a-long-random-string
```

The environment wins over `config.yaml`, exactly as `BYONK_ADMIN_TOKEN` does.
Both halves are required and a blank value counts as unset, so setting only a
username leaves the login disabled.

### Log in

Open `http://<host>:3000/dev/` in a browser and the login form appears. A bare
`http://<host>:3000/dev` is redirected to the same place.

A successful login sets a `byonk_dev` session cookie — `HttpOnly`, scoped to
`/dev`, good for 30 days. The key that signs it is generated fresh each time
the server starts, so restarting Byonk logs everyone out. A wrong username or
password is answered after a one-second delay, and the message never says
which of the two was wrong.

The **Logout** button in the toolbar appears only for a cookie session; the
two routes below are not sessions to end.

### The admin token also works

Any request carrying the admin token as a bearer credential is admitted
without a login:

```
Authorization: Bearer <your admin token>
```

That is what lets a script or an MCP client reach `/dev/api/*` directly. See
[Admin API](../api/admin-api.md) for where the token comes from.

### From the Home Assistant sidebar

The Home Assistant app publishes the editor as an Ingress panel: a **Byonk**
entry in the sidebar opens it directly, with no second login. Home Assistant
has already authenticated you, and the app trusts that route only for requests
arriving from the Supervisor itself.

Inside Home Assistant you therefore need no dev username or password at all.
Set them only if you also want to reach the editor from outside Home
Assistant, on `http://<host>:3000/dev/`.

## The three columns

**Left — screens.** Every screen the server can resolve, grouped by screen
repo handle. Repos you cannot write to are dimmed and their screens italic;
you can still open one and read every file, and **Copy** it into a repo you
can write to — but its files open read-only.

**Middle — files.** One tab per file in the selected screen, with
`meta.yaml`, `script.lua` and `screen.svg` first and everything else after
them alphabetically. Below the tabs sits the problems bar, where syntax errors
appear as you type.

**Right — preview.** The screen rendered through the same pipeline the device
uses, in a frame the size of the selected panel, with the render time and a
console beneath it. Hover the image for a magnified pixel view.

## Which repos you can edit

Writability is a property of where a screen repo's files live, not of its
name. `local` (your `SCREENS_DIR`), `examples`, and any repo registered with
`path:` are backed by directories Byonk manages and are editable in place. The
embedded `byonk-builtin` screens and any repo fetched from git with `repo:`
are read-only — a fetch can replace a git checkout wholesale, so edits there
would not survive. The full picture is in
[Screen Authoring](authoring.md#what-makes-a-repo-writable).

In the Home Assistant app only `local` and `examples` are writable: the app's
Screen repos option accepts `repo:` rows only.

## Editing and saving

Type in any file of a screen that lives in a writable repo. Its tab shows a
dirty marker, and so does the screen name in the toolbar, until the bytes
reach disk.

- **Cmd/Ctrl-S** saves the file you are in.
- The toolbar's **Save** button saves *every* file you have changed, not just
  the one on screen.

In a read-only repo the files open read-only: you cannot type in them at all.
Cmd/Ctrl-S still fires there, and is answered with a message naming the repo
rather than failing in silence. To change such a screen, **Copy** it into a
writable repo and edit the copy.

### Conflict detection

Every save carries the etag the file had when you opened it. If the file
changed underneath you — another browser tab, an editor on the server, a git
fetch — the write is refused and a banner offers two ways out:

- **Reload** throws your edits away and takes the version on disk.
- **Overwrite** re-reads the file only to take its current etag, then writes
  your buffer over it.

The same banner appears when the file watcher reports a change to a file you
have unsaved edits in — Byonk never silently replaces a buffer you are still
working on.

### New, Copy, Rename, Delete

| Button | What it needs | What it does |
|--------|---------------|--------------|
| **New** | a writable repo to put it in | Scaffolds `meta.yaml`, `script.lua` and `screen.svg` from a minimal starter. The generated `meta.yaml` pins the running engine's `byonk:` compatibility requirement. |
| **Copy** | any selected screen | Forks it — every file in its folder — into a writable repo you choose. This works from `byonk-builtin` and from git-fetched repos, which is the supported way to customize a screen you cannot edit. |
| **Rename** | a writable repo | Moves the screen within its own repo. You are asked to confirm first if you have unsaved edits, because the rename would leave them unsavable. |
| **Delete** | a writable repo | Removes the screen. |

The **+** next to the file tabs adds a file to the current screen. Creating a
file that already exists is reported rather than silently opening the existing
one. `meta.yaml`, `script.lua` and `screen.svg` cannot be deleted, and no
single file may exceed 5 MB.

## Syntax checking

400 ms after you stop typing, the unsaved buffer is checked on the server and
any error is marked in the editor and listed in the problems bar. It is the
text in front of you that gets checked, not the copy on disk.

What the check catches, by file:

| File | Check |
|------|-------|
| `meta.yaml` | Parses against the screen metadata schema. |
| `*.lua` | Compiles the chunk. Syntax errors only — the script is **not** executed. |
| `*.svg` | Compiles as a Tera template and resolves its `{% extends %}` / `{% include %}` chain, so a missing or misspelled include is caught. |
| anything else | No check; the file is reported as fine. |

After a successful save the whole screen is validated and the bar reports
`✔ all files valid` when nothing is left.

**What only a render catches.** Because the Lua is compiled but never run, the
check says nothing about what the script *does*: a failing HTTP request,
indexing a nil, an API that changed shape, a template that compiles but
positions text off the panel. Those show up in the preview and its console.
Keep an eye on both.

## Preview and pop-out

The preview re-renders whenever you save, whenever the file watcher reports a
change, and whenever you move a control in the Simulate drawer. Render time is
shown above the image and logged in the console below it.

**Pop out** opens the preview in its own window with **Fit** and **100%** zoom
— useful on a second monitor, or for looking at a 1872×1404 TRMNL X render at
its real size. The pop-out renders nothing itself: the editor window hands it
each finished image, so it follows the editor live and goes quiet when the
editor window closes. It follows the editor's colour scheme too.

## Colour scheme

The toolbar's **System / Light / Dark** control sets the editor's colour
scheme. It defaults to System — following your operating system's setting —
and your choice is remembered in the browser and applied to the pop-out
window as well.

## Live reload

Set `SCREENS_DIR` and Byonk watches it for changes. The editor holds a
Server-Sent Events connection to `/dev/api/events`, and the **File Watch**
badge in the toolbar shows whether it is up; a dropped connection retries by
itself every three seconds.

When a watched file changes, an open buffer showing that file is reloaded from
disk — unless you have unsaved edits in it, in which case you get the conflict
banner instead — and the preview re-renders. This is what makes an external
editor work alongside the built-in one.

Without `SCREENS_DIR` there is nothing on disk to watch, and Byonk says so at
startup.

## Simulate drawer

The **Simulate** button opens a drawer with everything about the device
context the preview is rendered under. Nothing here changes a file; it changes
what the renderer is told.

| Control | What it sets |
|---------|--------------|
| **Device** | Renders *that device's* own configured screen, params, panel and dither settings — not the screen open in the editor — and syncs your tuning to the real device (below). |
| **Panel profile** | Which panel's size and palette to render for, with its official and measured colour swatches. |
| **Show actual panel colors** | Whether the preview shows the panel's measured colours or the official palette. |
| **Dither** and its four tuning fields | The algorithm and its `max_error`, `chroma_clamp`, `noise_scale` and `strength`. |
| **Battery (V)**, **WiFi (dBm)** | What the script sees as the device's battery voltage and signal strength. |
| **Current Time** | Freezes the clock the script renders against, so a time-dependent screen can be checked at any hour. The `×` returns it to live time. |
| **Custom Parameters (JSON)** | The `params` table your Lua script receives. |

The dither dropdown and each algorithm's default tuning values are served by
the engine itself rather than listed in the page, so the preview cannot
disagree with what the renderer would do.

### Live device sync

When you pick a device entry and then change the dither algorithm, the tuning
values or the measured colours, those overrides are handed to the production
`/api/display` handler for that device. The physical device picks them up on
its next refresh — what you tune is what the device shows. The overrides live
in memory only and are gone when the server restarts, so commit anything you
want to keep to `config.yaml`.

## Display calibration

Dev mode provides tools for calibrating dithering to match your physical
display.

### Dither Algorithm Selection

The dither dropdown lets you try all 9 algorithms on your content:
- `atkinson` (default) — Atkinson error diffusion (75% propagation)
- `atkinson-hybrid` — Atkinson with hybrid propagation (100% achromatic / 75% chromatic)
- `floyd-steinberg` — Floyd-Steinberg with blue noise jitter
- `jarvis-judice-ninke` — wide 12-neighbor kernel
- `sierra`, `sierra-two-row`, `sierra-lite` — Sierra family
- `stucki` — wide 12-neighbor kernel similar to JJN
- `burkes` — 7-neighbor kernel, good balance of speed and quality

### Dither Tuning Controls

The drawer exposes these tuning parameters:

| Control | Effect |
|---------|--------|
| **Max error** | Caps how much accumulated error one pixel may carry into its neighbours. The default is 1.0 — full scale in a channel. Lowering it suppresses diffusion, which quietens oscillation but makes saturated areas render flat. |
| **Noise scale** | Controls blue noise jitter strength. Higher values break "worm" artifacts more aggressively. |
| **Chroma clamp** | Limits chromatic error propagation. Prevents color bleeding on chromatic palettes. |
| **Strength** | Error diffusion strength (0.0–2.0, default 1.0). Lower means less dithering texture. |

### Color Calibration

Click any actual-color swatch to open the HSL adjustment popup. Adjust hue, saturation, and lightness with live preview to match what your panel really displays. The adjusted `colors_actual` string can be copied to `config.yaml`.

### Calibrator Screen

Byonk ships a built-in calibration screen (`byonk-builtin/calibration/color`) designed specifically for display calibration. Assign it to your device temporarily while tuning:

```yaml
devices:
  "ABCDE-FGHJK":
    screen: byonk-builtin/calibration/color
    panel: my_panel
    dither: atkinson
```

The calibrator shows everything you need to evaluate dithering quality:

- **White-to-color gradients** for each palette color — reveals error diffusion artifacts, oscillation, and color bleeding
- **Full hue sweep** at 100% saturation — shows how the ditherer maps arbitrary colors to your limited palette
- **Test photo** — real-world image to judge overall photo reproduction
- **Solid color patches** with hex labels — compare what the panel actually displays against the expected color values

Use the calibrator on your physical device while adjusting tuning in dev mode — the live sync means every change you make is immediately visible on the display.

### Gamut Patch Screen

`byonk-builtin/calibration/gamut` answers a narrower question: which colors can
this panel actually mix, and which does it give up on?

```yaml
devices:
  "ABCDE-FGHJK":
    screen: byonk-builtin/calibration/gamut
    panel: my_panel
    params:
      hues: 24      # hue columns around the full circle (2-48)
      levels: 6     # lightness rows (1-12)
```

It draws the hue circle as isolated flat patches rather than the calibrator's
smooth gradient. That difference is the point: in a gradient, neighbouring hues
bleed together, so a hue the panel cannot reproduce still looks like it is doing
something. Here each patch stands alone, so you can read it directly:

- **A speckled patch** — the ditherer mixed several palette colors to approximate
  the request. This is what working output looks like.
- **A solid patch** — the ditherer picked one palette entry for every pixel. On a
  6-color panel expect solid blue across roughly 225°–270°: that genuinely is the
  best the palette offers, not a bug.
- **A solid white patch** (drawn outlined, so it doesn't read as a missing cell) —
  the request collapsed to white entirely. Cyan around 180° does this on 6-color
  panels, whose bluest and greenest inks are both dark.

Rows vary lightness because reachability depends on it — a hue may mix cleanly
when dark and collapse when light.

### Tone Marker A/B Screen

`byonk-builtin/calibration/tone` answers a different question again: what does
the gamut mapper actually change on real content, on your real panel?

```yaml
devices:
  "ABCDE-FGHJK":
    screen: byonk-builtin/calibration/tone
    panel: my_panel
    params:
      hues: 12      # patch grid hue columns (2-48)
      levels: 5     # patch grid rows (1-12)
```

It renders the same content — a photograph, a hue sweep, and a colour patch
grid, top to bottom — twice, side by side. The two columns are identical
markup with one difference: only the right-hand column is marked
`data-byonk-tone="continuous"`. That mark drives three things at once — the
right column is matched against the panel's *measured* colours and gamut
mapped, while the left column is matched against the *official* palette and
exact-match pinned (see
[Marking continuous-tone content](../tutorial/svg-templates.md#marking-continuous-tone-content)).
The left column is the untouched control. Whatever visibly differs between the
two columns on your device is what that whole difference in treatment is doing
to your content. The hue sweep is a fixed gradient of 12 equal hue steps, not
driven by either param.

There are no gamut-mapping knobs on this screen — it deliberately shows you
what a real screen gets, not a tuning surface.

### Calibration Workflow

1. **Assign the `byonk-builtin/calibration/color` screen** to your device in `config.yaml`
2. **Select your device** in the Simulate drawer — this loads its screen, panel, and dither settings
3. **Choose a dither algorithm** that works well for your content type
4. **Adjust tuning parameters** (max_error, noise_scale, chroma_clamp, strength) until the preview looks good
5. **Calibrate measured colors** by clicking actual-color swatches and adjusting HSL to match the solid patches on the physical display
6. **Verify on device** — changes sync automatically; wait for the next device refresh
7. **Commit to config** — copy the values to `config.yaml` and switch back to your normal screen:

```yaml
panels:
  my_panel:
    name: "My Panel"
    colors: "#000000,#FFFFFF,#FF0000,#FFFF00"
    colors_actual: "#303030,#D0D0C8,#C04040,#D0D020"  # from dev mode calibration

devices:
  "ABCDE-FGHJK":
    screen: examples/gphoto
    panel: my_panel
    dither: floyd-steinberg
    max_error: 1.0    # from dev mode tuning
    noise_scale: 0.5    # from dev mode tuning
```

Tuning values can also be set per-script in the Lua return table — see [Lua API](../api/lua-api.md#max_error-noise_scale-chroma_clamp-strength).

## Configuration

Dev mode is part of `byonk serve` and reads the same environment as the rest of
the server:

| Variable | Description | Default |
|----------|-------------|---------|
| `BIND_ADDR` | Server bind address | `0.0.0.0:3000` |
| `SCREENS_DIR` | Your writable `local` screen repo — also what the file watcher watches | (embedded) |
| `FONTS_DIR` | External fonts directory | (embedded) |
| `CONFIG_FILE` | External config file | (embedded) |
| `BYONK_DEV_USERNAME` | Dev UI login name | (unset — editor is 404) |
| `BYONK_DEV_PASSWORD` | Dev UI password | (unset — editor is 404) |

See [Environment Variables](installation.md#environment-variables) for the
full list.

## Example workflow

1. Extract embedded assets to work with:
   ```bash
   byonk init --all
   ```

2. Start the server with an external screens directory and a dev login:
   ```bash
   SCREENS_DIR=./screens CONFIG_FILE=./config.yaml \
   BYONK_DEV_USERNAME=me BYONK_DEV_PASSWORD=secret \
   byonk serve
   ```

3. Open `http://localhost:3000/dev/` and log in

4. Pick a screen in the left column — or **Copy** one of the examples into
   `local` and work on that

5. Edit `script.lua` and `screen.svg`; problems appear as you type, and the
   preview updates on save

6. Open **Simulate** to try another panel, freeze the clock, or pass custom
   params

7. Use the calibration tools to tune dithering for your panel

## The `/dev/api` endpoints

The page is a client of a small JSON API. Everything under `/dev/api` is
behind the same login as the page, and answers `401` with a JSON body rather
than a redirect when you are not admitted.

| Method and path | Purpose |
|-----------------|---------|
| `GET /dev/api/screens` | Screens, configured devices, panel profiles, and the dither algorithm list with per-algorithm defaults |
| `GET /dev/api/repos` | Screen repo handles and whether each is writable |
| `GET /dev/api/files?ref=` | The files of one screen, with size and whether each is text |
| `GET /dev/api/file?ref=&name=` | One file's text, with its etag |
| `PUT /dev/api/file?ref=&name=` | Write one file. `If-Match` is **required**: an etag to update, or `*` to create (`412` if it already exists). A missing header is `428` |
| `DELETE /dev/api/file?ref=&name=` | Delete one file (not the core three) |
| `POST /dev/api/screen` | Create a screen from a starter |
| `DELETE /dev/api/screen?ref=` | Delete a screen |
| `POST /dev/api/screen/copy` | Fork a screen into a writable repo |
| `POST /dev/api/screen/rename` | Rename a screen within its repo |
| `POST /dev/api/screen/validate` | Validate a screen as it is on disk |
| `POST /dev/api/screen/check` | Check one file's unsaved text |
| `GET /dev/api/render` | Render a screen to PNG under simulated device context |
| `GET /dev/api/events` | Server-Sent Events stream for live reload |
| `GET /dev/api/resolve-mac` | Resolve a MAC or registration code to that device's screen and params |
| `POST /dev/api/panel-colors`, `DELETE /dev/api/panel-colors/{panel}` | Set and clear the measured-colour override synced to the device |
| `GET /dev/api/whoami` | Who the server thinks you are, and whether you can log out |

For scripted or LLM-driven authoring, prefer the [MCP interface](mcp.md),
which covers the same screen-store operations with a stable tool contract.
