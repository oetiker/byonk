# Design — Screen Store Core + MCP Interface

_Spec 1 of the "byonk screen authoring" initiative. Status: draft for review._

## Purpose

Make byonk a place where screens are **authored**, not just served — and make an LLM
(Claude Code or similar) a first-class author that can develop a screen against a byonk
running _anywhere_ (including the Home Assistant add-on inside HA) with **no filesystem
access and no Samba share**, over the LAN.

This spec delivers the browser-free foundation: a shared screen-mutation core, a typed
screen-source model that makes read-only vs. writable a structural property, and an MCP
interface that exposes the full screen-development workflow (read context, edit files,
render-with-diagnostics, assign a device) plus the authoring contracts as MCP resources.

This is spec 1 of three. Out of scope here, each its own later spec:

- **Spec 2 — Svelte web UI at `/`**: always-on SPA, three-pane editor, live preview,
  full parity with today's `/dev`, after which `/dev` and `byonk dev` retire.
- **Spec 3 — Git commit & history**: gix write path (stage/commit/log/diff/revert-file)
  over local repos that are git working copies. Additive to this spec.

Both later specs consume the `ScreenStore` core defined here; neither is designed in a
vacuum because the core's operation set is fixed below.

## Background — current state (verified in tree)

- **Two run modes**: `byonk serve` (production) and `byonk dev` (adds `/dev`, a
  ~1900-line vanilla HTML/CSS/JS app: screen picker, panel/dither controls, SSE
  live-reload, `/dev/render` preview). Everything screen-related is **read-only** today;
  there is no server-side write path for screen files.
- **`AssetLoader`** (`src/assets.rs`, ~1023 lines): read-only, embedded-aware loader for
  screens/fonts/config. Embeds `screens/` (`EmbeddedScreens`), `fonts/`, and — separately
  — `byonk-base/` (`EmbeddedBase`).
- **Screen sources** (`src/services/screen_repo_loader.rs`): a `ScreenRepoSource` trait
  with `EmbeddedBuiltinSource` (embedded tree **overlaid with `SCREENS_DIR`** under the
  single `byonk-builtin` handle) and `DiskScreenRepoSource` (a fetched git cache under
  `/data/packages`). A screen is addressed `handle/path` (e.g. `byonk-builtin/default`,
  `tobitest/hello`). `BUILTIN_HANDLE = "byonk-builtin"`.
- **Base library**: `byonk-base/v1/{base,header,footer,status_bar,hinting}.svg`, embedded
  as `EmbeddedBase`, registered as Tera templates `byonk-base-v1/…`, reachable from **any**
  screen's SVG via `{% extends %}`/`{% include %}` regardless of that screen's repo. This
  is the shared layout backbone.
- **Config** (`src/models/config.rs`): `screen_repos: HashMap<String, ScreenRepoRef>` where
  `ScreenRepoRef { repo: Option<String>, pin: Option<String>, token: Option<String> }`.
  `repo == None` means the embedded builtin.
- **Admin API** (`src/api/admin/*`): token-gated `/api/admin/*` (devices, pending, config,
  screens, screen-repos, settings). `require_admin` checks `Authorization: Bearer <token>`
  against `admin.token` (or `BYONK_ADMIN_TOKEN`) in constant time; when no token is
  configured, admin routes return 404 (invisible). `addon_mode` gates **global** config
  writes to read-only (the add-on Options form owns them); device writes stay allowed.
- **Server hard-reference**: `content_pipeline.rs` falls back to `byonk-builtin/default`
  when a device has no assigned/resolvable screen. Registration/unassigned screens are
  **Rust-drawn**, not screen files.
- **HTTP stack**: axum 0.7, utoipa-swagger-ui 8, gix 0.66 (fetch-only), mlua 0.10.

## Key decisions (from brainstorming)

1. Writes land in a **writable local screen repo** on disk (backed by `SCREENS_DIR`, plus
   optionally more declared in config). No git in _this_ spec (commit/history is spec 3).
2. The **always-on** MCP interface is the primary agent surface; the Svelte UI (spec 2) is
   a second face on the same core.
3. **MCP over streamable HTTP**, mounted on the main router at `/mcp`, admin-token gated —
   so an agent can drive a byonk running inside HA over the LAN, no share, no fs access.
4. Implemented with **`rmcp`** (official Rust MCP SDK, 2.2), which requires **axum 0.8** —
   so an axum 0.7→0.8 bump is a prerequisite (small: two route strings + swagger-ui 8→9).
