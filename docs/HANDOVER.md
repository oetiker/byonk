# Handover — Byonk

_Last updated: 2026-07-03 — **Screen Packages Plan 2 (distribution) is COMPLETE.** All 10 tasks + the final whole-branch Opus review + its fix wave are done, reviewed clean, and `make check`-green on branch `feat/screen-packages-p2-distribution` @ **`1b93e45`**. **Kept as a local branch — NOT pushed/merged** (user chose "keep as-is"). No Critical was ever found. Next session: decide integration (see "Integrating Plan 2"), and/or start **Plan 3 (HA Options Flow)**. Plan 1 is done (history on this branch). HA Phase-6 findings still open, lower priority (bottom)._

## TL;DR — resume here

- **Plan 2 (git-backed screen-package distribution) shipped on `feat/screen-packages-p2-distribution` @ `1b93e45`.** Executed with `superpowers:subagent-driven-development` (fresh implementer + reviewer per task, fix waves, ledger-tracked). `make check` green, working tree clean.
- **NOT pushed, NOT merged.** The branch's merge-base with `main` is `cfddbd4` (pre-Plan-1), so **`main` still lacks Plan 1 + HA Phase 4/5 too** — merging this branch to `main` brings in all of that plus Plan 2 (a large multi-feature merge). Reconcile that before merging.
- **What Plan 2 added (the distribution feature):** registered packages (a package = a git repo) are fetched at a pin, cached by repo+sha, served through a hot-swappable loader, refreshed periodically, and managed over the token-gated admin API.

## What shipped (Plan 2, Tasks 1–10, all reviewed clean)

Service layer:
- `src/services/git_fetch.rs` — gix clone/fetch, resolve pin→sha, export tree to a clean dir (no `.git`). All git errors redacted of credentials (`redact_userinfo`, incl. `PinNotFound`). `PinKind{Sha,Tag,Branch,Embedded}` (snake_case; `Embedded` is an API-layer-only marker).
- `src/services/package_cache.rs` — `PackageCache`, checkout dirs keyed by `repo+sha` (`checkout_dir`, `has` checks `byonk-screens.yaml`).
- `src/services/package_status.rs` — `PackageState{Ready,Fetching,Error,Offline}`, `PackageStatus` (all-Option).
- `src/services/package_manager.rs` — `PackageManager`: fetch orchestration, per-handle status store, hot-swappable `ArcSwap<PackageLoader>`. **All methods sync+blocking → call sites wrap in `spawn_blocking`.** Immutable-sha pins reused from disk; mutable tag/branch pins re-fetched; a fetch/install failure with a prior cached checkout → `Offline` (keeps serving). `refresh_one` **short-circuits `move_dir` when the resolved sha is already cached** (never tears down the live checkout). `forget_status(handle)` evicts on delete.

Wiring & API:
- `AppState.package_manager: Arc<PackageManager>` **replaced** `package_loader` crate-wide; all resolution via `state.package_manager.loader()` (fresh per request — no stale snapshots). `PACKAGES_CACHE_DIR` env (fallback `temp_dir()/byonk-packages`). `reload_config` calls `rebuild_loader()`. `build_package_manager` helper shared by server + CLI paths.
- `src/services/config_writer.rs` — comment-preserving `upsert_package`/`remove_package` (device helpers generalized with a `section` param).
- `AppConfig.package_refresh_interval: u64` (seconds; 0=disabled) + a periodic tokio task in `run_server` (spawned unconditionally; re-reads the interval each tick; `refresh_all(false)` in `spawn_blocking`; logs JoinError).
- Admin endpoints (`src/api/admin/{write,read,mod}.rs`): `POST /packages`, `PATCH/DELETE /packages/:handle`, `POST /packages/:handle/update`, `POST /packages/update`, and enriched `GET /packages` (pin_kind/resolved_sha/status/last_fetched/error). Shared `build_package_info` builder. **Token never serialized** (only `token_set`). Delete rejects builtin + any handle referenced by a device, `default_screen`, or `registration.screen`.
- `homeassistant/byonk/config.yaml` add-on manifest sets `PACKAGES_CACHE_DIR: /data/packages` (persistent). Docs in `docs/src/api/admin-api.md`, `docs/src/guide/ha-addon.md`, `CHANGES.md`.

## Integrating Plan 2 (decision pending)

The branch is kept as-is. To integrate later, use `superpowers:finishing-a-development-branch`. Options: push+PR against `main`, or merge locally. **Before either**, note the merge scope caveat above (Plan 1 + HA phases + Plan 2 all land together relative to `main`). If you want Plan 2 on `main` in isolation, you'd first need Plan 1 / HA-phase branches merged to `main` (or a rebase strategy).

