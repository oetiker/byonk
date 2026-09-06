# Dev mode: always on, logged in, with a screen editor

Date: 2026-09-07
Status: approved design, not yet planned

## Goal

Turn the dev page from a simulator that only exists under a separate
`byonk dev` command into a permanent part of the server: reachable at `/dev`
on every `byonk serve`, protected by a login, and able to create and edit the
screens in writable screen repos with a syntax-checking editor.

Three user-visible outcomes:

1. `/dev` is always mounted. A username and password protect it. Inside Home
   Assistant the HA login is used instead, through add-on Ingress.
2. The page can list, create, copy, rename, delete and edit screens in any
   writable repo, with live syntax checking and the existing preview.
3. The preview can be popped out into its own window and sized freely.

## Non-goals

- No new write path. Every edit goes through the existing `ScreenStore`, the
  same one the MCP tools use. REST, MCP and the browser share one set of
  rules (path safety, size limit, ETag conflicts).
- No panel-level write path. Panels stay ssh-only; that gap is a separate
  piece of work.
- No multi-user model. One username, one password. Under Ingress the HA user
  name is displayed but carries no permissions of its own.
- No file upload for binary assets. Binary files are listed, not edited.
- No framework. The page stays plain HTML, CSS and JS. The only library is
  the editor.

## 1. One server mode and the auth layer

### The `dev` subcommand is removed

`byonk serve` mounts the `/dev` subtree always. The `Dev` variant of the CLI
enum, `run_dev_server()` and its docs go away. The file watcher that fed live
reload starts whenever `SCREENS_DIR` is set, regardless of how the server was
started. Its SSE stream stays available at `/dev/api/events`.

The `DevState` struct is folded into `AppState`. `build_router` gains the
`/dev` nest. `FileWatcher` becomes an `Option<Arc<FileWatcher>>` on
`AppState`, `None` when there is no external screens directory.

Breaking change for 0.20.0, listed in `CHANGES.md`.

### Credentials

| Source | Standalone | HA add-on |
|---|---|---|
| Username | `BYONK_DEV_USERNAME` env | `dev_username` option |
| Password | `BYONK_DEV_PASSWORD` env | `dev_password` option |

Resolution follows the `admin_token` rule in `addon_options.rs`: the env var
wins when set, otherwise the add-on option, otherwise none. A blank value
counts as none. Both must be set for the form login to exist. The values live
in a new `DevAuth` struct on `AppState`:

```rust
pub struct DevAuth {
    pub credentials: Option<(String, String)>, // (username, password)
    pub session_key: [u8; 32],                  // random per process
}
```

### Three ways through the middleware

One axum middleware, `require_dev`, wraps every route under `/dev` except the
login form and the static assets it needs. A request passes when any of the
following holds, checked in this order:

1. **Ingress peer.** The request arrived on the ingress listener (see below)
   and its peer address is `172.30.32.2`. The `X-Remote-User-Name` header,
   when present, is the display name; otherwise `ha-user`. Requests on the
   ingress listener from any other peer get 403, whatever headers they carry.
2. **Bearer token.** `Authorization: Bearer <admin_token>` matches the
   configured admin token, using the existing constant-time compare. Display
   name `token`. Lets scripts and MCP clients call `/dev/api/*` without a
   browser session. Absent admin token, this path is disabled.
3. **Session cookie.** A `byonk_dev` cookie whose value verifies against
   `session_key`. Display name is the configured username.

When none passes:

- The API routes (`/dev/api/*`) answer 401 with a JSON body.
- The page routes answer 302 to `/dev/login` when credentials are configured,
  and 404 when they are not (dev UI invisible, same convention as the admin
  API without a token).

### Session cookie

`POST /dev/login` with form fields `username` and `password`. On success it
sets:

```
Set-Cookie: byonk_dev=<issued_unix>.<hex hmac-sha256(session_key, issued_unix)>;
            HttpOnly; SameSite=Lax; Path=/dev; Max-Age=2592000
```

Verification recomputes the HMAC and rejects a token older than 30 days.
`session_key` is drawn from `rand` at startup, so a restart invalidates all
sessions. Nothing is stored server-side.

A failed login sleeps one second before answering, logs a `warn` line with the
peer address, and re-renders the form with a message. Username and password
are compared with the same constant-time helper the admin token uses.

`POST /dev/logout` clears the cookie and redirects to `/dev/login`.

`hmac`, `sha2` and `rand` are already dependencies.

### Ingress listener

