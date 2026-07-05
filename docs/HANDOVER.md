# Handover — Byonk

_Last updated: 2026-07-05 — **Plan B (reserved DEFAULT device) is now WRITTEN and committed.** The full 10-task implementation plan is at `docs/superpowers/plans/2026-07-05-reserved-default-device.md`, grounded against this HEAD with the real test harnesses verified. Branch `feat/screen-packages-p2-distribution` @ `cff3b8f`, tree clean. **Nothing implemented yet — the next session EXECUTES the plan.** Plan A stays live-VM-verified (byonk side). Do execution in a FRESH session (this one spent its context on the deep ground-truth read + plan authoring)._

## TL;DR — resume here

1. **Execute Plan B** — `docs/superpowers/plans/2026-07-05-reserved-default-device.md`. Use `superpowers:subagent-driven-development` (recommended: fresh subagent per task + two-stage review) or `superpowers:executing-plans`. The plan is self-contained (exact paths, full code, TDD steps, exact commands); it does NOT require re-reading the whole codebase first.
2. **Part A (Tasks 1-8, Rust)** is standalone-shippable on its own: byonk resolves screens via the reserved DEFAULT device in both standalone and add-on mode, settable over `PATCH /devices/DEFAULT`. Run `make check` at the Part-A checkpoint. **Part B (Tasks 9-10, HA integration + docs)** presents the DEFAULT device with a live screen-select; run `.venv/bin/pytest tests_ha -q` (baseline 69 passing).
3. **Then finish the branch** (`superpowers:finishing-a-development-branch`): branch carries Plan 1 + 2 + 3 + A (code) + A/B specs+plans. Plan B is the last part of the same redirection — **hold the merge until Plan B lands**, then the whole redirection merges together.
4. **Before merge (still open):** VM-verify Plan B (set the DEFAULT device screen via admin API / the Byonk Default card; confirm an unregistered device shows the code and a registered-unassigned device shows the DEFAULT screen) + the user-side HA-UI eyeball of Plan-A monitoring entities. Both need the VM (mind the `ha-vm-addon-manifest-sync-gap` — but Plan B touches NO add-on manifest, so `make ha-rebuild` alone suffices for the byonk-side; no schema dance).

## Plan B in one paragraph (what it does)

Removes byonk's two overlapping "what does an unconfigured device show" settings — `AppConfig.default_screen` and `RegistrationConfig.screen` — and replaces them with a single reserved `DEFAULT` device (`devices["DEFAULT"]`). Resolution becomes `device.screen → DEFAULT.screen → built-in fallback`. The shipped `byonk-builtin/default` screen is **already registration-aware** (`screens/default/screen.svg` renders `device.registration_code_hyphenated` under `{% if device.registration_code %}`), so the DEFAULT screen shows the pairing code for un-onboarded devices and normal content otherwise. New built-in ultimate fallback: code screen when a `registration_code` is present, generic "unassigned" screen otherwise. `registration.enabled` is KEPT. The DEFAULT device is written/read over the per-device admin API (live, allowed in add-on mode) — so it needs **no add-on manifest / options.json change**, and the integration presents it as a normal device with a screen-select. Core model change → applies to standalone byonk too.

## Plan B blast radius (verified — larger than the earlier sketch)

Beyond the files the previous handover named, the plan accounts for all of these (already written into the tasks):
- **Rust removal (Task 5) also touches:** `src/api/dev.rs` (`ScreensResponse.default_screen` field + assignment), `src/server.rs` (`test_config_swap_is_visible`), `src/assets.rs` (test asserting `default_screen:` in embedded config), plus `src/api/display.rs` logging line referencing `registration.screen`.
- **Test migration (Task 7):** `tests/admin_write_test.rs` (5 patch tests for default_screen/registration_screen), `tests/admin_packages_test.rs` (dangling-ref test → now via the DEFAULT device), `tests/common/app.rs:244` (broken-config fixture embeds `default_screen: broken`).
- **`GET /devices` already surfaces `config.devices` entries with `registered:true`** (`read.rs::list_devices` step 2) — so a DEFAULT device auto-appears. Task 8 adds a `reserved` flag to `AdminDevice` so the integration can special-case it.
- **Integration reconcile hazard (Task 9):** `coordinator.py::_async_reconcile` **orphan-prunes** byonk devices HA has no entry for — it would `DELETE /devices/DEFAULT` during the startup window. The plan exempts `DEFAULT_DEVICE_KEY` from both reconcile branches and auto-provisions the DEFAULT config entry (dedicated discovery short-circuit in `config_flow.py`, no screen-picker since the device already exists in byonk). `coordinator.registration_screen()` is dead (Plan-3 Options-Flow reverted) → removed.
- **DEFAULT device scoped to `screen` only** (not dither/panel) per spec §4a + YAGNI; `select.py` special-cases the DEFAULT entry to show just `ByonkScreenSelect`.

