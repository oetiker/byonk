# Handover — Byonk

_Last updated: 2026-07-03 — **Screen Packages Plan 1 (format & loader) is implemented, fully reviewed, and live-deployed** to the HA test VM. **Plan 2 (distribution/git-fetch) is written and ready to execute.** Everything lives on branch `feat/screen-packages-p1` (committed, **NOT pushed/merged** — user chose "keep as-is, continue working"). Next session: **execute Plan 2** (or the small correctness follow-ups first). The older HA device-page findings (Phase 6) are still open but lower priority — see the bottom._

## TL;DR for the next session

- **Active initiative: Screen Packages** — a package format for screens so a community can publish/version/share them. One coherent spec, phased into 3 plans.
  - **Plan 1 (format & loader): DONE** — implemented (13 tasks, subagent-driven, every task + a whole-branch opus review), `make check` green, **deployed & validated on the HA test VM**.
  - **Plan 2 (distribution): WRITTEN, ready to execute** — gix git-fetch/cache/refresh + package-management API. `docs/superpowers/plans/2026-07-03-screen-packages-p2-distribution.md`.
  - **Plan 3 (HA Options Flow): not yet written** — the proper HA package-management UI (spec §9a.4). Today's integration change was a read-side stopgap.
- **Branch `feat/screen-packages-p1`** was cut from `feat/ha-phase4-release-docs`, so it **also carries all HA Phase 4/5 work** as ancestors. Nothing is pushed or merged. HEAD `431f20f`.
- **byonk is now `0.16.0-dev`** and screens are **packages**: a screen is a folder `meta.yaml` + `script.lua` + `screen.svg`, addressed as **`handle/path`** (e.g. `byonk-builtin/useful/gphoto`). The 11 built-in screens were migrated/renamed into the embedded **`byonk-builtin`** package. **Clean break — no legacy reader, no bare names, no `@params` comments.**
- **The admin `/screens` API shape changed** (now package-grouped) and **`GET/POST/… /api/admin/packages`** was added. The HA integration's coordinator was updated to match.
- **`make check` green.** VM is running the new build with a cleaned config + updated integration (see "Current VM state").

## Where to start

1. **Read the spec:** `docs/superpowers/specs/2026-07-02-screen-packages-design.md` (format, sharing/resolution, registry/addressing, compat, distribution §8, admin API §9a, HA config placement §9a.4, migration §7).
2. **Read the SDD ledger:** `.superpowers/sdd/progress.md` — Plan 1's per-task review log, commit ranges, the final whole-branch review verdict, and the tracked follow-ups (#4–#6 + minors).
3. **Execute Plan 2** via `superpowers:subagent-driven-development` (recommended) — the plan doc is code-complete except the `git_fetch` gix spike (Task 1), which has a git2 fallback gate. **Branch:** Plan 2 depends on Plan 1, so branch from `feat/screen-packages-p1`.

## Screen Packages — Plan 1: what shipped (commits `f7b244c..48b0279`)

Branch `feat/screen-packages-p1`. Design decisions (all in the spec):

- **Package = a git repo**, atomic versioned unit. **Screen = any dir with a `meta.yaml`**; its repo-relative path is the screen name. Files fixed: `meta.yaml` (title/description/`byonk` semver/params), `script.lua`, `screen.svg`. Mandatory repo-root `byonk-screens.yaml` (name/description/author/license, optional `root:`).
- **Addressing:** `handle/path`. **Registry** = `packages:` in config (`handle → {repo, pin, token?}`); same repo under multiple handles = two-level pinning. **`byonk-builtin`** is embedded (rust-embed), always registered, never fetched.
- **Sharing:** repo-relative `require("lib/x")` / `{% include "parts/y.svg" %}` + a path-versioned **`byonk-base-v1/…`** std namespace (embedded base layout/hinting/helpers). Net-new **sandboxed Lua `require()`** with an `is_safe_rel` path-traversal guard (covers require, includes, image-refs, `read_asset` — all funnel through `PackageSource::read`). Per-package Tera template scoping.
- **Compat:** `byonk:` field, semver, **bare = caret** (`"0.15"` ⇒ `^0.15`); mismatch **warns, never blocks**.
- **Migration (spec §7 map):** `gphoto`→`byonk-builtin/useful/gphoto`, `transit`→`.../useful/swiss-departure-board`, `calibrator`→`.../calibration/color`, `graytest`→`.../calibration/grey`, `hintdemo`→`.../demo/font/hinting`, `fontdemo-bitmap`→`.../demo/font/bitmap`, `fontdemo-terminus`→`.../demo/font/ttf`, `hello`→`.../example/hello`, `mandelbrot`→`.../example/mandelbrot`, `floerli`→`.../example/webscrape`, `default`→`byonk-builtin/default`.