5. **Writability is a typed property of the screen source**, not a name check. `byonk-builtin`
   and git repos reject writes _structurally_.
6. **Three built-in layers, separated**: the `byonk-base-v1` include library (embedded,
   universal, versioned), the minimal `byonk-builtin` repo (`default` + `calibration/*`),
   and shipped **examples** (seeded to a writable `examples` repo on first run).
7. MCP surface: read context (screens/devices/repos/files + base-library + examples),
   edit, render-with-diagnostics, `assign_screen`. **No** log access. Contracts as MCP
   **resources**.

## Architecture

```
                       ┌────────────────────────────────────────┐
   MCP client (LLM) ──►│  /mcp   (rmcp, streamable HTTP)         │
   Svelte UI (spec 2)─►│  /api/admin/screens/*   (REST)          │──┐
   CLI  byonk render ─►│  (existing)                             │  │  require_admin
                       └────────────────────────────────────────┘  │  (Bearer token)
                                        │                            │
                                        ▼                            │
                         ┌──────────────────────────────┐           │
                         │        ScreenStore            │◄──────────┘
                         │  (the sole screen-mutation    │
                         │   owner; validation; render   │
                         │   orchestration)              │
                         └──────────────┬───────────────┘
                                        │ resolves handle → source
                                        ▼
              ┌───────────────────────────────────────────────────────┐
              │  ScreenRepoLoader registry (handle → ScreenRepoSource) │
              │                                                        │
              │  EmbeddedBuiltinSource   writable_root() = None        │
              │  GitScreenRepoSource     writable_root() = None        │
              │  LocalScreenRepoSource   writable_root() = Some(path)  │  ◄── NEW
              └───────────────────────────────────────────────────────┘
                     (byonk-base-v1 include library is orthogonal:
                      always-embedded Tera templates, reachable by any screen)
```

`AssetLoader` is unchanged and remains the read path. `ScreenStore` is the new write/validate/
render-orchestration owner and sits beside it, holding the one thing `AssetLoader` lacks: the
knowledge of which handles are writable and where their roots are.

## Component 1 — axum 0.7 → 0.8 upgrade (prerequisite, own commit)

Mechanical, landed and verified **before** any feature work:

- `axum` `0.7` → `0.8`; `utoipa-swagger-ui` `8` → `9` (its 9.x line targets axum 0.8).
- Change the two `:param` route strings to `{param}` syntax:
  - `src/server.rs:219` `"/api/image/:hash"` → `"/api/image/{hash}"`
  - `src/main.rs:845` `"/dev/panel-colors/:panel"` → `"/dev/panel-colors/{panel}"`
    (this route dies with `/dev` in spec 2, but must compile now).
- Fix any fallout from axum 0.8 dropping `#[async_trait]` on `FromRequest`/`FromRequestParts`
  and the `Path`/handler signature changes. `require_admin` takes `&HeaderMap` and is
  unaffected.

**Done-when**: `make check` green (fmt + clippy -D warnings + tests) with no behavior change.

## Component 2 — Typed screen sources

Add to the `ScreenRepoSource` trait:

```rust
/// The on-disk directory this source may be written to, or `None` if the
/// source is read-only (embedded, or a git cache that refresh would clobber).
fn writable_root(&self) -> Option<&Path> { None }
```

Three implementations:

- **`EmbeddedBuiltinSource`** — pure embedded tree, **the `SCREENS_DIR` overlay is removed**.
  `writable_root() = None`. Handle `byonk-builtin` now means exactly "the screens byonk
  ships," unshadowable and uneditable.
- **`GitScreenRepoSource`** — today's `DiskScreenRepoSource`, renamed for clarity (it backs
  git-fetched caches). `writable_root() = None` (a refresh overwrites it).
- **`LocalScreenRepoSource`** (new) — a writable directory on disk with a
  `byonk-screens.yaml` manifest. `writable_root() = Some(path)`.

### Config surface

`ScreenRepoRef` gains a `path` variant, **mutually exclusive** with `repo`:

```yaml
screen_repos:
  local:                      # auto-registered from SCREENS_DIR if not explicitly present
    path: /config/screens
  my-clocks:
    path: /home/me/byonk-clocks
  weather:
    repo: github.com/acme/screens
    pin: v1.4.0
```

