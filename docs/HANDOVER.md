# Handover — Byonk

_Last updated: 2026-07-04 — **Redirection now split into two plans.** The add-on-owned-global-config spec (all §11 decisions resolved) was decomposed, via `superpowers:writing-plans`, into two sequenced plans. **Plan A (add-on-owned config) is written, self-reviewed, and ready to execute.** **Plan B (reserved DEFAULT device) is NOT yet written** — it needs a short focused ground-truth pass first (see below). Branch `feat/screen-packages-p2-distribution` @ `e5f4be3`, tree clean. Plan-3 merge still HELD._

## TL;DR — resume here

1. **Decide the fork:** either (a) **execute Plan A** now (subagent-driven, per `superpowers:subagent-driven-development`), or (b) **write Plan B first** (fresh session recommended — it needs the ground-truth pass in §"Plan B" below). Plan A is independently shippable and unblocks the Plan-3 merge decision; Plan B is a follow-up.
2. **Plan A:** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md` — 9 tasks, TDD, complete code in every step. Covers: `addon_mode` flag → options.json parsing (settings + packages) → read-only gating of global-config admin writes in add-on mode → add-on manifest schema → HA integration reverts (Plan-3 Tasks 3–7) → docs.
3. **Plan B (to write):** reserved DEFAULT device — replaces `default_screen` + `registration.screen` with a synthetic `devices["DEFAULT"]`. Core model change (applies to standalone too). Design + gathered ground truth below.
4. **Do NOT merge Plan 3** — its config-write UI is reverted by Plan A Tasks 5–8.

## Why the split (writing-plans scope check)

The redirection spec covers **two independent subsystems**: (A) the add-on-mode config redirection, and (B) the `default_screen`/`registration.screen` → DEFAULT-device unification (a core model change touching standalone byonk). Per the scope-check guidance they became two plans, sequenced A→B. Plan A leaves a working intermediate state: byonk's shipped `default_screen: byonk-builtin/default` stays until Plan B lands, and per-device screens are set via the integration (allowed in add-on mode).

## Plan A — ready to execute

- File: `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`.
- **byonk (Rust):** Task 1 `AppState.addon_mode` (field defaults false in `create_app_state_with_overrides` @ `src/server.rs:179-190`; set in `src/main.rs:685` from `matches!(addon, ReadResult::Parsed(_))`). Task 2 extend `AddonOptions` + `apply_to_config` (`src/addon_options.rs`) to parse `auth_mode`/`package_refresh_interval`/`packages[]` (list→`HashMap<handle,PackageRef>`). Task 3 `require_writable_global` guard in `src/api/admin/write.rs` on `patch_settings`/`add_package`/`patch_package`/`delete_package` → 409; NOT on device handlers or `update_package`/`update_all_packages`.
- **Manifest:** Task 4 extend `homeassistant/byonk/config.yaml` `options:`/`schema:`.
- **Integration reverts:** Task 5 subentry flow (config_flow.py 81-88, 285-296, 299-367). Task 6 reconcile (coordinator.py 167-212, 128, 87, imports 12-13) + delete listener (__init__.py 59-80, 45, 42-44, import 12). Task 7 Options Flow (config_flow.py 90-93, 238-282). Task 8 trim test_api.py package-write tests. Delete tests: `test_package_subentry_flow.py`, `test_package_reconcile.py`, `test_package_delete_propagation.py`, `test_options_flow.py`.
- **Keeps (do not touch):** package status sensors (`package_entities.py`), Update button (`button.py`), registration switch (`switch.py`), per-device selects (`select.py`), `api.py::async_update_settings`/`async_get_packages`/`async_update_packages`.
- Task 9 docs (`CHANGES.md` + `docs/src/`).

## Plan B — to write (design + ground truth already gathered)

**Goal:** replace `AppConfig.default_screen` + `AppConfig.registration.screen` with a single reserved DEFAULT device (`devices["DEFAULT"]`). Resolution becomes `device.screen → DEFAULT.screen → built-in fallback`. Keep `registration.enabled`.

**Ground truth (verified this session):**
- `AppConfig` @ `src/models/config.rs`: `default_screen: Option<String>` (line 176; `default_screen()` fn 222-224), `registration: RegistrationConfig` (180); `RegistrationConfig { enabled (284), screen (291) }` (Default 302-306); `devices: HashMap<String, DeviceConfig>` (168); `DeviceConfig { screen: String (230), dither/panel/… }` (227-264). `Default` impl 394-407.
- **Two resolution paths differ (load-bearing):**
  - **Registered-unassigned** — `content_pipeline::run_script_for_device` @ `src/services/content_pipeline.rs:190-194`: `device.screen → config.default_screen → "byonk-builtin/default"`.
  - **Server unregistered** — `src/api/display.rs:285-342`: `registration.screen → render_registration_screen(code)` (**deliberately NO default_screen** — comment says unregistered must show the code).
  - **CLI unregistered** — `src/main.rs:260-294`: `registration.screen → default_screen → render_registration_screen(code)`.
- Built-in code renderer: `content_pipeline::render_registration_screen(code,w,h)` @ `src/services/content_pipeline.rs:464` — **today always renders code**. Spec §4a wants: code when `registration_code` present, generic "unassigned" otherwise (add `render_unassigned_screen(w,h)` + branch).
- `device_context.registration_code` carries the pairing code (built in display.rs 271-283/586-603, main.rs 230-241) and reaches Lua as `device.registration_code` (`src/services/lua_runtime.rs:329-336`). So a registration-aware DEFAULT screen template renders the code when unregistered.
- Settings write DTO `SettingsWrite` @ `src/api/admin/write.rs:243-250` has `default_screen` + `registration_screen` (patch_settings 252-311 writes YAML paths `["default_screen"]`, `["registration","screen"]`) — remove both.
- YAML: `default-config.yaml` has `default_screen: byonk-builtin/default` (line 9), `devices: {}` (118). `config.yaml` (dev) has `default_screen: default` (288). Both must ship `devices: { DEFAULT: { screen: byonk-builtin/default } }` instead.

**Still to read before writing Plan B:** exact body of `display.rs:285-342` unregistered branch; the shipped `byonk-builtin/default` screen template (is it registration-aware?); `read.rs::get_config` (does it serialize `default_screen`/`registration.screen`?); the integration's per-device select wiring to present a DEFAULT-device screen-select (`select.py` `ByonkScreenSelect`).

**Sketch of Plan B tasks:** (1) `RESERVED_DEFAULT_KEY="DEFAULT"` const + `AppConfig::default_device_screen()->Option<&str>` helper + tests. (2) route `run_script_for_device` fallback through it. (3) route display.rs + CLI unregistered branches through it. (4) make `render_registration_screen` path render generic when no `registration_code` (add `render_unassigned_screen`). (5) remove `default_screen` + `registration.screen` fields + SettingsWrite entries + patch_settings handling; migrate both YAMLs to ship the DEFAULT device; fix `get_config` if needed. (6) make `byonk-builtin/default` screen registration-aware. (7) integration: present DEFAULT device with a live screen-select. (8) docs.

## Branch / merge state

- **Branch `feat/screen-packages-p2-distribution` @ `e5f4be3`** (tree clean). Carries Plan 1 + Plan 2 (code) + Plan 3 (code) + both specs + this session's plan doc.
- **Plan 3 merge HELD.** Plan A reverts Plan-3 Tasks 3–7 in place on this branch (keeps 1,2,8,9,10). Do not merge/push yet.

## Build / verify

- **byonk (Rust):** `make check` (fmt + clippy `-D warnings` + tests), `make docs` (mdBook).
- **HA integration (Python):** `make ha-setup` once, then `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` (currently 84 passing; Plan A changes the count as revert-target tests are deleted). Deploy: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"`.
- **Add-on manifest:** `homeassistant/byonk/config.yaml`. Rebuild on VM: `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild`.
- **Admin-API verification on the VM without printing the token** (memory `ha-vm-admin-api-testing`): fetch token into a shell var via `bash tools/ha-vm/ssh.sh 'ha addons info local_byonk --raw-json | jq -r .data.options.admin_token'`, then `curl localhost:3000/api/admin/*` from the **Mac host**.

