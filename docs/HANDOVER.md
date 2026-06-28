# Handover — Byonk ↔ Home Assistant

_Last updated: 2026-06-28 (Phase 2 complete, PR #20)_

## Goal (the whole effort)

Make byonk runnable and **fully manageable from Home Assistant**, in two user-facing deliverables:
- **HA Add-on** — runs byonk as a Supervisor container (reuses the prebuilt `ghcr.io/oetiker/byonk` multi-arch image) with persistent config.
- **HA Integration** (`custom_components/byonk/`) — manages byonk via **HA-idiomatic UI** (select/switch/number/text entities + native config forms): device telemetry, full read-write of device→screen mappings and global settings, and device onboarding.

Both live as folders in this repo and talk to a byonk **admin API**. Byonk stays the source of truth and persists everything to `config.yaml`.

### Phase plan (each phase = its own spec → plan → implementation)
1. **Phase 1 — Byonk admin/management API. ✅ DONE** (see below).
2. **Phase 2 — HA Add-on** packaging. ✅ DONE (PR **#20**). Direct prebuilt-image add-on (`repository.yaml` at repo root + `homeassistant/byonk/`), static `environment:` for paths/bind, editable persistent `/config` via `map: addon_config:rw`, host port 3000 for LAN devices. byonk reads `admin_token`+`log_level` from `/data/options.json` (read-only; no token gen/persist/log). No Ingress (out of scope).
3. **Phase 3 — HA Integration** (`custom_components/byonk/`): config flow, HA Device per TRMNL, sensors, diagnostic entities, select/number/switch/text controls, subentry-based device add/edit, registration onboarding. Consumes the Phase 1 API.
4. **Phase 4 — Release & docs** (multi-arch publishing aligned to add-on versions, mdBook docs, HACS metadata).

## Current status

- **Phase 1 is merged** into `main` (PR #19, commit `33ed51f`).
- **Phase 2 is complete and in review:** PR **#20** → https://github.com/oetiker/byonk/pull/20
  - Branch: `feat/homeassistant-addon` (cut from `main`; base of the PR is `main`).
  - Built task-by-task with TDD (4 tasks); every task individually reviewed + a final whole-branch review (verdict: ready to merge, no Critical/Important). Full suite green; `make docs` clean.
  - SDD ledger: `.superpowers/sdd/progress.md` (git-ignored) has per-task commits + deferred Minors.
- **Phase 3 (HA Integration) is NEXT.** Phase 4 not begun.

## What Phase 1 delivered (the API Phase 3 will consume)

Token-gated `/api/admin/*` (bearer token from `BYONK_ADMIN_TOKEN` env or `admin.token` in config; **404 when no token configured**, 401 when wrong):

| Method + path | Purpose |
|---|---|
| `GET /api/admin/devices` | configured + seen devices, telemetry + resolved active screen |
| `GET /api/admin/pending` | connected-but-unregistered devices (+ registration code) |
| `GET /api/admin/config` | effective config as JSON (admin token stripped) |
| `GET /api/admin/screens` | screens + per-screen param schemas + panels + dither algorithms |
| `POST /api/admin/devices` | add a device→screen mapping |
| `PATCH /api/admin/devices/:key` | update mapping (top-level fields merge; **`params` is a full replacement**) |
| `DELETE /api/admin/devices/:key` | remove a mapping |
| `PATCH /api/admin/settings` | registration on/off, auth_mode, default_screen |

Supporting features: per-screen **`@params`** schema (parsed-not-executed YAML header in each screen `.lua`), **comment-preserving** `config.yaml` writes (yamlpath/yamlpatch), **config hot-reload** (arc-swap; atomic write + reparse + rollback; writes serialized by a mutex).

Key source: `src/api/admin/{mod,read,write}.rs`, `src/models/param_schema.rs`, `src/services/config_writer.rs`, `src/server.rs` (`SharedConfig`, `AppState`, `reload_config`). User docs: `docs/src/api/admin-api.md`.

## Where to pick up next

**Start Phase 3 (HA Integration, `custom_components/byonk/`).** Run `brainstorming` → `writing-plans` → execute (subagent-driven-development). Key contract already established by Phase 2 and **must stay zero-touch / no-redundancy**:
- **Trust is automatic** — the integration generates the admin token, writes it into the byonk add-on's options via the Supervisor API (`AddonManager.async_set_addon_options` + `async_restart_addon`, the `zwave_js` pattern; needs `after_dependencies: ["hassio"]`, guard with `is_hassio`), then reads it back. The **user never sets or copies a token.** The add-on option is the token's single source of truth; the integration must not cache its own copy.
- byonk reads `admin_token`/`log_level` from `/data/options.json` (Phase 2); a blank token leaves the admin API dormant (404) until the integration provisions it.
- The integration manages device→screen mappings and global settings **only** via the Phase 1 admin API — never duplicate settings the add-on/`config.yaml` already own.
- Likely contents: config flow + Supervisor add-on discovery, HA Device per TRMNL, sensors/diagnostics, select/number/switch/text controls, subentry-based device add/edit, registration onboarding.

## Reference docs (read these before continuing)

- Phase 1 spec: `docs/superpowers/specs/2026-06-28-byonk-homeassistant-phase1-admin-api-design.md` (also contains the umbrella vision + phase summaries)
- Phase 1 plan: `docs/superpowers/plans/2026-06-28-byonk-homeassistant-phase1-admin-api.md`
- **Phase 2 spec:** `docs/superpowers/specs/2026-06-28-byonk-homeassistant-phase2-addon-design.md` (§5 zero-touch trust model — the Phase 3 contract)
- **Phase 2 plan:** `docs/superpowers/plans/2026-06-28-byonk-homeassistant-phase2-addon.md`
- SDD progress ledger (git-ignored): `.superpowers/sdd/progress.md` — per-task commit ranges + deferred findings.

## Deferred / fast-follow items (non-blocking, from the final review)

### Phase 1 deferred

1. Convert the per-handler `require_admin` call into a nested-router **middleware layer** so a future admin endpoint can't silently skip auth.
2. Reconcile write-path screen validation (config-only) with `ContentPipeline::resolve_screen`'s filesystem auto-discovery — or document that admin writes require a configured `screens:` entry.
3. Minor hardening: `extract_params_block` should require the `@params` marker inside a `--[[ ]]` comment; surface `persist` rollback-write failures; `config_writer` insert assumes 2-space indent and a non-flow `devices:` map.
4. Test coverage gaps (all behavior is integration-tested, but some unit gaps): per-screen `@params` parse tests (floerli), more no-param screens, more device-write branch unit tests.

### Phase 2 deferred (non-blocking, from the final review)

1. `AddonOptions` derives `Debug` (plan-mandated) → the token *could* leak via `{:?}` if future code debug-prints it (no current path does). Consider a redacting `Debug` impl.
2. Integration test `addon_options_test` assumes `BYONK_ADMIN_TOKEN` is unset (same harness-wide assumption as all `new_admin` tests) — could false-fail in a CI env that sets it.
3. Minor test gaps: no `Malformed`/`Missing`-path integration test (both verified no-op via unit tests); manifest test doesn't assert `init: true`.

## Build / verify

- `make check` — fmt + clippy + tests (all green as of Phase 2).
- `make docs` — mdBook build (clean).
- `make build` / `make release`.