```rust
pub struct ScreenRepoRef {
    pub repo: Option<String>,   // git remote  — read-only source
    pub path: Option<String>,   // local dir   — writable source   (NEW)
    pub pin: Option<String>,
    pub token: Option<String>,
}
```

- Both `repo` and `path` set → config error (rejected at load with a clear message).
- `SCREENS_DIR` auto-registers as handle `local` unless config already declares a `local`
  entry (explicit config wins). If `SCREENS_DIR` is unset, no `local` repo exists and the
  store is read-only (every write tool returns a message explaining how to enable it).
- Handle `byonk-builtin` remains reserved (rejected as a user handle, as today).
- **Add-on options schema** gains the same `path`-variant shape for `screen_repos` (the
  add-on Options form owns global config; see memory `ha-vm-addon-manifest-sync-gap` for the
  manifest-sync gotcha when changing the schema on the VM).

The `ScreenRepoLoader::new` signature already takes `disk_packages: HashMap<String, PathBuf>`;
extend the manager to pass both git-cache roots (as `GitScreenRepoSource`) and local paths
(as `LocalScreenRepoSource`), tagged by kind.

## Component 3 — `ScreenStore` (`src/services/screen_store.rs`)

The single owner of screen **mutation, validation, and render orchestration**. Holds an
`Arc<ScreenRepoManager>` (for the live loader/registry) and an `Arc<ContentPipeline>` (render).

A **screen** is a directory `handle/path` containing `meta.yaml` + `script.lua` +
`screen.svg` + optional sibling assets (`background.jpg`, `parts/*.svg`, …).

### Operations

All path inputs are validated: reject `..`, absolute paths, and symlink escape by
**canonicalize-then-verify-prefix** against the source's `writable_root()`. Per-file cap
~5 MB. Writes are **atomic** (temp file in the same directory + rename). Every write resolves
the target handle → source and **fails unless `writable_root()` is `Some`**, with an error
that names `copy_screen` as the remedy.

| op | signature (conceptual) | notes |
|---|---|---|
| list screens | `list_screens() -> Vec<ScreenListEntry>` | delegates to loader `list_all`; each entry adds `writable: bool` (derived from the source) |
| read file | `read_file(screen_ref, file) -> FileContents` | bytes + `etag` (content hash, e.g. blake3/sha256 hex) + `binary: bool` |
| write file | `write_file(screen_ref, file, bytes, if_match: Option<Etag>) -> Etag` | atomic; `Conflict` if `if_match` set and current etag differs |
| create screen | `create_screen(handle, name, template: StarterKind) -> ScreenRef` | scaffolds meta/lua/svg from an embedded starter that `{% extends "byonk-base-v1/base.svg" %}` |
| copy screen | `copy_screen(from_ref, to_handle, to_name) -> ScreenRef` | forks any (incl. read-only) screen into a writable repo — the sanctioned "customize a builtin/example" path |
| rename screen | `rename_screen(screen_ref, new_name)` | writable source only |
| delete screen | `delete_screen(screen_ref)` | writable source only |
| delete file | `delete_file(screen_ref, file)` | writable source only; refuses to delete the last of meta/lua/svg out from under a screen |
| validate | `validate(screen_ref) -> ValidationReport` | see below; no side effects |
| render | `render(screen_ref, opts) -> RenderResult` | see below |

After any structural mutation (create/copy/rename/delete), `ScreenStore` triggers
`ScreenRepoManager::rebuild_loader()` so the registry reflects disk.

### Concurrency model ("files on disk are truth")

The store never locks and never holds UI-only state. Optimistic concurrency via `etag`
(content hash) on `write_file`: a client that read etag `E`, then writes with `if_match: E`,
gets `Conflict` if an agent/editor/vim changed the file meanwhile — surfacing "changed on
disk since you opened it" instead of silently clobbering. The existing `FileWatcher` + SSE
(used by `/dev`, and by the spec-2 UI) continue to notify browsers of external edits.

### Validation

`validate(screen_ref) -> ValidationReport { ok, issues: [{severity, location, message}] }`:

- `meta.yaml` parses and conforms to the `ScreenMeta` schema (a JSON Schema, see resources).
- `script.lua` **compiles** (mlua load, no execution) → syntax errors with line numbers.
- `screen.svg` parses as XML and its `{% extends/include %}` targets resolve against
  the base library + the screen's own repo (reuse `template_service` resolution).

### Render with diagnostics

`render(screen_ref, opts) -> RenderResult`:

```
RenderResult {
  png: Vec<u8>,             // dithered, device-accurate
  raw_png: Option<Vec<u8>>, // pre-dither, for comparison (opts.include_raw)
  log: Vec<String>,         // captured log_info/log_warn/log_error from the script
  data: serde_json::Value,  // the table the Lua script returned
  refresh_rate: u32,
  error: Option<RenderError>, // Lua error with line, or template/SVG error
}
```

`opts` carries the existing render knobs (model/width/height, panel profile, dither
algorithm, error_clamp, chroma_clamp, noise_scale, preserve_exact, timestamp override) —
the same set `/dev/render` exposes today. The Lua `log_*` hooks (`lua_runtime.rs:869`) are
captured per-render into `log` instead of only going to tracing.

## Component 4 — Built-in layers split, examples, migration

### The three layers (restating the source model concretely)

1. **Base include library** `byonk-base-v1/…` — **untouched**. Stays embedded (`EmbeddedBase`),
   universal, versioned, read-only. The essential shared layout backbone.
2. **`byonk-builtin` repo** — shrinks to the minimum the server itself needs:
   `default` (the hard-referenced fallback) + `calibration/color` + `calibration/grey`
   (device setup). Embedded, read-only.
3. **`examples` repo** — `hello`, `mandelbrot`, `webscrape`, `gphoto`,
   `swiss-departure-board`, `demo/font`. **Embedded in the binary** but **seeded on first run**
   to a writable local repo `examples` (own `byonk-screens.yaml`, `name: examples`), so they
   are editable / forkable / deletable and work offline out of the box. They also back the
   MCP "examples" resource.

The `screens/` source tree is reorganized so the build embeds (a) the minimal builtin set
and (b) the examples set under distinct embed roots (e.g. `screens/builtin/` and
`screens/examples/`), keeping `EmbeddedScreens`-style access but partitioned. `byonk-base/`
is unaffected.

### Seeding changes (`AssetLoader::seed_if_configured`)

- **Stop copying builtin screens** into `SCREENS_DIR`. Into an empty `SCREENS_DIR`, seed only
  a `byonk-screens.yaml` manifest (`name: local`).
