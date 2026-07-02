# Screen Packages — Design

**Date:** 2026-07-02
**Status:** Design (approved for spec review)
**Byonk version at time of writing:** 0.15.0

## 1. Problem & motivation

Today a "screen" is a flat pair of files in `screens/`: `<name>.lua` (data-fetch
logic plus an optional `@params` YAML block embedded in a Lua comment) and
`<name>.svg` (a Tera template). Screens resolve by bare filename from a single
global namespace that merges the compiled-in `rust-embed` assets with an
optional external `screens_dir` overlay.

This has three limitations we want to remove:

1. **No real screen metadata.** A screen's identity is just its filename. Title
   and description live in unparsed Lua comments that byonk never reads; the
   admin API only exposes `{ name, params, schema_error }`.
2. **No sharing between screens.** SVG reuse exists (Tera `{% extends %}` /
   `{% include %}` against `layouts/` and `components/`) but resolves from one
   flat global namespace. There is **no Lua sharing mechanism at all** — no
   `require`, no shared helper modules.
3. **No distribution story.** Screens ship only as part of byonk. There is no
   way for a community to publish, version, and share screens.

The goal is a **package format** where a git repo holds one or more screens plus
the shared Lua and SVG they use, described by structured YAML metadata, versioned
atomically, addressable and installable by repo + pin, with the official screens
shipping as one bundled package — and with old flat screens still readable.

## 2. Concepts & terminology

- **Package** — a git repo. It is the unit of distribution and versioning: an
  atomic, versioned tree identified by a repo URL + a pin. Everything a screen
  needs (its shared Lua/SVG) lives in the same repo and is therefore versioned
  together with it.
- **Screen** — any directory inside a package that contains a `meta.yaml`. Its
  path relative to the discovery root is the screen's name.
- **Registry** — a `packages:` table in byonk's config mapping a short **handle**
  to `{ repo, pin, token? }`. The same repo may be registered under multiple
  handles at different pins.
- **Screen reference** — `handle/path` (e.g. `weather/forecast`,
  `official/gphoto`). This is what a device is assigned, and what the admin API
  reports as a screen's canonical name.
- **`byonk-base-vN`** — a reserved namespace naming byonk's built-in "standard
  library" of shared SVG layouts/components and Lua helpers, versioned by the
  `-vN` suffix.

## 3. On-disk format

### 3.1 Package layout

Organization inside a repo is free. Any directory containing a `meta.yaml` is a
screen; everything else is shared support code referenced repo-relative.

```
acme-screens/                      # the repo (a package)
  byonk-screens.yaml               # MANDATORY package manifest (repo root)
  lib/weather_api.lua              # shared Lua, referenced repo-relative
  parts/panel.svg                  # shared SVG, referenced repo-relative
  weather/forecast/                # screen id: weather/forecast
    meta.yaml
    script.lua
    screen.svg
  weather/hourly/                  # screen id: weather/hourly
    meta.yaml
    script.lua
    screen.svg
```

File names are **fixed by convention** — `meta.yaml`, `script.lua`,
`screen.svg` — with no override. This keeps discovery unambiguous.

### 3.2 `byonk-screens.yaml` (package manifest, mandatory)

Lives at the repo root. Distinctively named so a general-purpose repo can also
host byonk screens without colliding with an existing `package.json`,
`Cargo.toml`, etc.

```yaml
# byonk-screens.yaml
name: acme-screens                 # MANDATORY
description: Weather and transit screens for TRMNL.   # MANDATORY
author: Acme <hi@acme.example>     # MANDATORY
license: MIT                       # MANDATORY
root: contrib/trmnl                # OPTIONAL: scope screen discovery to a subtree
```

- `name`, `description`, `author`, `license` are required. A repo missing the
  manifest, or missing any required field, is rejected at load with a clear
  error (except the synthesized legacy `local` package — see §7).
- `root:` (optional) bounds screen discovery to a subtree, so a large mixed repo
  need not be scanned in full. When present, screen ids are relative to `root:`;
  when absent, discovery scans the whole repo and ids are full repo-relative
  paths.

### 3.3 `meta.yaml` (per-screen, the single source of screen truth)

```yaml
# weather/forecast/meta.yaml
title: 5-Day Forecast
description: Daily high/low and conditions for a location.
byonk: "0.15"                      # engine compatibility (semver; see §6)
refresh: 900                       # OPTIONAL default; Lua return value still overrides
params:                            # same field schema as today's @params
  location:
    type: string
    label: Location
    required: true
  units:
    type: enum
    label: Units
    options: [metric, imperial]
    default: metric
```

