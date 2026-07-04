# Handover — Byonk

_Last updated: 2026-07-05 — **Plan A (add-on-owned global config) is now LIVE-VM-VERIFIED on the byonk side.** All 409-gate cases, the registration-toggle Critical fix, and the options.json→effective-config round-trip (package registry authoritative from the add-on Options form, restart-to-apply) were confirmed on the HAOS VM. Branch `feat/screen-packages-p2-distribution` @ `9932719`, tree clean — **no repo code changed this session** (all changes were VM-side). **Plan B (reserved DEFAULT device) is still unwritten.** Two follow-ups surfaced: a VM test-tooling gap (below) and a user-side HA-UI eyeball._

## TL;DR — resume here

1. **Plan-A VM verification: DONE on the byonk side** (details in §"Plan A VM verification — RESULTS"). Remaining slice is **user-side**: eyeball the integration **entities** in the HA UI (package status sensors update, registration switch toggles, Update-packages button) — needs HA UI login creds this agent doesn't have. The byonk APIs those entities call are all verified green.
2. **Write + execute Plan B** (reserved DEFAULT device) — `superpowers:writing-plans` then subagent-driven, after the short ground-truth read (§"Plan B"). **This is the main remaining work; do it in a FRESH session** (this one spent heavy context debugging the VM supervisor).
3. **Then decide branch finishing** (`superpowers:finishing-a-development-branch`): branch carries Plan 1 + 2 + 3 + A; Plan B is the remaining part of the same redirection. Recommendation: hold the merge until Plan B lands so the whole redirection merges together.
4. **New follow-up (VM tooling):** `make ha-rebuild` does NOT sync the add-on manifest, so Plan-A's options-schema never reached the VM until patched by hand this session. See §"VM tooling gap" + memory `ha-vm-addon-manifest-sync-gap`. Worth fixing before the next VM verification.

## Plan A VM verification — RESULTS (2026-07-05, on the HAOS VM)

Rebuilt the add-on (`make ha-rebuild`), patched the VM manifest schema (see gap below), set options via the Supervisor API, restarted, and probed byonk's `/api/admin/*` from the Mac host (`:3000`). All green:

- **409 gate (add-on mode active, field-aware):** `PATCH /settings` with `auth_mode` / `default_screen` / `package_refresh_interval` / `registration_screen` → **409**. `POST`/`PATCH`/`DELETE /packages` (registry) → **409**.
- **Registration-toggle Critical fix:** `PATCH /settings {"registration_enabled":true}` (only) → **200** (stays live in add-on mode).
- **Not gated:** `PATCH /devices/:key` (per-device) → **200**; `POST /packages/update` (content refresh) → **200**.
- **Options form drives effective config (round-trip, restart-to-apply):** set add-on option `packages:[{handle:"optonly",…}]` → after add-on restart, byonk's **`/packages` registry = [byonk-builtin, optonly]** — i.e. it follows **options.json authoritatively**, NOT the byonk-owned `config.yaml` (which has `disttest`). An **empty** options `packages:[]` clears the registry to just `byonk-builtin`. Confirmed both directions.
- **Design note (not a bug):** `GET /config` (`read.rs::get_config`) serves the **raw `config.yaml` file from disk**, not the options-overlaid in-memory config — so it echoes the file's `package_refresh_interval`/`auth_mode`, not the effective options values. The overlay's real effect is observed via `/packages` (registry) and the running behavior. `apply_to_config` (auth_mode / package_refresh_interval / packages) is unit-tested (`src/addon_options.rs`). ⇒ **If any consumer needs the *effective* global settings in add-on mode, `/config` is the wrong source for the 3 options-managed fields; packages have the accurate `/packages` endpoint.**

VM left **clean**: add-on options reset (`packages:[]`, `package_refresh_interval:0`, `auth_mode:api_key`); registry = `byonk-builtin` only; reverted Plan-A integration deployed (`make ha-deploy`) + HA core restarted (loads without the old Plan-3 `config_subentries.package` errors).

## VM tooling gap (found 2026-07-05 — worth fixing)

`make ha-rebuild` (`tools/ha-vm/rebuild.sh`) syncs only build inputs, **never the add-on manifest `config.yaml`**. So Plan-A's schema additions (`auth_mode`/`package_refresh_interval`/`packages` in `homeassistant/byonk/config.yaml`) never reached the VM add-on — supervisor silently **stripped** the unknown options keys. The VM's local-build manifest is a **separate hand-maintained file** (no `image:`, aarch64, `name: Byonk (dev build)`) with **no tracked source** in the repo.

**Manual fix applied this session (repeatable procedure in memory `ha-vm-addon-manifest-sync-gap`):** edit `/addons/byonk/config.yaml` + **bump `version:`** → `POST http://supervisor/store/reload` (from SSH) → `ha addons update local_byonk`. `ha addons reload`/`rebuild`/`ha supervisor restart` alone do **not** refresh a local add-on's cached options schema. The VM manifest is now at `0.16.1-dev` with the full Plan-A schema.

**Suggested real fix:** track the local-build manifest as a template under `tools/ha-vm/` and have setup/rebuild sync it (with local-build overrides), so schema changes propagate automatically.

## Plan B — to write (design + ground truth already gathered)

**Goal:** replace `AppConfig.default_screen` + `AppConfig.registration.screen` with a single reserved DEFAULT device (`devices["DEFAULT"]`). Resolution becomes `device.screen → DEFAULT.screen → built-in fallback`. Keep `registration.enabled`. Core model change — applies to standalone byonk too.

