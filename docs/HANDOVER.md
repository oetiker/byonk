# Handover — Byonk

_Last updated: 2026-07-05 — **Plan A (add-on-owned global config) is fully executed, reviewed, and merge-clean.** Ran subagent-driven: all 9 tasks + final whole-branch review (opus) + a fix wave for one Critical the final review caught. Branch `feat/screen-packages-p2-distribution` @ `dbf4613`, tree clean, all gates green. **Not yet merged** — the branch-finishing decision is deferred to this session. **Plan B (reserved DEFAULT device) is still unwritten** (design + ground truth below)._

## TL;DR — resume here

1. **Two things are pending, in order:**
   - **(a) Live-VM verify Plan A** (spec §8) — redeploy the reverted integration + rebuild the add-on (new manifest schema) on the HAOS VM, then confirm: registration switch stays live in add-on mode, packages/settings come from the add-on Options form, restart-to-apply works, global-config admin writes 409. See §"Live-VM verification" below.
   - **(b) Write + execute Plan B** (reserved DEFAULT device) — `superpowers:writing-plans` then subagent-driven, after a short ground-truth read (§"Plan B").
2. **Then decide branch finishing** (`superpowers:finishing-a-development-branch`): the branch carries **Plan 1 + 2 + 3 + A**; Plan B is the remaining part of the same redirection. Recommendation: hold the merge until Plan B lands (and Plan A is VM-verified) so the whole redirection merges together. Merging locally to `main` or opening a PR are the other options.
3. **The SDD ledger** `.superpowers/sdd/progress.md` has the full per-task record for Plan A (Plan-A section at the bottom) — trust it + `git log` over memory.

## Plan A — DONE (what shipped)

- **Plan:** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`. **Range `8b7c7fe..dbf4613`** (11 commits). Verified: `make check` GREEN, HA `ruff` clean + `pytest` **69 passed**, `make docs` clean, tree clean.
- **byonk (Rust):**
  - `AppState.addon_mode: bool` (field defaults false in `create_app_state_with_overrides`; `src/main.rs` sets it from `matches!(addon, ReadResult::Parsed(_))`).
  - `src/addon_options.rs` parses `auth_mode`, `package_refresh_interval`, `packages[]` (`AddonPackage{handle,repo,pin,token}`) and applies them to `AppConfig` in add-on mode. **Package registry is AUTHORITATIVE from options.json** (empty list clears; matches admin_token/auth_mode). `PackageRef` imported via `crate::models::config::PackageRef`.
  - `src/api/admin/write.rs`: `require_writable_global` gates `add_package`/`patch_package`/`delete_package` → 409 in add-on mode. `patch_settings` uses a **field-aware** `require_writable_settings` → 409 in add-on mode **only if** the body touches a genuinely-global field (`auth_mode`/`package_refresh_interval`/`default_screen`/`registration_screen`); a **`registration_enabled`-only body is allowed live** (this was the Critical fix — the registration switch shares `PATCH /settings`). Per-device writes + `update_package`/`update_all_packages` (content refresh) never gated.
- **Add-on manifest:** `homeassistant/byonk/config.yaml` `options:`/`schema:` expose `auth_mode`, `package_refresh_interval`, `packages` (handle/repo/pin/token). `tests/addon_manifest_test.rs` updated to expect exactly those 5 schema keys.
- **HA integration reverts (Plan-3 write paths removed):** package subentry add/reconfigure flow, subentry reconcile, delete-propagation listener, global settings Options Flow — all gone; four Plan-3 test files deleted; `test_api.py` package-write tests trimmed. **Kept:** package **status sensors** (`package_entities.py`), **Update-packages** button (`button.py`), **registration switch** (`switch.py`), per-device selects/entities, `api.py::async_update_settings`/`async_get_packages`/`async_update_packages`.
- **Docs:** `CHANGES.md` + `docs/src/guide/ha-addon.md` (new "Global configuration" section) + `ha-integration.md` (stale "Managing Screen Packages" → "Monitoring Screen Packages").

**Process lesson (baked into next plans):** Plan A's Task-4 brief verified the manifest with a Python `yaml` assertion only, **not `cargo test`** → `tests/addon_manifest_test.rs::addon_config_matches_design` (asserts the exact schema key set) was silently RED from Task 4 until the final fix wave caught it. **Any change to `homeassistant/byonk/config.yaml` must run `make check`.**

**Deferred Minors (all cosmetic, none block merge):** one accurate historical "subentry" mention remains in `CHANGES.md` `### Changed` (describes what the new model replaced — not false); duplicate trimmed package-handle silently last-wins (HA UI prevents dups). See ledger.

## Plan B — to write (design + ground truth already gathered)

**Goal:** replace `AppConfig.default_screen` + `AppConfig.registration.screen` with a single reserved DEFAULT device (`devices["DEFAULT"]`). Resolution becomes `device.screen → DEFAULT.screen → built-in fallback`. Keep `registration.enabled`. Core model change — applies to standalone byonk too.

**Ground truth (verified; may have shifted ±lines after Plan A — Plan A did NOT touch these files except `write.rs`/`addon_options.rs`):**
- `AppConfig` @ `src/models/config.rs`: `default_screen: Option<String>` (~176; `default_screen()` fn ~222-224), `registration: RegistrationConfig` (~180); `RegistrationConfig { enabled (~284), screen (~291) }`; `devices: HashMap<String, DeviceConfig>` (~168); `DeviceConfig { screen: String (~230), dither/panel/… }` (~227-264).
- **Two resolution paths differ (load-bearing):**
  - **Registered-unassigned** — `content_pipeline::run_script_for_device` @ `src/services/content_pipeline.rs:190-194`: `device.screen → config.default_screen → "byonk-builtin/default"`.
  - **Server unregistered** — `src/api/display.rs:285-342`: `registration.screen → render_registration_screen(code)` (**deliberately NO default_screen** — unregistered must show the code).
  - **CLI unregistered** — `src/main.rs:260-294`: `registration.screen → default_screen → render_registration_screen(code)`.