- `title` and `description` are new, parsed, first-class metadata.
- `params` uses the **exact same field schema** byonk parses today (`ParamField`:
  `type`, `label`, `description`, `required`, `default`, `min`/`max`/`step`,
  `unit`, `mode`, `options`, `sensitive`, `multiline`, `hidden`, `advanced`). It
  simply moves out of the Lua comment and into structured YAML.
- `refresh` is an optional default; the Lua script's returned `refresh_rate`
  still overrides at runtime, matching today's precedence.

## 4. Sharing & resolution

### 4.1 Repo-relative sharing (the author's own code)

Within a package, a screen references shared code by a repo-relative path:

- **Lua:** `local api = require("lib/weather_api")`
- **SVG:** `{% include "parts/panel.svg" %}`, `{% extends "layouts/base.svg" %}`

Because these resolve within the repo, and the repo is one atomic versioned unit,
a screen and the shared elements it uses can never drift out of version sync.

### 4.2 `byonk-base-vN` (byonk's standard contract)

Paths beginning `byonk-base-vN/` resolve to byonk's built-in standard library —
the base SVG layout, `hinting.svg` (tightly coupled to byonk's font rendering and
needed by essentially every screen), and common Lua helpers. It is shipped
embedded in byonk and versioned by the `-vN` suffix, so a later `byonk-base-v2`
can change the contract without breaking screens written against `byonk-base-v1`.
byonk keeps multiple base versions available simultaneously.

```
{% extends "byonk-base-v1/base.svg" %}
{% include "byonk-base-v1/hinting.svg" %}
```
```lua
local std = require("byonk-base-v1/std")
```

### 4.3 Net-new: sandboxed Lua `require()`

There is no Lua sharing today. byonk installs a custom module searcher into the
Lua state that resolves module names against exactly two roots:

1. the **screen's own package** (repo-relative), and
2. the **`byonk-base-vN`** namespace.

No arbitrary filesystem access. Modules are loaded through the existing
`AssetLoader` (embedded + package cache + external overlay), loaded once, and
cached in `package.loaded` for the duration of a script run.

### 4.4 Per-repo-scoped template resolution

Tera resolution changes from "preload one global `layouts/` + `components/`
namespace" to per-render scoping: when rendering a screen, byonk registers that
screen's `screen.svg` plus its package's `.svg` files (by repo-relative name) plus
the `byonk-base-vN` templates. This prevents cross-package template-name
collisions while preserving `extends`/`include`.

## 5. Registry, addressing & the official package

```yaml
# byonk config
packages:
  official: {}                                          # built-in, embedded copy
  weather:      { repo: github.com/acme/screens, pin: v1.4.0 }
  weather-beta: { repo: github.com/acme/screens, pin: v2.0.0 }
  private:      { repo: github.com/acme/secret, pin: v1.0.0, token: ${GITHUB_TOKEN} }
```

- A screen is addressed as `handle/path`:

  ```yaml
  # device assignment
  screen: weather/forecast        # -> v1.4.0
  screen: weather-beta/forecast   # -> v2.0.0 (same repo, different handle)
  screen: official/gphoto
  ```

- **Two-level pinning** is achieved entirely through the registry: to run two
  versions of one repo, register it under two handles. There is no inline
  `@pin` grammar to parse; every `(repo, pin)` combination in use is an explicit,
  greppable registry entry. Per-screen pinning falls out because each
  device→screen assignment independently picks a handle.

- **The `official` package** — `official: {}` uses the copy **embedded** in byonk
  (`rust-embed`), so the official screens are always present, work offline, and
  are versioned with byonk itself. It may optionally be repointed to
  `{ repo, pin }` to track official screens ahead of a byonk release. All of
  today's built-in screens move under this package.

## 6. Compatibility

Two complementary, "declare-what-you-need, forward-compat-is-automatic"
mechanisms:

1. **`byonk-base-vN`** guards the **std include/template/helper contract** (what
   you `include`/`require` from the base namespace).
2. **`byonk:` in `meta.yaml`** guards the **engine / Lua-runtime API** (globals
   like `http_get`, `scale_pixel`, `scale_font`, dither modes) — things not
   captured by an SVG include path.