Inside the add-on a second listener serves the same router on a second port,
`8099`, read from `BYONK_INGRESS_ADDR` (default unset, listener off). The
add-on manifest sets it to `0.0.0.0:8099`. Requests on that listener carry a
marker extension so `require_dev` knows which listener they came from; the
peer address comes from axum's `ConnectInfo`.

The port is not in the manifest's `ports:` list, so it is not published on
the host. The peer check is still enforced, because other containers on the
Supervisor network could reach it otherwise.

Under Ingress the Supervisor proxies
`/api/hassio_ingress/<token>/…` to the add-on and sets `X-Ingress-Path`. The
dev page therefore uses **relative URLs only**: `./dev.js`, `./api/render`,
and so on. `GET /dev` redirects to `/dev/` so relative paths resolve the same
way on both listeners. No handler reads `X-Ingress-Path`.

### Add-on manifest

```yaml
ingress: true
ingress_port: 8099
ingress_entry: /dev/
panel_icon: mdi:pencil-ruler
panel_title: Byonk
panel_admin: true
environment:
  BYONK_INGRESS_ADDR: 0.0.0.0:8099
options:
  dev_username: ""
  dev_password: ""
schema:
  dev_username: "str?"
  dev_password: "password?"
```

Translations get entries for both options. The manifest version is bumped so
the test VM reloads the schema (see the manifest sync trap in the handoff).

## 2. HTTP surface

All under `/dev/api/`, JSON in and out, behind `require_dev`. Errors use the
existing `ApiError` mapping: 400 bad input, 404 unknown ref, 409 conflict,
412 ETag mismatch, 413 too large, 422 validation.

### Moved, unchanged in behaviour

```
GET  /dev/api/screens                screens + devices + panels
GET  /dev/api/events                 SSE: file change events
GET  /dev/api/render                 PNG preview, all tuning params as today
GET  /dev/api/resolve-mac
POST /dev/api/panel-colors
DELETE /dev/api/panel-colors/{panel}
GET  /dev/api/whoami                 {"user": "...", "via": "cookie|ingress|token", "can_logout": bool}
```

### Screens and files

A screen is addressed by its `ref`, `handle/path`, exactly as the MCP tools
address it. The path may contain slashes (`local/calibration/dotpairs1a`), so
it travels as a query parameter on GET and DELETE and as a JSON field on
POST, never as a path segment (axum allows a wildcard only at the end of a
route). `name` is a path relative to the screen directory and passes through
the store's `safe_rel`.

```
GET    /dev/api/repos
       -> [{"handle": "local", "writable": true}, ...]

GET    /dev/api/files?ref=local/clock
       -> [{"name": "script.lua", "size": 812, "text": true}, ...]
       Lists the screen directory recursively. `text` is true when the
       file is valid UTF-8 and within MAX_FILE_BYTES.

GET    /dev/api/file?ref=local/clock&name=script.lua
       -> body = contents, ETag header
       404 for a missing file, 415 for a binary one.

PUT    /dev/api/file?ref=local/clock&name=script.lua
       body = contents, If-Match required
       -> 204 with new ETag. 412 when the ETag does not match. 409 when
       the repo is read-only.
       `If-Match: *` creates a new file and fails with 412 if it exists.

DELETE /dev/api/file?ref=local/clock&name=extra.svg
       The three core files (meta.yaml, script.lua, screen.svg) cannot be
       deleted; 422.

POST   /dev/api/screen
       {"handle": "local", "name": "clock", "starter": "minimal"}
       -> {"ref": "local/clock"}

POST   /dev/api/screen/copy
       {"ref": "examples/gphoto", "to_handle": "local", "to_name": "gphoto"}
       -> {"ref": "local/gphoto"}

POST   /dev/api/screen/rename
       {"ref": "local/clock", "name": "clock3"} -> {"ref": "local/clock3"}

DELETE /dev/api/screen?ref=local/clock3
       -> 204

POST   /dev/api/screen/validate
       {"ref": "local/clock"} -> ValidationReport (saved files on disk)

POST   /dev/api/screen/check
       {"ref": "local/clock", "file": "script.lua", "text": "..."}
       -> {"ok": bool, "issues": [{"severity", "location", "message",
           "line": Option<u32>}]}
```

### `check`: validating a draft

`ScreenStore::validate` reads files from disk and runs three validators, each
of which already takes a string. `check` extracts those into
`ScreenStore::check_text(screen_ref, file, text)`:

| file | validator |
|---|---|
| `meta.yaml` | `ScreenMeta::from_yaml` |
| `*.lua` | `mlua::Lua::new().load(text).into_function()` compile only |
| `*.svg` | `TemplateService::validate_template(text, &source, screen_path)`, includes resolved against the screen on disk |
| anything else | no check, `ok: true` |

`line` is parsed from the Lua and Tera error messages where they carry one
(`[string "..."]:6:` and `line 6` forms). It is `None` when unknown. The
existing `validate` is reworked to call `check_text` per file, so the two
cannot drift.

### Store additions

- `ScreenStore::list_files(screen_ref) -> Vec<FileEntry>`
- `ScreenStore::check_text(screen_ref, file, text) -> ValidationReport`
- `Issue` gains `line: Option<u32>`; MCP `validate_screen` passes it through.

Everything else is already there: `read_file`, `write_file` with `if_match`,
`delete_file`, `create_screen`, `copy_screen`, `rename_screen`,
`delete_screen`, `validate`, `render`.

### Auth routes

```
GET  /dev/login     form (200) or 404 when no credentials
POST /dev/login     form body -> 302 /dev/ or 200 form with error
POST /dev/logout    -> 302 /dev/login
```

## 3. The page

Layout A from the brainstorm: three columns, screens on the left, editor in
the middle, preview on the right, simulator controls in a drawer.

### Files

```
static/dev/index.html        the page
static/dev/login.html        the form
static/dev/preview.html      the pop-out
static/dev/dev.css
static/dev/dev.js            app logic (rewritten around a small state object)
static/dev/editor/           pnpm project: package.json, rollup.config.js, src/editor.js
static/dev/editor.bundle.js  committed build output, embedded via include_str!
```

The editor bundle exports one function on `window.byonkEditor`:
`create(parent, {language, doc, onChange, onSave})` returning
`{getValue, setValue, setDiagnostics(issues), focus}`. Languages: Lua, XML
(for SVG), YAML. Features: line numbers, bracket matching, search, undo
history, a lint gutter fed by `setDiagnostics`. Built with Rollup and
Terser, target ES2020. `pnpm run build` in `static/dev/editor` regenerates
the bundle and stamps a SHA-256 of `src/`, `package.json` and
`pnpm-lock.yaml` into its first line as a comment. The Makefile gets an
`editor` target that runs the build, and `make check` recomputes the hash and
fails when it differs from the stamp, so a source edit without a rebuild
cannot be committed unnoticed. CI does not need pnpm: it only hashes.

### Left column: screens

- Repos in the order `list_repos` returns them, writable first. Read-only
  repos are dimmed; their screens open in the editor read-only.
- Toolbar: **New**, **Copy**, **Rename**, **Delete**. New opens an inline
  form (repo select limited to writable handles, name input) and creates from
  the `minimal` starter. Copy defaults the destination to the first writable
  repo and the source name. Delete asks once with an in-page confirm, never a
  browser dialog.
- Selecting a screen loads its file list and opens `script.lua`. The
  selection persists in `localStorage`.

### Middle: editor

- One tab per file in the screen, core files first, then the rest in name
  order. Binary files show as a disabled tab with the size.
- `+` tab: name prompt in-page, creates the file with an empty body through
  `PUT` with `If-Match: *`.
- Each open file keeps its own editor instance, document and ETag. A dot on
  the tab marks a dirty buffer.
- **Save**: button or Cmd/Ctrl+S saves the current file. On 412 the strip
  shows "changed on disk" with **Reload** and **Overwrite** buttons.
  Overwrite re-reads the ETag and saves again.
- After a successful save the page runs `validate` on the screen and
  re-renders the preview.
- Switching screens or closing the tab with dirty buffers asks once in-page.
  `beforeunload` is set while any buffer is dirty.

### Problems strip

- While typing, 400 ms after the last keystroke, the draft goes to `check`.
  Issues with a line become gutter markers and underline marks through
  `setDiagnostics`; every issue is listed in the strip as
  `file:line message`.
- After save, the `validate` result replaces the strip content and shows one
  green tick per clean file.

### Right column: preview

- Device frame, dimensions, model and render time, then the console with
  log, data and error, as today. `render` runs on screen select, on save, on
  a file watcher event for the selected screen, and on any drawer change.
- **Pop out** button opens `./preview.html` with `window.open`, named
  `byonk-preview` so a second click focuses it instead of opening another.
  The pop-out contains the device frame only, plus a zoom switch: **fit**
  (scales to the window) and **100%** (one image pixel per CSS pixel, with
  scrollbars). The pixel inspector magnifier works in both places.