## Current VM state

HAOS VM running (qemu; `:8123` HA, `:3000` byonk `0.16.0-dev` with Plan-2 distribution, `:2222` ssh, `:4445` samba). It runs the **Plan-3** integration + a registered `disttest` package + `package_refresh_interval=900` in `/addon_configs/local_byonk/config.yaml`. Throwaway test state; fine to leave or reset. Will need redeploy once Plan A lands.

## Reference

- **Spec (source of both plans):** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md` (all §11 decisions resolved).
- **Plan A (ready):** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`.
- **Plan B:** not yet written — see §"Plan B" above.
- **Plan 3 (superseded placement, code merge-ready):** spec `…/specs/2026-07-04-screen-packages-p3-ha-config-design.md`, plan `…/plans/2026-07-04-screen-packages-p3-ha-config.md`. Root spec `…/specs/2026-07-02-screen-packages-design.md`.
- **SDD ledger:** `.superpowers/sdd/progress.md` (Plan 1 + 2 + 3 records).
- **Memories:** `ha-addon-owned-global-config`, `ha-vm-admin-api-testing`, `ha-addon-phase2`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Admin API (token-gated `/api/admin/*`)

`GET /devices|pending|config|screens|packages` (reads) · `POST/PATCH/DELETE /devices[/:key]` (per-device — stay writable in add-on mode) · `PATCH /settings` + `POST/PATCH/DELETE /packages/:handle` (registry — become read-only 409 in add-on mode per Plan A Task 3) · `POST /packages[/:handle]/update` (content refresh — stays allowed).

## Config files (distinction)

- **`config.yaml`** (repo root) = developer's local test config. **`default-config.yaml`** = shipped/embedded default (device-free today; Plan B adds the DEFAULT device). On the VM, byonk's live app config is `/addon_configs/local_byonk/config.yaml` (byonk-owned; integration never touches it — API only). In add-on mode (Plan A), `options.json` supplies settings + packages; `config.yaml` continues to supply `devices:`.