`byonk:` uses standard **semver requirement** semantics (Rust `semver` crate,
same rules as Cargo's `VersionReq`):

- A **bare version** means **caret**: `byonk: "1.2"` ⇒ `^1.2` ⇒ `>=1.2.0, <2.0.0`.
  The author declares only the minimum version they built against; the "works
  until the next major" ceiling is implicit from byonk's promise of no breaking
  changes within a major version.
- **Pre-1.0 (byonk is currently 0.x):** semver treats 0.x specially — the *minor*
  is the breaking boundary. `byonk: "0.15"` ⇒ `^0.15` ⇒ `>=0.15.0, <0.16.0`.
  Compatibility is therefore tighter until byonk reaches 1.0.
- **Explicit ranges** remain available as an escape hatch
  (`byonk: ">=0.14, <0.17"`), but the bare-version caret form is the documented
  default.

byonk checks the range at screen load; on mismatch it refuses the screen (or
warns) with a clear message.

## 7. Backward compatibility (legacy reader)

Existing installations keep working:

- **Loose flat pairs.** A `<name>.lua` + `<name>.svg` pair discovered outside any
  package is folded into a synthesized `local` package (`local/<name>`). This
  package is exempt from the mandatory-`byonk-screens.yaml` rule — byonk
  synthesizes it as a compatibility shim.
- **Legacy `@params`.** The existing `--[[ @params … ]]` extraction remains for
  legacy screens that carry their schema in the Lua comment. New packages put
  `params` in `meta.yaml`.
- **Legacy global include paths.** Old-style `{% include "components/hinting.svg" %}`
  / `{% extends "layouts/base.svg" %}` continue to resolve in a compatibility
  mode. New packages use `byonk-base-v1/…` and repo-relative paths.

## 8. Distribution

### 8.1 Fetch engine — `gix` (gitoxide, pure Rust)

byonk fetches packages using **gitoxide (`gix`)**, a pure-Rust git
implementation — no external `git` binary dependency, keeping byonk
self-contained and the HA add-on image lean. `git2` (libgit2 bindings) is the
known fallback if gix's auth turns out insufficient during implementation.

### 8.2 Pin semantics & re-fetch

- **Full commit sha** — truly immutable. Fetched once, cached permanently, never
  re-fetched.
- **Tag or branch** — treated as **mutable** (tags can be force-moved/re-pointed,
  so they are not safe to cache forever). Re-fetched on demand: an admin "update
  packages" action, and optionally on a configurable interval. **Never** silently
  re-fetched mid-serve. A re-fetch resolves the ref to its current sha.

### 8.3 Cache

A byonk-managed cache directory keyed by **repo + resolved sha**, so multiple
pins of one repo coexist and anything still pinned to an old sha keeps working
after a tag/branch moves. **Offline:** byonk serves from cache; a fetch failure
never takes down a screen that is already cached.

### 8.4 Auth

- **Default:** host git credentials — credential helpers, ssh-agent, `~/.netrc`
  — handled ambiently by gix. Public repos work everywhere; private repos work
  wherever the host is already authenticated.
- **Override:** an optional per-package `token` in the registry entry, used when
  the host is not pre-configured.

## 9. Impact on byonk internals

- **`assets.rs`** — a package-aware loader: resolve `handle/path` → package (via
  registry → cache/embedded/legacy overlay) → files. New responsibilities for
  discovering screens by `meta.yaml` marker (honoring `root:`).
- **`param_schema.rs`** — schema source moves to `meta.yaml`; keep `@params`
  extraction for the legacy reader. Field schema itself is unchanged.
- **`template_service.rs`** — per-repo scoped template registration plus
  `byonk-base-vN`; legacy global-namespace mode retained.
- **`lua_runtime.rs`** — install the sandboxed `require()` searcher (§4.3).
- **`api/admin/read.rs`** — `ScreenInfo` gains `title` and `description`; `name`
  becomes `handle/path`; screens grouped by package.
- **`models/config.rs`** — new `packages:` registry; device `screen` field uses
  `handle/path`.
- **New: a package/distribution service** — registry resolution, gix
  fetch/cache/update, pin semantics, auth.

## 10. Implementation phasing (for the plan, not this spec)

The design is unified, but implementation naturally splits:

1. **Format & loader** — `byonk-screens.yaml`, `meta.yaml`, folder-per-screen
   discovery, repo-relative + `byonk-base-v1` resolution, Lua `require()`, the
   registry schema, admin/config updates, the legacy reader. Packages placed on
   disk manually (plus the embedded official package); no git fetching yet.
2. **Distribution** — gix fetch/cache/update, pin semantics, offline behavior,
   host + token auth.

## 11. Open questions / risks

- **gix auth maturity.** Pure-Rust credential-helper/ssh support is less
  battle-tested than shelling out to `git`. Mitigation: `git2` fallback; validate
  private-repo fetch early in phase 2.
- **Legacy resolution overlap.** The legacy global include namespace and the new
  per-repo/`byonk-base-vN` scoping must coexist without surprising precedence.
  Needs explicit precedence rules in the plan.
- **Migration of official screens.** Moving all current screens under the
  `official` package and rewriting their includes to `byonk-base-v1/…` is
  mechanical but broad; the plan should cover it screen-by-screen.