- Main page and pop-out share a `BroadcastChannel('byonk-preview')`. The main
  page posts `{url, width, height}` on every render; the pop-out reloads its
  image. When the pop-out opens it asks with `{hello: true}` and the main
  page answers with the current state. Same-origin, so it works under
  Ingress.

### Simulate drawer

The current configuration panel, moved into a drawer that slides over the
preview column when **Simulate** is pressed: device select, panel select and
swatches, preserve-exact, show-actual, dither, max error, chroma clamp,
noise scale, strength, battery, RSSI, time, params. The colour popup and the
device sync to `DevOverrides` are unchanged.

The dither select is populated from `/dev/api/screens` instead of being
hard-coded, which removes the duplicated `DITHER_DEFAULTS` list flagged in
the handoff.

### Top bar

Screen ref with dirty marker, watch status badge, **Simulate** toggle,
**Save**, user name from `whoami`, **Logout** (hidden when `can_logout` is
false, that is under Ingress or a bearer token).

### Live reload

An SSE event for a file open in the editor:

- buffer clean: reload the file silently, update the ETag.
- buffer dirty: keep the buffer, show "changed on disk" in the strip with the
  same Reload and Overwrite buttons the 412 path uses.

An SSE event for the selected screen also re-renders the preview.

## 4. Packaging, docs, changelog

- `homeassistant/byonk/config.yaml` as in section 1, version bumped.
  `translations/en.yaml` gains the two options. `DOCS.md` explains the
  sidebar entry and the optional password for access outside HA.
- `Dockerfile` unchanged. The ingress listener is an env var, not a port.
- `docs/src/guide/dev-mode.md` rewritten: starting the server, login,
  Ingress, the three columns, editing, pop-out, calibration drawer.
  Screenshot replaced.
- `byonk dev` removed from README, CLI help, `docs/src/guide/configuration.md`
  and the `lua-api.md` line that names `/dev/render`.
- `CHANGES.md` Unreleased:
  - Breaking: `byonk dev` is gone. The dev UI is part of `byonk serve` and
    requires a login (`BYONK_DEV_USERNAME` / `BYONK_DEV_PASSWORD`, or the
    add-on options). Existing `/dev/*` endpoints moved to `/dev/api/*`.
  - New: screen editor with syntax checking; Home Assistant sidebar entry
    via Ingress; pop-out preview.
- `.gitignore` gains `.superpowers/`.

## 5. Testing

Every test is broken once on purpose to confirm it fails before it is kept.

### Rust

- `require_dev`: no credentials gives 404 on `/dev/` and 401 on `/dev/api/`;
  wrong password 401; good cookie passes; cookie with a bad HMAC fails;
  cookie older than 30 days fails; bearer with the admin token passes;
  bearer disabled when no admin token; ingress listener rejects peer
  `10.0.0.5` with 403 and trusts `172.30.32.2`, exposing the header name in
  `whoami`; `X-Remote-User-Name` on the public listener is ignored.
- Login: one-second delay on failure is asserted by elapsed time; success
  sets the cookie with `HttpOnly` and `Path=/dev`.
- Routes: each `/dev/api/screen` handler through a temp writable repo,
  including 412 on a stale ETag, `If-Match: *` on an existing file, 422 on
  deleting `script.lua`, 409 on a read-only handle, `check` returning a line
  number for a Lua syntax error and for a Tera error, `check` on an unknown
  file kind returning ok.
- `validate` and `check_text` agree: a fixture with one broken file yields
  the same issue from both.
- `build_router` still constructs (the brace-syntax test) with the `/dev`
  nest.
- Credential resolution: env over option, blank is none, both required.

### Browser

Driven with the Chrome tools against a local `byonk serve` with a temp
`SCREENS_DIR`: log in, create a screen, introduce a Lua error and see the
gutter marker, fix it, save, see the preview, open the pop-out and confirm it
follows a re-render, copy a read-only example into `local`, delete the copy,
log out.

### Home Assistant

On the test VM: install the bumped add-on, open the sidebar entry, confirm
the page loads under the Ingress path with the HA user name shown and no
login form, save an edit, and confirm `http://<vm>:3000/dev/` still asks for
the password.

## 6. Open points settled during design

- Credentials are separate from `admin_token` so a human password and a
  machine token can be rotated independently.
- Cookie session rather than HTTP Basic, for logout and clean SSE and image
  loads.
- CodeMirror 6 with a committed bundle rather than CodeMirror 5, accepting a
  small pnpm build step for a maintained editor.
- Pop-out uses `BroadcastChannel`, not a second render pipeline; the pop-out
  never renders on its own.