**Key source (post-Plan-1):**
- `src/services/package_loader.rs` — `PackageSource` trait (`read`/`read_string`/`screen_paths`/`svg_files`/`manifest`), `ResolvedScreen`, `DiskPackageSource`, `EmbeddedBuiltinSource`, `PackageLoader` (**immutable** `registry: HashMap<String, Arc<dyn PackageSource>>`; `new`/`resolve`/`list_all`/`handles`), `is_safe_rel`, `join_rel`, `BUILTIN_HANDLE`.
- `src/models/{screen_meta,package_manifest,compat}.rs` — meta.yaml / byonk-screens.yaml / semver compat parsers.
- `src/models/config.rs` — `PackageRef` + `packages` field; `default_screen()` = `byonk-builtin/default`; the old flat-file `ScreenConfig`/`screens`/`get_screen_*` surface was **removed**.
- `byonk-base/v1/*.svg` — embedded std assets (`EmbeddedBase` in `src/assets.rs`; `EmbeddedScreens` now embeds `*.yaml` too).
- `src/services/{lua_runtime,template_service,content_pipeline}.rs` — resolve/run/render through `PackageLoader`. Two distinct `ScriptResult` types (lua raw vs pipeline enriched) — don't conflate.
- `src/api/admin/{read,write,mod}.rs` — `/screens` package-grouped; `GET /packages`; ref validation via the loader; `get_config` redacts `admin.token` **and** `packages.*.token`.

**Final whole-branch review** (opus): *Ready to merge — no Critical, sandbox + token redaction solid.* Two Important items were fixed before we stopped: dead `config.rs` resolution code removed; user-facing docs updated to the package model.

**Tracked follow-ups** (non-blocking, in the ledger):
- **#4 refresh precedence** — a *new* package author's `meta.refresh` (≠900) is ignored unless the Lua returns `0` (contradicts spec §3.3; pre-existing behavior, all 11 built-ins set it explicitly). Fix = have `run_script` report "unset" so `meta.refresh`/device override can win.
- **#5** test-only `AssetScreensSource::read` lacks the `is_safe_rel` guard (not production-reachable).
- **#6** `PackageLoader` isn't rebuilt on config reload — **Plan 2 fixes this** (PackageManager + ArcSwap).
- Minors: `demo/font/ttf/screen.svg` header text reads "Bitmap Fonts" (pre-existing — the two fontdemo SVGs were byte-identical); missing `pub use` re-exports for the new model types; a redundant `#[serde(default)]`; a couple of doc comments.

## Screen Packages — Plan 2: distribution (WRITTEN, next)