- Built-in code renderer `content_pipeline::render_registration_screen(code,w,h)` @ ~464 — **today always renders code**. Spec §4a wants: code when `registration_code` present, generic "unassigned" otherwise (add `render_unassigned_screen`).
- `device_context.registration_code` → Lua `device.registration_code` (`src/services/lua_runtime.rs:329-336`). A registration-aware DEFAULT screen renders the code when unregistered.
- **Settings DTO `SettingsWrite` @ `src/api/admin/write.rs` now also drives the field-aware gate** — it still has `default_screen` + `registration_screen`; Plan B removes both (and their entries from the gate's `touches_global` check + the new Rust tests + `patch_settings` YAML writes). Keep `registration_enabled`/`auth_mode`/`package_refresh_interval`.
- YAML: `default-config.yaml` `default_screen: byonk-builtin/default` (~9), `devices: {}` (~118); `config.yaml` (dev) `default_screen: default` (~288). Both must ship `devices: { DEFAULT: { screen: byonk-builtin/default } }` instead.

**Still to read before writing Plan B:** exact body of `display.rs:285-342` unregistered branch; the shipped `byonk-builtin/default` screen template (is it registration-aware?); `src/api/admin/read.rs::get_config` (does it serialize `default_screen`/`registration.screen`? — note it already strips `admin.token` + per-package `token`); `select.py::ByonkScreenSelect` (to present a DEFAULT-device screen-select).

**Sketch of Plan B tasks:** (1) `RESERVED_DEFAULT_KEY="DEFAULT"` const + `AppConfig::default_device_screen()->Option<&str>` + tests. (2) route `run_script_for_device` fallback through it. (3) route display.rs + CLI unregistered branches through it. (4) `render_unassigned_screen` for the code-vs-generic ultimate fallback. (5) remove `default_screen` + `registration.screen` fields + their SettingsWrite/gate/patch_settings handling; migrate both YAMLs to ship the DEFAULT device; fix `get_config` if needed. (6) make `byonk-builtin/default` registration-aware. (7) integration: present DEFAULT device with a live screen-select. (8) docs.

## Branch / merge state

- **Branch `feat/screen-packages-p2-distribution` @ `dbf4613`** (tree clean). Carries Plan 1 + 2 + 3 + A (code) + both specs + Plan-A plan doc.
- **Not merged.** Plan A reverted Plan-3's config-write UI in place (kept Plan-3 Tasks 1,2,8,9,10). No push yet.

## Live-VM verification (Plan A — not yet run)

The VM still runs the **old Plan-3 integration**. To verify Plan A:
1. Rebuild the add-on (picks up the new manifest schema + Rust changes): `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild`.
2. Redeploy the reverted integration: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"`.
3. In the add-on **Configuration** tab, set `auth_mode`/`package_refresh_interval`/a `packages` row → save → add-on restarts → `GET /config` + `GET /packages` reflect it.
4. Confirm the integration: package **status sensors** update; **registration switch** toggles **without 409** (the Critical fix); **Update packages** button works; global-config admin writes (`PATCH /settings` with a global field, package add/patch/delete) return **409**.
- **Admin-API check without printing the token** (memory `ha-vm-admin-api-testing`): `bash tools/ha-vm/ssh.sh 'ha addons info local_byonk --raw-json | jq -r .data.options.admin_token'` into a shell var, then `curl localhost:3000/api/admin/*` from the **Mac host** (`:3000`).

## Build / verify

- **byonk (Rust):** `make check` (fmt + clippy `-D warnings` + tests), `make docs`.
- **HA integration (Python):** `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` (**69 passing** after Plan A). Deploy commands above.
- **Add-on manifest changes:** always `make check` (see the process lesson above).

## Reference

- **Spec:** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md` (all §11 decisions resolved; source of Plan A + Plan B).
- **Plan A (done):** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`.
- **Plan B:** not yet written — §"Plan B" above.
- **SDD ledger:** `.superpowers/sdd/progress.md` (Plan 1+2+3 + full Plan-A record incl. final review + fix wave).
- **Memories:** `ha-addon-owned-global-config`, `ha-vm-admin-api-testing`, `ha-addon-phase2`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Admin API (token-gated `/api/admin/*`)

`GET /devices|pending|config|screens|packages` (reads) · `POST/PATCH/DELETE /devices[/:key]` (per-device — stay writable in add-on mode) · `PATCH /settings` (field-aware: global fields 409 in add-on mode, `registration_enabled` stays live) · `POST/PATCH/DELETE /packages/:handle` (registry — 409 in add-on mode) · `POST /packages[/:handle]/update` (content refresh — always allowed).

## Config files (distinction)

- **`config.yaml`** (repo root) = developer's local test config. **`default-config.yaml`** = shipped/embedded default (device-free today; Plan B adds the DEFAULT device). On the VM, byonk's live app config is `/addon_configs/local_byonk/config.yaml` (byonk-owned; integration never touches it — API only). In add-on mode, `options.json` supplies settings + packages (authoritative); `config.yaml` supplies `devices:` + the operational `registration.enabled`.