## Test harnesses (verified to exist — the plan mirrors them)

- **Rust pipeline:** `src/services/content_pipeline.rs` → `#[cfg(test)] mod pipeline_tests`, helper `build_pipeline(disk, loader)` over `AppConfig::default()`. Task 3 adds `build_pipeline_with_config` to inject the DEFAULT device.
- **Rust admin/integration tests:** `tests/` with `tests/common/app.rs` harness (`TestApp`).
- **HA integration:** `tests_ha/` has `conftest.py`, `test_reconcile.py`, `test_device_flow.py`, `test_device_entry.py`, `test_select.py`, `test_settings_entities.py` — Task 9's `test_default_device.py` mirrors these fakes (fake client `async_get_devices` returning a `{"key":"DEFAULT","reserved":True,...}` entry).

## Plan A — status (unchanged, DONE + VM-verified byonk-side)

Add-on-owned global config: options.json → byonk (settings + package registry, restart-to-apply), global-config admin writes 409 in add-on mode, `registration_enabled` stays live, per-device writes + content refresh allowed. All verified on the HAOS VM (byonk side). Remaining Plan-A slice = user-side HA-UI eyeball of monitoring entities. Plan-A plan: `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md` (range `8b7c7fe..dbf4613`).

## Branch / merge state

- **Branch `feat/screen-packages-p2-distribution` @ `cff3b8f`** (tree clean). Carries Plan 1 + 2 + 3 + A (code) + both specs + Plan-A plan + **Plan-B plan (new, this session)**. This session made ONE commit: the Plan-B plan doc (`cff3b8f`). No code changed.
- **Not merged. No push yet** (branch has no upstream — `git pull` needs an explicit remote/branch; it's local-only).

## Build / verify

- **byonk (Rust):** `make check` (fmt + clippy `-D warnings` + tests), `make docs`. Plan B does NOT touch `homeassistant/byonk/config.yaml` (the add-on manifest), so no `addon_manifest_test` schema concern and no VM schema dance.
- **HA integration (Python):** `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` (baseline 69). Deploy: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"`.
- **VM add-on rebuild (Rust change):** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (no manifest change in Plan B → no manual dance needed).
- **Admin-API check without printing the token** (memory `ha-vm-admin-api-testing`): fetch `admin_token` into a shell var via `bash tools/ha-vm/ssh.sh 'ha addons info local_byonk --raw-json' | jq -r .data.options.admin_token`, then `curl localhost:3000/api/admin/*` from the **Mac host** (`:3000`).

## Reference

- **Plan B (WRITTEN, next to execute):** `docs/superpowers/plans/2026-07-05-reserved-default-device.md`.
- **Spec (source of Plan A + Plan B):** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md` (§4a + §5.6 + §6 = Plan B; all §11 decisions resolved).
- **Plan A (done + VM-verified):** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`.
- **SDD ledger:** `.superpowers/sdd/progress.md` (Plan 1+2+3+A record). The Plan-B execution session should append its per-task review status here.
- **Memories:** `ha-addon-owned-global-config`, `ha-vm-addon-manifest-sync-gap`, `ha-vm-admin-api-testing`, `ha-addon-phase2`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Admin API (token-gated `/api/admin/*`) — verified behavior

`GET /devices|pending|config|screens|packages` (reads; `/config` = raw file, `/packages` = effective registry) · `POST/PATCH/DELETE /devices[/:key]` (per-device — writable in add-on mode; **this is how the DEFAULT device is set**) · `PATCH /settings` (field-aware: global fields 409 in add-on mode, `registration_enabled` stays live) · `POST/PATCH/DELETE /packages/:handle` (registry — 409 in add-on mode) · `POST /packages[/:handle]/update` (content refresh — always allowed).