`docs/superpowers/plans/2026-07-03-screen-packages-p2-distribution.md` — 10 TDD tasks:
`git_fetch` (gix; **git2 fallback gate** in Task 1) → package cache (repo+sha) → status types → **`PackageManager`** (fetch orchestration + **hot-swappable loader via `ArcSwap`**, fixing #6) → `AppState` wiring (replaces `package_loader` with `package_manager`; pipeline resolves through it) → `config_writer` `upsert_package`/`remove_package` (device fns are the template) → `package_refresh_interval` + periodic tokio task (first production background task; `spawn_blocking` for blocking git) → register/patch/delete/update endpoints → enriched `GET /packages` status → docs.

**Two things to eyeball before executing:** the gix-spike/git2 gate (Task 1), and whether `PACKAGES_CACHE_DIR` → the add-on's persistent `/data` is right (else the cache is a temp dir re-fetched after restart).

## Screen Packages — Plan 3: HA Options Flow (not written)

Spec §9a.4. **There is no byonk admin UI** — the HA integration is the config front-end. Move package config (repo/pin/token registry, default screen, auth mode) into a config-entry **Options Flow** (secrets belong here, never as entities); add hub-device **status entities** (per-package fetch-status sensors + an "Update packages" button). Depends on Plan 2's write API. The byonk hub device stays (bridge pattern) but stops being a settings panel.

## Deploying screen-packages to the HA VM (what we learned)

A screen-packages change is a **Rust** change → needs an **add-on rebuild**, not just a screen-file sync.

- **byonk server:** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (rsyncs source + `SCREENS_DIR` + `ha addons rebuild`). **Gotchas we hit & fixed:**
  - `rebuild.sh` must sync **`byonk-base/`** (added; embedded at compile time) — else `RustEmbed folder byonk-base/ does not exist`.
  - The add-on **Dockerfile** needs `COPY byonk-base ./byonk-base` in the builder stage (fixed in the live VM `addons/byonk/Dockerfile` + gitignored staging `tools/ha-vm/work/addon-staging/byonk/Dockerfile`). **Durability gap:** the add-on Dockerfile is **not tracked in the repo** — a VM rebuild-from-scratch would regress it. Worth tracking it (follow-up).
  - `ha addons …` is **deprecated → `ha apps …`** (still works, just warns).
  - A rebuild can leave the add-on **stopped** (`state: error`) — run `ha addons start local_byonk` after. Transient **Docker Hub 500s** on the base image happen — just retry.
- **Integration:** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`, then **restart HA core** (`make ha-ssh CMD="ha core restart"`) to load changed Python. The coordinator was updated to flatten the new `packages[].screens[]` and key off `ref` (`custom_components/byonk/coordinator.py`); `tests_ha` updated (65 pass). Everything else in the integration went through `screen_names()`/`screen_params()` unchanged.
- **Cleaning residual config:** old device configs reference **old bare screen names** which no longer resolve. Edit `addon_configs/local_byonk/config.yaml` over Samba (redact `token:` when viewing) — remap `screen:` values to `byonk-builtin/…`, drop any legacy `screens:` map, set `default_screen: byonk-builtin/default`. **Never read/print the admin token** — verify via the HA UI / entities.
- **Verify without the token:** `curl :3000/api/admin/packages` → **401** proves the *new* binary is live (that route is Plan-1-new); byonk `/health` → 200; the HA coordinator logs only on failure, so no post-restart error = successful `/screens` parse.

## Current VM state

HAOS VM running (qemu detached); HA `:8123` (German UI, owner `byonk`/`byonk`). `local_byonk` add-on on `:3000` = **byonk `0.16.0-dev`, package format**, `state: started`. Config cleaned: `default_screen: byonk-builtin/default`, 4 devices remapped (`gphoto`→`byonk-builtin/useful/gphoto` ×2, `floerli`→`byonk-builtin/example/webscrape`, `fontdemo-bitmap`→`byonk-builtin/demo/font/bitmap`; params preserved; backup at `config.yaml.bak-prepkg`). HA has the updated integration loaded (hub "Byonk" + device entries) and was restarted. Physical devices render the new screens on their next poll.

## Test harness (`tools/ha-vm/`)

Scripted headless HAOS in QEMU on macOS (Apple Silicon). See `tools/ha-vm/README.md`.

- **Boot:** `make ha-vm` (hostfwd 8123/3000/4445/2222). Stop `make ha-vm-stop`; reset `make ha-vm-clean`. Detached: `nohup make ha-vm > tools/ha-vm/work/boot.log 2>&1 & disown`.
- **SSH (Terminal & SSH add-on, key at `tools/ha-vm/ssh/` — gitignored, never commit):** `make ha-ssh` (shell) / `make ha-ssh CMD="…"` (one cmd). Handy: `ha addons info local_byonk`, `ha addons logs local_byonk`, `ha supervisor logs`, `ha core restart`.
- **Samba shares** (creds `byonk`/`byonk`, port 4445): `addons` = `/addons` (add-on build context + Dockerfile); `addon_configs` = per-add-on `/config` — **byonk's runtime config is `addon_configs/local_byonk/config.yaml`**, its runtime screens `addon_configs/local_byonk/screens/`; `config` = HA `/config` (integration → `custom_components/byonk`).
- **Local `local_byonk` add-on** (from-source build; no published image has Plan 1+): builder does `COPY … byonk-base default-config.yaml` etc. + `apk add build-base musl-dev perl curl`. Re-stage/rebuild via `make ha-rebuild` (handles rsync + `ha addons rebuild`).
- **Driving HA from Chrome** (`mcp__claude-in-chrome` + the page `hass` object): `config_entries/get`, `supervisor/api` (`/addons/local_byonk/{info,rebuild,restart}`); REST via `hass.callApi`. **Token** in the add-on options, usable in-page as `window.__tok`; **byonk's admin API is CORS-blocked from the HA page** — check host-side via `curl :3000/api/admin/…`.

## Build / verify

- `make check` — Rust fmt + clippy + tests.
- `make ha-setup` (one-time: `.venv` via uv, **Py ≤ 3.13**), then `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`, 65 tests). `tests_ha/` is **not** in the ruff target — run `.venv/bin/ruff check tests_ha` separately.
- `make docs` — mdBook build.
- Env vars: `CONFIG_FILE`, `SCREENS_DIR`, `FONTS_DIR`, **`PACKAGES_DIR`** (Plan 1: on-disk non-builtin packages, `handle → <dir>/<handle>`), **`PACKAGES_CACHE_DIR`** (Plan 2: git checkout cache).

## Config files (important distinction)

- **`config.yaml`** = developer's local test config (has demo devices). Used by `make run`/`watch`.
- **`default-config.yaml`** = shipped/embedded default (device-free; `screens: {}`; `default_screen: byonk-builtin/default`). Embedded by `src/assets.rs` + copied into Docker images.

## The admin API the integration consumes (post-Plan-1)

Token-gated `/api/admin/*` (bearer; 404 when no token configured, 401 when wrong):

| Method + path | Purpose |
|---|---|
| `GET /api/admin/devices` | configured + seen devices, telemetry + resolved screen/dither/panel |
| `GET /api/admin/pending` | connected-but-unregistered devices (+ code) |
| `GET /api/admin/config` | effective config JSON (**`admin.token` + `packages.*.token` stripped**) |
| `GET /api/admin/screens` | **package-grouped**: `{ packages: [{handle,name,description,author,license, screens:[{ref,title,description,params,byonk,compat_warning}]}], panels, dither_algorithms }` |
| `GET /api/admin/packages` | registered packages: `{handle,repo,pin,builtin,token_set,screen_count,status}` (Plan 2 adds `pin_kind`/`resolved_sha`/`last_fetched`/`error`) |
| `POST/PATCH/DELETE /api/admin/devices[/:key]` | device→screen mapping (`screen` is a `handle/path` ref) |
| `PATCH /api/admin/settings` | registration, auth_mode, default_screen, registration_screen |
| **Plan 2:** `POST/PATCH/DELETE /packages[/:handle]`, `POST /packages[/:handle]/update` | package management |

Comment-preserving `config.yaml` writes via `config_writer` (device-keyed helpers today; Plan 2 adds package-keyed); hot-reload via `reload_config` (arc-swap — **note it does not yet rebuild the package loader; Plan 2 fixes**).

## Reference docs

- **Screen-packages spec:** `docs/superpowers/specs/2026-07-02-screen-packages-design.md`
- **Plans:** `…/plans/2026-07-02-screen-packages-p1-format-loader.md` (done), `…/plans/2026-07-03-screen-packages-p2-distribution.md` (next)
- **SDD ledger (git-ignored):** `.superpowers/sdd/progress.md` — Plan 1 per-task reviews, commit ranges, final review, follow-ups.
- HA phase specs/plans: `…/2026-06-{28,29,30}-byonk-homeassistant-phase{1..6}-*.md`; user docs `docs/src/…`; harness `tools/ha-vm/README.md`.

---

## Still open — HA device-page findings (Phase 6 candidate, lower priority)

Surfaced during Phase-5 live testing; **independent of screen-packages** and still unaddressed on this branch. Detail in the ledger / prior handover history.

1. **BUG — per-device Panel/Dither write but don't read back.** Setting Panel/Dither via the device selects writes to byonk (on disk) but HA shows "unknown"; Screen/Model/telemetry DO read back. Root cause not found by inspection — needs a Rust integration test (POST device → `PATCH …/devices/:key {"panel":"trmnl_x"}` → `GET …/devices` assert `panel=="trmnl_x"`). Files: `src/api/admin/{read,write}.rs`, `src/models/config.rs`, `src/server.rs` (`reload_config`); HA `coordinator.py`/`select.py`.
2. **Model shows "og" for a reTerminal E1002** — model detection from the Board/Model header (`src/models/device.rs`).
3. **RSSI sensor hidden** (`entity_registry_enabled_default=False` in `sensor.py`) — enable it; consider exposing more last-request metadata.
4. **Per-device refresh override** — wants design (precedence: per-device > Lua-returned > screen `meta.refresh`); overlaps Plan-1 follow-up #4. Brainstorm → spec first.

## Deferred / fast-follow (non-blocking)

- **Screen-packages:** follow-ups #4–#6 + minors above.
- **Add-on Dockerfile not tracked in-repo** (durability) — the `byonk-base` COPY lives only in the VM + gitignored staging.
- **HA Phase-5 minors / earlier phases:** `require_admin` → middleware; `config_writer::set_scalar` swallows the Replace error before Add (unreachable today); strike-dict stale-count micro-leak; `AddonOptions` redacting `Debug`.
- **Still open from Phase 4 (not started):** add-on `version:` automation, HACS list + `home-assistant/brands` prep; a real byonk **release** so a published image exists (only then can the published-image add-on path be validated vs the from-source local build).