- Seed the **examples** set into the `examples` repo directory on first run (empty-only, so a
  user who deletes an example doesn't get it back on restart). Location: a sibling of
  `SCREENS_DIR` or a configured `examples` path; default `<SCREENS_DIR>/../examples`, declared
  as a `screen_repos.examples.path` entry the add-on/config sets by default.
- Font and config seeding are unchanged.

### One-time migration on startup (logged loudly)

For installs created before this change, where `SCREENS_DIR` was the `byonk-builtin` overlay:

1. If `SCREENS_DIR/byonk-screens.yaml` has `name: byonk-builtin`, rewrite it to `name: local`.
2. For each device config referencing `byonk-builtin/<x>` where `<x>` exists as a screen
   **in `SCREENS_DIR`** (i.e. a user screen, not a real builtin), rewrite the ref to
   `local/<x>` via `config_writer`. Refs to genuine builtins (`default`, `calibration/*`)
   are left alone — they keep resolving against the embedded source.
3. Emit an INFO summary of what was rewritten; make no change if nothing matches (idempotent).

Devices keep rendering the same screen across the upgrade.

## Component 5 — MCP interface (`/mcp`)

`rmcp` streamable-HTTP service mounted on the **main** router (`build_router`), so it shares
the listener, port, and `AppState`. **Auth**: the same Bearer token as `/api/admin/*`
(`require_admin` semantics) via a middleware layer on the `/mcp` route — no token configured
⇒ `/mcp` returns 404 (invisible), matching admin behavior. Clients pass
`Authorization: Bearer <token>`; in the HA add-on the token is auto-provisioned and copied
from the add-on Options once.

### Tools

**Read context**
- `list_screens()` → screens grouped by repo, each with `writable`, title, params, compat.
- `read_screen_file(screen_ref, file)` → contents + etag (+ `binary`).
- `list_devices()` → mac, model, assigned screen, last-seen, battery, rssi.
- `get_config()` → non-secret global config (tokens redacted).
- `list_screen_repos()` → handle, kind (embedded/git/local), status, sha.

**Edit** (all via `ScreenStore`; writable-target enforced structurally)
- `write_screen_file(screen_ref, file, content, if_match?)` → new etag.
- `create_screen(handle, name, template?)` → screen_ref.
- `copy_screen(from_ref, to_handle, to_name)` → screen_ref.
- `rename_screen(screen_ref, new_name)`, `delete_screen(screen_ref)`, `delete_screen_file(screen_ref, file)`.

**Render + diagnose**
- `render_screen(screen_ref, opts?)` → dithered PNG (as an MCP image content block) +
  optional raw PNG + `log` + returned `data` + `refresh_rate` + `error{line,message}`.
- `validate_screen(screen_ref)` → `ValidationReport`.

**Device assignment (write)**
- `assign_screen(mac, screen_ref)` → assigns a device to a screen. **Allowed even in
  add-on mode** (device writes are not global-config writes); global settings stay read-only.

### Resources (contracts, served not scaffolded)

- `meta.yaml` **JSON Schema** (generated from the `ScreenMeta` type).
- **Lua API reference** — the globals/functions byonk injects (`log_*`, `time_now`, HTTP
  fetch, fonts, the return-table contract: data, refresh, colors, dither, …).
- **SVG / base-library reference** — the `byonk-base-v1` includes, the blocks they expose,
  and the `{% extends/include %}` conventions.
- **Worked examples** — the `examples` set, readable as reference.

An agent thus learns the rules from the server it is editing, needing no local scaffolding
and no filesystem access.

## Error handling

- Path traversal / symlink escape → rejected before any IO, typed error.
- Write to a read-only handle → typed error naming `copy_screen` as the fix.
- Etag mismatch on `write_file` → `Conflict` (agent/UI re-reads and retries).
- Lua compile/runtime error → structured `{line, message}`, never a 500 opaque blob.
- No admin token configured → both `/mcp` and screen-write admin routes are 404 (invisible),
  identical to existing admin behavior.
- Config with both `repo` and `path` on one entry, or a reserved handle → load-time error
  with a clear message; server refuses to start with an actionable log (matching existing
  config-validation behavior).

## Testing strategy

- **axum bump**: existing suite green; a smoke test that `/api/image/{hash}` and admin routes
  still resolve.
- **Typed sources**: unit tests that `writable_root()` is `None` for embedded/git and `Some`
  for local; that a write to `byonk-builtin/*` is rejected structurally; that `copy_screen`
  from a read-only source into `local` succeeds.
- **ScreenStore**: traversal/symlink-escape rejection; atomic write; etag conflict; create/
  copy/rename/delete round-trips + loader rebuild; validate catches a bad meta / Lua syntax
  error / unresolved include; render returns log + data + line-numbered Lua error.
- **Migration**: fixture `SCREENS_DIR` with `name: byonk-builtin` + a device pointing at a
  user screen and at a genuine builtin → manifest rewritten, only the user-screen ref
  rewritten, idempotent on re-run.
- **Seeding**: empty `SCREENS_DIR` seeds a `local` manifest (no screen copies); examples
  seeded once; deleting an example and restarting does not resurrect it.
- **MCP**: integration test driving `/mcp` with a Bearer token — list/read/write/render/
  assign happy paths; 404 when no token configured; 401 on bad token; write to read-only
  handle returns the typed error; resources list the schema + Lua/SVG references.
- **Gates**: `make check` (Rust) and `make docs` green. HA add-on options-schema change
  validated on the VM per the manifest-sync memory before claiming add-on parity.

## Documentation

- `docs/src/` — new "Authoring screens with an LLM / MCP" guide; update the screen-repos and
  configuration pages for the `path:` variant and the builtin/examples/base split; note the
  `/dev` → `/` transition is coming in spec 2 (do not remove `/dev` docs yet).
- `CHANGES.md` (Unreleased) — user-facing entries only (per memory
  `changelog-user-facing-only`): the MCP interface, the writable local repos + `path:`
  config, the builtin-vs-examples reorganization, and the one-time migration note.

## Rollout / ordering

1. axum 0.8 bump (own commit, green).
2. Typed sources + config `path:` variant.
3. `ScreenStore` (read/write/validate/render), no callers yet.
4. Builtin/examples split + seeding + migration.
5. MCP endpoint (tools + resources) on `ScreenStore`.
6. Docs + CHANGES.

Each step is independently reviewable and leaves the tree green.

## Open questions (none blocking)

- Etag hash choice (blake3 vs sha256) — implementation detail, pick blake3 (already fast, no
  new heavy dep) unless a dep already provides sha256 conveniently.
- Exact default `examples` path in the add-on vs. standalone — resolve during implementation
  against the add-on's `/config` layout.