**Ground truth (verified earlier; Plan A did NOT touch these files except `write.rs`/`addon_options.rs`, so line numbers may have shifted ±):**
- `AppConfig` @ `src/models/config.rs`: `default_screen: Option<String>` (~176; `default_screen()` fn ~222-224), `registration: RegistrationConfig` (~180); `RegistrationConfig { enabled (~284), screen (~291) }`; `devices: HashMap<String, DeviceConfig>` (~168); `DeviceConfig { screen: String (~230), … }` (~227-264).
- **Two resolution paths differ (load-bearing):**
  - **Registered-unassigned** — `content_pipeline::run_script_for_device` @ `src/services/content_pipeline.rs:190-194`: `device.screen → config.default_screen → "byonk-builtin/default"`.
  - **Server unregistered** — `src/api/display.rs:285-342`: `registration.screen → render_registration_screen(code)` (**deliberately NO default_screen** — unregistered must show the code).
  - **CLI unregistered** — `src/main.rs:260-294`: `registration.screen → default_screen → render_registration_screen(code)`.
- Built-in code renderer `content_pipeline::render_registration_screen(code,w,h)` @ ~464 — **today always renders code**. Spec §4a wants: code when `registration_code` present, generic "unassigned" otherwise (add `render_unassigned_screen`).
- `device_context.registration_code` → Lua `device.registration_code` (`src/services/lua_runtime.rs:329-336`).
- **`SettingsWrite` @ `src/api/admin/write.rs` (~272) still has `default_screen` + `registration_screen`** and drives the field-aware gate (`touches_global`, ~57-60). Plan B removes both fields (and their gate entries + the Rust tests + `patch_settings` YAML writes). Keep `registration_enabled`/`auth_mode`/`package_refresh_interval`.
- YAML: `default-config.yaml` `default_screen: byonk-builtin/default` (~9), `devices: {}` (~118); `config.yaml` (dev) `default_screen: default` (~288). Both should ship `devices: { DEFAULT: { screen: byonk-builtin/default } }` instead. **Note:** the byonk-owned VM config at `/addon_configs/local_byonk/config.yaml` also has `default_screen`/`packages`/`package_refresh_interval` — it's a "dirty" test config; not a repo file.

**Still to read before writing Plan B:** exact body of `display.rs:285-342`; the shipped `byonk-builtin/default` screen template (registration-aware?); `read.rs::get_config` (serializes `default_screen`/`registration.screen`? — it reads the raw file and strips `admin.token` + per-package `token`); `select.py::ByonkScreenSelect` (present a DEFAULT-device screen-select).

**Sketch of Plan B tasks:** (1) `RESERVED_DEFAULT_KEY="DEFAULT"` const + `AppConfig::default_device_screen()->Option<&str>` + tests. (2) route `run_script_for_device` fallback through it. (3) route display.rs + CLI unregistered branches through it. (4) `render_unassigned_screen` for the code-vs-generic ultimate fallback. (5) remove `default_screen` + `registration.screen` fields + their SettingsWrite/gate/patch_settings handling; migrate both YAMLs to ship the DEFAULT device; fix `get_config` if needed. (6) make `byonk-builtin/default` registration-aware. (7) integration: present DEFAULT device with a live screen-select. (8) docs.

## Branch / merge state

- **Branch `feat/screen-packages-p2-distribution` @ `9932719`** (tree clean). Carries Plan 1 + 2 + 3 + A (code) + both specs + Plan-A plan doc. **This session made NO repo commits** — only VM-side changes.
- **Not merged.** No push yet.

## Build / verify

- **byonk (Rust):** `make check` (fmt + clippy `-D warnings` + tests), `make docs`. **Any change to `homeassistant/byonk/config.yaml` must run `make check`** (Plan-A process lesson: `tests/addon_manifest_test.rs` asserts the exact schema key set).
- **HA integration (Python):** `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` (**69 passing** after Plan A). Deploy: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"`.
- **VM add-on rebuild (Rust change):** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild`. **Manifest/schema change also needs the manual dance** in §"VM tooling gap".
- **Admin-API check without printing the token** (memory `ha-vm-admin-api-testing`): fetch `admin_token` into a shell var via `bash tools/ha-vm/ssh.sh 'ha addons info local_byonk --raw-json' | jq -r .data.options.admin_token`, then `curl localhost:3000/api/admin/*` from the **Mac host**. The Supervisor API is reachable from an SSH session as `http://supervisor` with `$SUPERVISOR_TOKEN` (present there).

## Reference

- **Spec:** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md` (all §11 decisions resolved; source of Plan A + Plan B).
- **Plan A (done + VM-verified):** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`. Range `8b7c7fe..dbf4613`.
- **Plan B:** not yet written — §"Plan B" above.
- **SDD ledger:** `.superpowers/sdd/progress.md` (Plan 1+2+3 + full Plan-A record incl. final review + fix wave).
- **Memories:** `ha-addon-owned-global-config`, `ha-vm-addon-manifest-sync-gap` (new), `ha-vm-admin-api-testing`, `ha-addon-phase2`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Admin API (token-gated `/api/admin/*`) — verified behavior

`GET /devices|pending|config|screens|packages` (reads; `/config` = raw file, `/packages` = effective registry) · `POST/PATCH/DELETE /devices[/:key]` (per-device — writable in add-on mode) · `PATCH /settings` (field-aware: global fields 409 in add-on mode, `registration_enabled` stays live) · `POST/PATCH/DELETE /packages/:handle` (registry — 409 in add-on mode) · `POST /packages[/:handle]/update` (content refresh — always allowed).