**Post-merge live test (important):** the HA VM still runs the **Plan-1** build. After Plan 2 lands, re-run `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (Rust change → add-on rebuild), then `ha addons start local_byonk`. Verify with `curl :3000/api/admin/packages` → **401** (proves new binary live) and byonk `/health` → 200. **Never read/print the admin token.**

## Deferred Minors (non-blocking fast-follow — in the SDD ledger)

None are Critical or Important; the final review triaged all as fast-follow:
- **git_fetch:** symlinks written as plain files; submodule/gitlink entries skipped (documented); `dest` not cleaned on partial `fetch_into` failure (self-heals next fetch); SSH-URL auth untested.
- **config_writer:** no package test for the present-with-entries insert path (device path covers identical code); the section-absent arm now creates a missing `devices:`/`packages:` section instead of erroring (untested, benign); no package test asserts comment preservation for packages.
- **endpoints/tests:** the `admin_packages_test` cases POST/PATCH real repo strings, firing an un-awaited `spawn_blocking(refresh_one)` → a real git fetch to a nonexistent remote (possible CI latency/flakiness — inject a fake fetcher).
- **PackageManager:** missing-pin defaults to `"main"` (live default — pin not required at register).

## Next initiative — Plan 3 (HA Options Flow), NOT WRITTEN

Spec §9a.4. Move package config (repo/pin/token registry, default screen, auth mode) into a config-entry Options Flow; add hub-device status entities (per-package fetch-status sensors + an "Update packages" button). **Depends on Plan 2's write API (now shipped).** Start with `superpowers:brainstorming` → spec → `superpowers:writing-plans`.

---

## Screen Packages — the big picture (spec + plans)

- **Spec:** `docs/superpowers/specs/2026-07-02-screen-packages-design.md` (format, sharing/resolution, registry/addressing, compat, **distribution §8**, **admin API §9a**, HA config placement §9a.4, migration §7).
- **A package = a git repo** (atomic versioned unit); **a screen = any dir with `meta.yaml`** (+ `script.lua` + `screen.svg`), addressed **`handle/path`**. Repo root has mandatory `byonk-screens.yaml`. **`byonk-builtin`** is embedded (rust-embed), always registered, never fetched, cannot be deleted. Registry = `packages:` in config (`handle → {repo, pin, token?}`).
- **Plan 1 (format & loader): DONE.** 13 tasks, whole-branch review clean; 11 built-ins migrated into the embedded `byonk-builtin` package. Clean break — no legacy reader, no bare names, no `@params`. History on this branch (details in git + `.superpowers/sdd/progress.md` Plan 1 section).
- **Plan 2 (distribution): DONE** (this handover) — `docs/superpowers/plans/2026-07-03-screen-packages-p2-distribution.md`. Global Constraints block (pin semantics, cache, auth/redaction, `byonk-builtin`, `PackageInfo` shape) at the top.
- **Plan 3 (HA Options Flow): NOT WRITTEN** (spec §9a.4) — see above.

## The admin API (token-gated `/api/admin/*`; bearer; 404 when no token, 401 when wrong)

| Method + path | Purpose |
|---|---|
| `GET /devices` · `GET /pending` · `GET /config` (secrets stripped) | device/config reads |
| `GET /screens` | package-grouped screens + panels + dither_algorithms |
| `GET /packages` | registered packages; enriched with `pin_kind`/`resolved_sha`/`status`/`last_fetched`/`error` (+ `handle,repo,pin,builtin,token_set,screen_count`) |
| `POST/PATCH/DELETE /devices[/:key]` | device→screen mapping |
| `PATCH /settings` | registration, auth_mode, default_screen, registration_screen, **package_refresh_interval** |
| `POST /packages` · `PATCH/DELETE /packages/:handle` · `POST /packages/:handle/update` · `POST /packages/update` | package register/patch/delete/refresh (update endpoints are fire-and-forget — client polls `GET /packages`) |

Comment-preserving `config.yaml` writes via `config_writer` (device- and package-keyed helpers). Hot-reload via `reload_config` (arc-swap) + `rebuild_loader`.

## Build / verify

- `make check` — Rust fmt + clippy (`-D warnings`) + tests. **Green at HEAD `1b93e45`.**
- `make ha-setup` (one-time: `.venv` via uv, **Py ≤ 3.13**), then `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`). `tests_ha/` isn't in the ruff target — run `.venv/bin/ruff check tests_ha` separately.
- `make docs` — mdBook build (green).
- Env vars: `CONFIG_FILE`, `SCREENS_DIR`, `FONTS_DIR`, **`PACKAGES_DIR`** (on-disk dev packages, `handle → <dir>/<handle>`), **`PACKAGES_CACHE_DIR`** (git checkout cache; add-on → `/data/packages`).

## Config files (important distinction)

- **`config.yaml`** = developer's local test config (demo devices). Used by `make run`/`watch`.
- **`default-config.yaml`** = shipped/embedded default (device-free; `screens: {}`; `default_screen: byonk-builtin/default`). Embedded by `src/assets.rs` + copied into Docker images.

## Deploying to the HA VM (Rust change → add-on rebuild)

- **byonk server:** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (rsyncs source + `SCREENS_DIR` + `ha addons rebuild`). Gotchas: `rebuild.sh` syncs `byonk-base/`; the add-on **Dockerfile** needs `COPY byonk-base ./byonk-base`. **Durability gap: the add-on Dockerfile is NOT tracked in the repo** — a VM rebuild-from-scratch regresses it. `ha addons …` is deprecated → `ha apps …`. A rebuild can leave the add-on stopped — `ha addons start local_byonk` after. Transient Docker Hub 500s — retry. Plan 2 add-on run config now carries `PACKAGES_CACHE_DIR=/data/packages` (tracked in `homeassistant/byonk/config.yaml`).
- **Integration:** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`, then `make ha-ssh CMD="ha core restart"`.
- **Verify without the token:** `curl :3000/api/admin/packages` → **401** proves the new binary is live; `/health` → 200. **Never read/print the admin token.**

## Current VM state

HAOS VM (qemu detached); HA `:8123` (German UI, owner `byonk`/`byonk`). `local_byonk` add-on on `:3000` = **byonk `0.16.0-dev`, Plan-1 package format**, `state: started`. **This VM does NOT yet have Plan 2** — re-`make ha-rebuild` after Plan 2 lands to test distribution live.

## Test harness (`tools/ha-vm/`)

Scripted headless HAOS in QEMU on macOS (Apple Silicon). See `tools/ha-vm/README.md`.
- **Boot:** `make ha-vm` (hostfwd 8123/3000/4445/2222). Stop `make ha-vm-stop`; reset `make ha-vm-clean`. Detached: `nohup make ha-vm > tools/ha-vm/work/boot.log 2>&1 & disown`.
- **SSH** (key at `tools/ha-vm/ssh/` — gitignored, never commit): `make ha-ssh` / `make ha-ssh CMD="…"`.
- **Samba** (creds `byonk`/`byonk`, port 4445): `addons` (build context + Dockerfile), `addon_configs` (runtime config + `screens/`), `config` (HA `/config`).
- **Note:** the add-on's own `config.yaml`/Dockerfile under `tools/ha-vm/work/` is **gitignored staging** — the tracked add-on manifest is `homeassistant/byonk/config.yaml`.

## Reference docs & ledger

- Spec: `docs/superpowers/specs/2026-07-02-screen-packages-design.md`
- Plans: `…/plans/2026-07-02-screen-packages-p1-format-loader.md` (done), `…/plans/2026-07-03-screen-packages-p2-distribution.md` (done).
- SDD ledger (git-ignored): `.superpowers/sdd/progress.md` — Plan 1 + Plan 2 per-task reviews, commit ranges, the final review + fix wave, and the consolidated deferred-Minors list.
- HA phase specs/plans: `…/2026-06-{28,29,30}-byonk-homeassistant-phase{1..6}-*.md`; user docs `docs/src/…`; harness `tools/ha-vm/README.md`.

---

## Still open — HA device-page findings (Phase 6 candidate, lower priority)

Independent of screen-packages, still unaddressed on this branch:
1. **BUG — per-device Panel/Dither write but don't read back.** Selects write to byonk (on disk) but HA shows "unknown"; Screen/Model/telemetry DO read back. Needs a Rust integration test (POST device → PATCH `{"panel":"trmnl_x"}` → GET assert). Files: `src/api/admin/{read,write}.rs`, `src/models/config.rs`, `src/server.rs`; HA `coordinator.py`/`select.py`.
2. **Model shows "og" for a reTerminal E1002** — detection from Board/Model header (`src/models/device.rs`).
3. **RSSI sensor hidden** (`entity_registry_enabled_default=False` in `sensor.py`) — enable it.
4. **Per-device refresh override** — wants design (precedence: per-device > Lua-returned > screen `meta.refresh`); overlaps Plan-1 follow-up #4. Brainstorm → spec first.

## Deferred / fast-follow (non-blocking)

- **Plan 2 deferred Minors** (above / in the ledger).
- **Plan 1 follow-ups:** #4 refresh precedence (new author's `meta.refresh` ignored unless Lua returns 0); #5 test-only `AssetScreensSource::read` lacks `is_safe_rel` guard.
- **Add-on Dockerfile not tracked in-repo** (durability) — `byonk-base` COPY lives only in the VM + gitignored staging.
- **HA earlier-phase minors:** `require_admin` → middleware; `config_writer::set_scalar` swallows Replace error before Add; strike-dict stale-count micro-leak; `AddonOptions` redacting `Debug`.
- **Phase 4 (not started):** add-on `version:` automation, HACS list + `home-assistant/brands` prep; a real byonk **release** so a published image exists (only then can the published-image add-on path be validated).
