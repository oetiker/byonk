# Handover — Byonk ↔ Home Assistant

_Last updated: 2026-06-28_

## Goal (the whole effort)

Make byonk runnable and **fully manageable from Home Assistant**, in two user-facing deliverables:
- **HA Add-on** — runs byonk as a Supervisor container (reuses the prebuilt `ghcr.io/oetiker/byonk` multi-arch image) with persistent config.
- **HA Integration** (`custom_components/byonk/`) — manages byonk via **HA-idiomatic UI** (select/switch/number/text entities + native config forms): device telemetry, full read-write of device→screen mappings and global settings, and device onboarding.

Both live as folders in this repo and talk to a byonk **admin API**. Byonk stays the source of truth and persists everything to `config.yaml`.

### Phase plan (each phase = its own spec → plan → implementation)
1. **Phase 1 — Byonk admin/management API. ✅ DONE** (see below).
2. **Phase 2 — HA Add-on** packaging (`homeassistant/` add-on repo structure, prebuilt image, persistent `addon_config`, options, optional Ingress for the `/dev` preview). NEXT.
3. **Phase 3 — HA Integration** (`custom_components/byonk/`): config flow, HA Device per TRMNL, sensors, diagnostic entities, select/number/switch/text controls, subentry-based device add/edit, registration onboarding. Consumes the Phase 1 API.
4. **Phase 4 — Release & docs** (multi-arch publishing aligned to add-on versions, mdBook docs, HACS metadata).

## Current status

- **Phase 1 is complete and in review:** PR **#19** → https://github.com/oetiker/byonk/pull/19
  - Branch: `feat/homeassistant-admin-api` (cut from `chore/update-dependencies`, so it also carries that dep-update commit; base of the PR is `main`).
  - Built task-by-task with TDD; every task individually reviewed + a final whole-branch review (verdict: ready to merge, no Critical/Important). Full suite green; `make docs` clean.
- Nothing else started yet. Phases 2–4 are not begun.

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

**Start Phase 2 (HA Add-on).** Run the `brainstorming` skill to produce a Phase 2 spec, then `writing-plans`, then execute (subagent-driven-development). Likely contents:
- `homeassistant/` add-on repository structure (repository.yaml + one add-on folder with `config.yaml`/`Dockerfile` or `image:` referencing `ghcr.io/oetiker/byonk`).
- Map a persistent config dir (`addon_config`); set env `CONFIG_FILE`/`SCREENS_DIR`/`FONTS_DIR`/`BIND_ADDR` and `BYONK_ADMIN_TOKEN`.
- Add-on options: token (auto-generate?), port, log level, run mode (serve vs dev), optional Ingress for `/dev`.
- TRMNL devices reach byonk directly on the LAN (host port), not via Ingress.

## Reference docs (read these before continuing)

- Phase 1 spec: `docs/superpowers/specs/2026-06-28-byonk-homeassistant-phase1-admin-api-design.md` (also contains the umbrella vision + phase summaries)
- Phase 1 plan: `docs/superpowers/plans/2026-06-28-byonk-homeassistant-phase1-admin-api.md`
- SDD progress ledger (git-ignored): `.superpowers/sdd/progress.md` — per-task commit ranges + deferred findings.

## Deferred / fast-follow items (non-blocking, from the final review)

1. Convert the per-handler `require_admin` call into a nested-router **middleware layer** so a future admin endpoint can't silently skip auth.
2. Reconcile write-path screen validation (config-only) with `ContentPipeline::resolve_screen`'s filesystem auto-discovery — or document that admin writes require a configured `screens:` entry.
3. Minor hardening: `extract_params_block` should require the `@params` marker inside a `--[[ ]]` comment; surface `persist` rollback-write failures; `config_writer` insert assumes 2-space indent and a non-flow `devices:` map.
4. Test coverage gaps (all behavior is integration-tested, but some unit gaps): per-screen `@params` parse tests (floerli), more no-param screens, more device-write branch unit tests.

## Build / verify

- `make check` — fmt + clippy + tests (all green as of Phase 1).
- `make docs` — mdBook build (clean).
- `make build` / `make release`.
