# Handover — Byonk

_Last updated: 2026-07-03 — **Screen Packages Plan 2 (distribution) is HALF DONE.** The core services layer (Tasks 1–4: git_fetch → cache → status → PackageManager) is implemented, reviewed, and green on branch `feat/screen-packages-p2-distribution`. **Tasks 5–10 remain** (AppState wiring → config_writer → refresh interval → write endpoints → GET status → docs). Everything is committed, **NOT pushed/merged**. HEAD `d9380c5`. Next session: **resume at Task 5** via subagent-driven-development. Plan 1 is done+deployed (history). HA Phase-6 findings still open, lower priority (bottom)._

## TL;DR — resume here

- **Active work: Screen Packages Plan 2 (distribution).** Being executed with `superpowers:subagent-driven-development` (fresh implementer + reviewer subagent per task, fix-wave loop, ledger-tracked).
- **Branch `feat/screen-packages-p2-distribution`** cut from Plan 1's HEAD `9834bf7`. So it carries all Plan 1 + HA Phase 4/5 as ancestors. **HEAD `d9380c5`. Nothing pushed/merged.**
- **DONE (Tasks 1–4, core services, all reviewed clean + `make check` green):**
  1. `src/services/git_fetch.rs` — gix clone/fetch, resolve pin→sha, export tree to a clean dir (no `.git`). **No git2 fallback needed.**
  2. `src/services/package_cache.rs` — checkout dirs keyed by repo+sha.
  3. `src/services/package_status.rs` — `PackageState`/`PackageStatus` types.
  4. `src/services/package_manager.rs` — `PackageManager`: fetch orchestration + per-handle status store + **hot-swappable `ArcSwap<PackageLoader>`** (this is the fix for Plan-1 follow-up #6).
- **TODO (Tasks 5–10):** 5 AppState wiring · 6 config_writer upsert/remove_package · 7 package_refresh_interval + periodic tokio task · 8 package write endpoints · 9 enrich `GET /packages` · 10 docs/CHANGES/deploy.
- **All task briefs already generated** in `.superpowers/sdd/task-{5..10}-brief.md` (the `subagent-driven-development` skill's `scripts/task-brief` regenerates them if needed).

## How to resume (do this first)

1. **Read the plan:** `docs/superpowers/plans/2026-07-03-screen-packages-p2-distribution.md` — 10 tasks, code-complete, TDD. Global Constraints block at top (pin semantics, cache, auth, PackageInfo shape) is the reviewer's attention lens.
2. **Read the ledger:** `.superpowers/sdd/progress.md` — the **Plan 2 section at the bottom** has per-task review status, commit ranges, and tracked Minors. Trust it + `git log` over memory. Tasks 1–4 are marked COMPLETE — **do not re-dispatch them.**
3. **Invoke `superpowers:subagent-driven-development`** and continue at **Task 5**. Per task: `scripts/task-brief PLAN N` → dispatch implementer (model per complexity) → `scripts/review-package BASE HEAD` (BASE = pre-task commit from ledger, never `HEAD~1`) → dispatch reviewer → fix-wave loop on Critical/Important → mark complete in ledger.
4. After Task 10: **final whole-branch review** (opus) over `scripts/review-package 9834bf7 HEAD`, then `superpowers:finishing-a-development-branch`.

## Plan 2 service interfaces (what Tasks 5–10 build on)

Verified signatures from the committed Tasks 1–4 — Task 5 wiring needs these:

- **git_fetch** (`src/services/git_fetch.rs`): `pub fn fetch(repo, pin, token: Option<&str>, dest: &Path) -> Result<FetchOutcome, FetchError>`; `pub fn looks_like_sha(pin) -> bool`; `PinKind{Sha,Tag,Branch}` (serde snake_case); `FetchOutcome{resolved_sha,pin_kind}`; `FetchError{Git(String),PinNotFound(String,String)}`. **All git error strings are redacted** through `git_err`/`redact_userinfo` (strips `user:pass@` userinfo) — tokens never leak into errors.
- **package_cache**: `PackageCache::new(root: PathBuf)`, `.checkout_dir(repo,sha)->PathBuf` (= `root/<sha256(repo)[..8]>/<sha>`), `.has(repo,sha)->bool` (checks `byonk-screens.yaml` exists).
- **package_status**: `PackageState{Ready,Fetching,Error,Offline}` (snake_case); `PackageStatus{state:Option<PackageState>, resolved_sha:Option<String>, last_fetched:Option<DateTime<Utc>>, error:Option<String>, pin_kind:Option<PinKind>}` (Default = all None).
- **PackageManager** (`src/services/package_manager.rs`): `PackageManager::new(asset_loader: Arc<AssetLoader>, config: SharedConfig, cache: PackageCache, extra_disk: HashMap<String,PathBuf>) -> Arc<Self>`; `.loader() -> Arc<PackageLoader>` (cheap per-resolve snapshot via `ArcSwap::load_full`); `.refresh_one(&handle)`; `.refresh_all(force: bool)`; `.rebuild_loader()`; `.status_snapshot() -> HashMap<String,PackageStatus>`. **Methods are sync + blocking** — call sites (Tasks 7/8) MUST wrap in `tokio::task::spawn_blocking`. Never panic (poison-safe locking; mutex never held across the blocking fetch).
  - Behaviors locked in by review: immutable-sha pins are reused from disk (survive restart, never re-fetched); `refresh_all(false)` skips only immutable-sha-cached handles so **tag/branch pins re-fetch every tick** (Task 7 depends on this); a fetch/install failure with a prior cached checkout → `Offline` (keeps serving), else `Error`; missing pin currently defaults to `"main"` (Minor — see below).

### Plan-1 interfaces the wiring touches (unchanged)
- `PackageLoader::new(asset_loader: Arc<AssetLoader>, disk_packages: HashMap<String,PathBuf>) -> Self` (adds `byonk-builtin` embedded inside `new`); `.resolve(&str)`, `.list_all()`, `.handles()`. `BUILTIN_HANDLE = "byonk-builtin"`.
- `src/server.rs`: today `AppState.package_loader: Arc<PackageLoader>`, built by `build_package_loader(asset_loader, &config)` + `collect_disk_packages(dir, &config.packages)` from `PACKAGES_DIR`; `create_app_state_with_overrides`; `reload_config`. **Task 5 replaces `package_loader` with `package_manager` crate-wide.**
- `src/services/content_pipeline.rs` holds an `Arc<PackageLoader>` and calls `.resolve(...)` — Task 5 changes it to hold `Arc<PackageManager>` and call `.loader().resolve(...)` per request.
- `src/models/config.rs`: `PackageRef{repo:Option<String>,pin:Option<String>,token:Option<String>}`; `AppConfig.packages: HashMap<String,PackageRef>`; `SharedConfig = Arc<ArcSwap<AppConfig>>`.

## Tracked Minors (non-blocking — final-review triage)

From Tasks 1–4 reviews, deferred not dropped (all in the ledger):
- **git_fetch:** symlinks written as plain files; submodule/gitlink entries skipped (documented); `dest` not cleaned on partial `fetch_into` failure (self-heals next fetch); SSH-URL auth untested.
- **PackageManager:** missing-pin defaults to `"main"` (confirm vs Task 8 whether `pin` is required at register — could be dead default); `fetch_error_message` is a needless wrapper (its doc rationale is wrong — `FetchError` is already imported); **no same-handle concurrency guard** — a manual admin refresh racing a periodic tick could race `move_dir` on the same dest sha dir (worth a per-handle lock, likely in Task 8).
- **CHANGES.md** entry for the whole distribution feature is deferred to **Task 10** (git_fetch/cache/manager aren't user-visible until endpoints land).

## Two things to eyeball during Tasks 5/10

- **`PACKAGES_CACHE_DIR`** (Task 5 env, Task 10 add-on): must point at the add-on's **persistent `/data`** (e.g. `/data/packages`), else the cache is a temp dir re-fetched after every restart. Task 5 fallback is `std::env::temp_dir().join("byonk-packages")`.
- **`reload_config`** must call `state.package_manager.rebuild_loader()` after storing new config (Task 5) so a `packages:` edit takes effect without restart.

---

## Screen Packages — the big picture (spec + plans)

- **Spec:** `docs/superpowers/specs/2026-07-02-screen-packages-design.md` (format, sharing/resolution, registry/addressing, compat, **distribution §8**, **admin API §9a**, HA config placement §9a.4, migration §7).
- **A package = a git repo** (atomic versioned unit); **a screen = any dir with `meta.yaml`** (+ `script.lua` + `screen.svg`), addressed **`handle/path`**. Repo root has mandatory `byonk-screens.yaml`. **`byonk-builtin`** is embedded (rust-embed), always registered, never fetched. Registry = `packages:` in config (`handle → {repo, pin, token?}`).
- **Plan 1 (format & loader): DONE + deployed** to the HA VM (13 tasks, whole-branch opus review clean). byonk is `0.16.0-dev`; the 11 built-ins were migrated into the embedded `byonk-builtin` package. Clean break — no legacy reader, no bare names, no `@params`. This is committed history now; details in git + `.superpowers/sdd/progress.md` (Plan 1 section).
- **Plan 2 (distribution): IN PROGRESS** (this handover) — `docs/superpowers/plans/2026-07-03-screen-packages-p2-distribution.md`.
- **Plan 3 (HA Options Flow): NOT WRITTEN** (spec §9a.4) — move package config (repo/pin/token registry, default screen, auth mode) into a config-entry Options Flow; add hub-device status entities (per-package fetch-status sensors + "Update packages" button). Depends on Plan 2's write API.

## The admin API the HA integration consumes (post-Plan-1; Plan 2 extends)

Token-gated `/api/admin/*` (bearer; 404 when no token configured, 401 when wrong):

| Method + path | Purpose |
|---|---|
| `GET /api/admin/devices` | configured + seen devices, telemetry + resolved screen/dither/panel |
| `GET /api/admin/pending` | connected-but-unregistered devices (+ code) |
| `GET /api/admin/config` | effective config JSON (**`admin.token` + `packages.*.token` stripped**) |
| `GET /api/admin/screens` | **package-grouped**: `{ packages:[{handle,name,description,author,license, screens:[{ref,title,description,params,byonk,compat_warning}]}], panels, dither_algorithms }` |
| `GET /api/admin/packages` | registered packages; **Plan 2 Task 9 enriches** with `pin_kind`/`resolved_sha`/`status`/`last_fetched`/`error` (+ existing `handle,repo,pin,builtin,token_set,screen_count`) |
| `POST/PATCH/DELETE /api/admin/devices[/:key]` | device→screen mapping (`screen` is a `handle/path` ref) |
| `PATCH /api/admin/settings` | registration, auth_mode, default_screen, registration_screen; **Plan 2 Task 7 adds** `package_refresh_interval` |
| **Plan 2 Task 8:** `POST/PATCH/DELETE /packages[/:handle]`, `POST /packages[/:handle]/update`, `POST /packages/update` | package management (register/patch/delete/refresh) |

Comment-preserving `config.yaml` writes via `config_writer` (device-keyed helpers; **Plan 2 Task 6 adds `upsert_package`/`remove_package`**). Hot-reload via `reload_config` (arc-swap).

## Build / verify

- `make check` — Rust fmt + clippy (`-D warnings`) + tests. **Green at HEAD `d9380c5`.**
- `make ha-setup` (one-time: `.venv` via uv, **Py ≤ 3.13**), then `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`, 65 tests). `tests_ha/` is not in the ruff target — run `.venv/bin/ruff check tests_ha` separately.
- `make docs` — mdBook build. (Task 10 touches `docs/src/api/admin-api.md`.)
- Env vars: `CONFIG_FILE`, `SCREENS_DIR`, `FONTS_DIR`, **`PACKAGES_DIR`** (Plan 1: on-disk non-builtin dev packages, `handle → <dir>/<handle>`), **`PACKAGES_CACHE_DIR`** (Plan 2: git checkout cache — wired in Task 5).

## Config files (important distinction)

- **`config.yaml`** = developer's local test config (demo devices). Used by `make run`/`watch`.
- **`default-config.yaml`** = shipped/embedded default (device-free; `screens: {}`; `default_screen: byonk-builtin/default`). Embedded by `src/assets.rs` + copied into Docker images.

## Deploying screen-packages to the HA VM (Rust change → add-on rebuild)

A screen-packages change is a **Rust** change → needs an **add-on rebuild**, not just a screen-file sync.

- **byonk server:** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (rsyncs source + `SCREENS_DIR` + `ha addons rebuild`). Gotchas fixed in the live VM: `rebuild.sh` syncs **`byonk-base/`** (embedded at compile time); the add-on **Dockerfile** needs `COPY byonk-base ./byonk-base` in the builder stage. **Durability gap: the add-on Dockerfile is NOT tracked in the repo** — a VM rebuild-from-scratch regresses it (follow-up). `ha addons …` is deprecated → `ha apps …`. A rebuild can leave the add-on stopped — `ha addons start local_byonk` after. Transient Docker Hub 500s — retry. **Plan 2 also needs `PACKAGES_CACHE_DIR=/data/packages` set in the add-on run config (Task 10).**
- **Integration:** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`, then restart HA core (`make ha-ssh CMD="ha core restart"`).
- **Verify without the token:** `curl :3000/api/admin/packages` → **401** proves the new binary is live; byonk `/health` → 200. **Never read/print the admin token** — verify via the HA UI / entities.

## Current VM state

HAOS VM (qemu detached); HA `:8123` (German UI, owner `byonk`/`byonk`). `local_byonk` add-on on `:3000` = **byonk `0.16.0-dev`, Plan-1 package format**, `state: started`. Config cleaned to `byonk-builtin/…` refs, 4 devices remapped (backup `config.yaml.bak-prepkg`). HA has the updated integration loaded. **This VM does NOT yet have Plan 2** — it runs the Plan-1 build; re-`make ha-rebuild` after Plan 2 lands to test distribution live.

## Test harness (`tools/ha-vm/`)

Scripted headless HAOS in QEMU on macOS (Apple Silicon). See `tools/ha-vm/README.md`.
- **Boot:** `make ha-vm` (hostfwd 8123/3000/4445/2222). Stop `make ha-vm-stop`; reset `make ha-vm-clean`. Detached: `nohup make ha-vm > tools/ha-vm/work/boot.log 2>&1 & disown`.
- **SSH** (key at `tools/ha-vm/ssh/` — gitignored, never commit): `make ha-ssh` / `make ha-ssh CMD="…"`. Handy: `ha addons info/logs local_byonk`, `ha supervisor logs`, `ha core restart`.
- **Samba** (creds `byonk`/`byonk`, port 4445): `addons` (build context + Dockerfile), `addon_configs` (runtime config `addon_configs/local_byonk/config.yaml` + `screens/`), `config` (HA `/config` → `custom_components/byonk`).
- **Driving HA from Chrome** (`mcp__claude-in-chrome` + page `hass`): `config_entries/get`, `supervisor/api`; token in add-on options usable in-page as `window.__tok`; byonk's admin API is CORS-blocked from the HA page — check host-side via `curl :3000/api/admin/…`.

## Reference docs

- Spec: `docs/superpowers/specs/2026-07-02-screen-packages-design.md`
- Plans: `…/plans/2026-07-02-screen-packages-p1-format-loader.md` (done), `…/plans/2026-07-03-screen-packages-p2-distribution.md` (in progress).
- SDD ledger (git-ignored): `.superpowers/sdd/progress.md` — Plan 1 + Plan 2 per-task reviews, commit ranges, follow-ups.
- HA phase specs/plans: `…/2026-06-{28,29,30}-byonk-homeassistant-phase{1..6}-*.md`; user docs `docs/src/…`; harness `tools/ha-vm/README.md`.

---

## Still open — HA device-page findings (Phase 6 candidate, lower priority)

Independent of screen-packages, still unaddressed on this branch:
1. **BUG — per-device Panel/Dither write but don't read back.** Selects write to byonk (on disk) but HA shows "unknown"; Screen/Model/telemetry DO read back. Needs a Rust integration test (POST device → PATCH `{"panel":"trmnl_x"}` → GET assert). Files: `src/api/admin/{read,write}.rs`, `src/models/config.rs`, `src/server.rs`; HA `coordinator.py`/`select.py`.
2. **Model shows "og" for a reTerminal E1002** — detection from Board/Model header (`src/models/device.rs`).
3. **RSSI sensor hidden** (`entity_registry_enabled_default=False` in `sensor.py`) — enable it.
4. **Per-device refresh override** — wants design (precedence: per-device > Lua-returned > screen `meta.refresh`); overlaps Plan-1 follow-up #4. Brainstorm → spec first.

## Deferred / fast-follow (non-blocking)

- **Plan 2 Minors** above (final-review triage).
- **Plan 1 follow-ups:** #4 refresh precedence (new author's `meta.refresh` ignored unless Lua returns 0); #5 test-only `AssetScreensSource::read` lacks `is_safe_rel` guard. (#6 loader-rebuild-on-reload is being fixed BY Plan 2 Task 5.)
- **Add-on Dockerfile not tracked in-repo** (durability) — `byonk-base` COPY lives only in the VM + gitignored staging.
- **HA earlier-phase minors:** `require_admin` → middleware; `config_writer::set_scalar` swallows Replace error before Add; strike-dict stale-count micro-leak; `AddonOptions` redacting `Debug`.
- **Phase 4 (not started):** add-on `version:` automation, HACS list + `home-assistant/brands` prep; a real byonk **release** so a published image exists (only then can the published-image add-on path be validated vs the from-source local build).
