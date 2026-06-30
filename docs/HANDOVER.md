# Handover — Byonk ↔ Home Assistant

_Last updated: 2026-06-30 — Phase 4 in progress on branch `feat/ha-phase4-release-docs` (NOT pushed). Live HAOS-VM validation surfaced + fixed 6 bugs; the HA integration is close but **not yet push-ready**._

## TL;DR for the next (debugging) session

- Phases 1–3 are merged to `main`. **Phase 4 work lives on branch `feat/ha-phase4-release-docs`, which is committed but NOT pushed** (deliberate — we push/merge only once the HA integration is validated good).
- We built a **local HAOS VM test harness** (`tools/ha-vm/`) and validated the whole add-on + integration stack **live against a real Phase-1–3 byonk and real TRMNL hardware**. That validation found **6 real bugs** (all fixed + committed on the branch).
- **Crucial gotcha:** the published `ghcr.io/oetiker/byonk` image (latest tag `v0.15.0`, **2026-04-28**) **predates the entire HA effort** (Phase 1 admin API + Phase 2 options reader merged 2026-06-28; the Phase 4 byonk fix `026dce7` is even newer). So the **real** add-on (which pulls the published image) **cannot work until byonk is released with these changes**. For validation we build byonk **from source as a local add-on** inside the VM (slug `local_byonk`) — see "Test harness" below.
- **Phase 3's "no Rust changes" no longer holds:** commit `026dce7` changes `src/api/display.rs` (byonk core). See "Bugs fixed".

## What still needs doing before this is push-ready

1. **Validate the remaining flows live** (only unit-tested so far, not exercised on the VM):
   - **Re-auth** (checklist item 7): blank/invalidate the add-on `admin_token` option → integration should raise *Re-authentication required* and re-provision; a transient connection error must NOT loop.
   - **Removal grace** (item 8): a device that disappears from byonk survives one poll (2-strike) before its subentry is removed.
   - **Full real-device onboarding**: we onboarded a *synthetic* pending device end-to-end (write-through to `config.yaml` confirmed). Onboard an actual pending device (e.g. the real `94:A9:90:8C:6D:18`, code `GRFQSRWNSQ`) and confirm its per-device entities/telemetry populate.
2. **Cut a byonk release** containing Phase 1–3 **and** the `026dce7` onboarding fix, so a real published image exists. Only then can the **real add-on** path (published-image, not local build) and **Phase 4b version automation** be validated.
3. **Phase 4b/4c/4d** (original plan, tasks 6–11, NOT started): add-on `version:` automation, HACS default-list + `home-assistant/brands` prep, docs polish. Plan: `docs/superpowers/plans/2026-06-29-byonk-homeassistant-phase4-release-and-docs.md`.
4. **Fix C (deferred):** HA-worded registration screen ("Set me up in Home Assistant — code: …"); ship a screen asset + point `config.registration.screen` at it. (Phase 3 spec §8 optional.)

## Validated live (works) ✅

Against a from-source Phase-1–3 byonk (local add-on) on HAOS 18.0 in QEMU:
- **Zero-touch trust**: integration auto-installs/starts the add-on, provisions `admin_token` into the add-on option, byonk reads it via the `/data/options.json` reader (Phase 2), admin API authenticates; config entry stores **no** token.
- **Phase 2 embedded-asset seeding** (`Seeded screens/fonts/config` in byonk log).
- **Hub device** (4 entities: registration switch, auth-mode select, default-screen select, pending sensor) reflecting `config.yaml`; **per-TRMNL devices keyed by MAC** (8 entities each); config **subentries by MAC**; 116→124 entities.
- **Real TRMNL hardware** reached the VM byonk (via the Mac's LAN IP `:3000`, since QEMU binds `*:3000`) and rendered screens.
- **Pending → onboard → write-through**: unregistered devices surface in `/api/admin/pending` (after the fix), onboarding writes the mapping to byonk `config.yaml` **preserving comments**.

## Bugs fixed (all committed on `feat/ha-phase4-release-docs`)

| Commit | Area | Fix |
|---|---|---|
| `d5087cd` | harness | `make ha-deploy` rsync tripped Samba's `._*` veto on temp files → use `rsync --inplace --whole-file --no-perms/owner/group/times` |
| `bc79531` | integration | config-flow raised a 500 when the admin probe failed → catch `ByonkApiError` → abort `addon_unhealthy` (+test) |
| `215e909` | integration | post-provision probe raced the add-on restart (worked only on retry) → bounded readiness retry `_async_probe_ready` (+test) |
| `026dce7` | **byonk (Rust)** | **A:** unregistered device hitting `/api/display` returned before `registry.upsert` → never in `/api/admin/pending` → couldn't be onboarded. Now upsert it (with `api_key = identity_key`, so the pending code matches the on-screen code). **B:** the unregistered screen fell back to `default_screen`, hiding the code → now shows `registration.screen` or the built-in code screen. (+2 Rust tests) |
| `a235fa1` | integration | onboarding aborted `already_configured` (subentry-flow refresh let `_async_reconcile` create the subentry before the flow's `create_entry`) → flow now owns subentry creation; dropped the pre-create refresh (+regression test) |
| `2d4e7ed` | docs | in-memory pending note (`ha-integration.md`); corrected Phase 3 spec §8 ("no byonk changes" was wrong) |

Verification at handover: `make check` (Rust) green; `make ha-check` green (**46** Python tests); `make docs` clean.

## Known minor items / notes

- byonk's device registry is **in-memory** (`InMemoryRegistry`) → the pending list clears on byonk restart; a device reappears on its next check-in. Documented; not a bug.
- The local `local_byonk` add-on is a **throwaway from-source build** for validation only; its build context lives under `tools/ha-vm/work/addon-staging/byonk/` (git-ignored). It is NOT the shipping add-on.

## Test harness — how to resume validation (`tools/ha-vm/`)

Scripted headless HAOS in QEMU on macOS (Apple Silicon). See `tools/ha-vm/README.md`.

- **Boot:** `make ha-vm` (downloads HAOS `generic-aarch64`, boots headless, hostfwd 8123/3000/4445). If your own byonk dev server holds host :3000, run `BYONK_PORT=13000 make ha-vm`. Stop: `make ha-vm-stop`; full reset: `make ha-vm-clean`.
  - Boot **detached** so it survives the agent session: `nohup make ha-vm > tools/ha-vm/work/boot.log 2>&1 & disown`.
  - **Do not abruptly kill** mid-operation — a hard `pkill` (no graceful guest shutdown) once caused a ~13-min recovery hang. (Follow-up idea: add a QMP socket so `ha-vm-stop` can `system_powerdown`.)
- **HA onboarding (one-time):** browse `http://localhost:8123`, create owner (we used `byonk`/`byonk`), then install the **Samba share** add-on (username/password `byonk`/`byonk`).
- **Deploy the integration:** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`, then restart HA (Developer Tools → Restart). For UI automation, HA defaulted to **German** in our session.
- **Local byonk add-on (the validation workaround):** because no published image has the fixes, we run byonk from source as a local add-on:
  1. Stage build context: `rsync -a src crates fonts screens Cargo.toml Cargo.lock tools/ha-vm/work/addon-staging/byonk/`, `cp config.yaml …/default-config.yaml`, `cp -R static …/static`. Plus an add-on `config.yaml` (slug `byonk`, no `image:`) and a `Dockerfile`. The current staged copy already exists.
  2. **Dockerfile build deps that bit us** (all required): alpine `apk add build-base musl-dev perl curl` (`build-base` for mlua vendored Lua + ring; **`curl`** for `utoipa-swagger-ui`'s build script). The builder stage must **COPY `fonts/`, `screens/`, `config.yaml`, AND `static/`** before `cargo build` — byonk embeds them at compile time (rust-embed `screens/`,`fonts/`,`.`; `include_str!` of `static/dev/*`).
  3. Push to the VM: mount the Samba `addons` share (`//byonk:byonk@localhost:4445/addons`) and `cp -R` the staging dir to `/addons/byonk` (rsync aborts on `.DS_Store` vetoes; `cp -R` tolerates them).
  4. Remove the integration-added GitHub repo + its `*_byonk` add-on so `local_byonk` is the only `*_byonk` the integration's finder matches. Reload the add-on store (⋮ → check for updates), then install/rebuild `local_byonk` (⋮ → "Rebuild" / Neu aufbauen). Build ≈ 4–5 min in-VM.
  5. After byonk code changes: re-push `src/` to `/addons/byonk/src`, then **Rebuild** the add-on.
- **Reach byonk's admin API state without the token** (token-printing is blocked by the harness): `curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/api/admin/config` → `404` dormant, `401` tokened+up, `000` down. Query HA state via the browser WS API: `document.querySelector('home-assistant').hass.callWS({type:'supervisor/api', endpoint:'/addons/local_byonk/info', method:'get'})`.

## Phase 1 API the integration consumes

Token-gated `/api/admin/*` (bearer from `BYONK_ADMIN_TOKEN` env or `admin.token` in config; **404 when no token configured**, 401 when wrong):

| Method + path | Purpose |
|---|---|
| `GET /api/admin/devices` | configured + seen devices, telemetry + resolved active screen |
| `GET /api/admin/pending` | connected-but-unregistered devices (+ registration code) |
| `GET /api/admin/config` | effective config as JSON (admin token stripped) |
| `GET /api/admin/screens` | screens + per-screen param schemas + panels + dither algorithms |
| `POST /api/admin/devices` | add a device→screen mapping |
| `PATCH /api/admin/devices/:key` | update mapping (top-level merge; **`params` is a full replacement**) |
| `DELETE /api/admin/devices/:key` | remove a mapping |
| `PATCH /api/admin/settings` | registration on/off, auth_mode, default_screen |

Comment-preserving `config.yaml` writes (yamlpath/yamlpatch); config hot-reload (arc-swap). Key source: `src/api/admin/{mod,read,write}.rs`, `src/api/display.rs`, `src/models/device.rs`, `src/addon_options.rs`, `src/services/{device_registry,config_writer}.rs`.

## Build / verify

- `make check` — Rust fmt + clippy + tests.
- `make ha-setup` (one-time: `.venv` via uv, Py ≤ 3.13 — **HA Core does not support 3.14**), then `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`).
- `make docs` — mdBook build.
- `make ha-vm` / `ha-vm-stop` / `ha-vm-clean` / `ha-deploy` — the test VM.

## Reference docs

- Phase 1 spec/plan: `docs/superpowers/specs/2026-06-28-byonk-homeassistant-phase1-admin-api-design.md`, `…/plans/2026-06-28-byonk-homeassistant-phase1-admin-api.md`
- Phase 2 spec/plan: `…/specs/2026-06-28-byonk-homeassistant-phase2-addon-design.md`, `…/plans/2026-06-28-byonk-homeassistant-phase2-addon.md`
- Phase 3 spec/plan: `…/specs/2026-06-29-byonk-homeassistant-phase3-integration-design.md` (see corrected **§8**), `…/plans/2026-06-29-byonk-homeassistant-phase3-integration.md`
- **Phase 4 spec/plan:** `…/specs/2026-06-29-byonk-homeassistant-phase4-release-and-docs-design.md`, `…/plans/2026-06-29-byonk-homeassistant-phase4-release-and-docs.md`
- User docs: `docs/src/guide/ha-addon.md`, `docs/src/guide/ha-integration.md`; harness: `tools/ha-vm/README.md`
- **SDD ledger (git-ignored): `.superpowers/sdd/progress.md`** — full per-task + Task-5 validation log, commit ranges, every finding from this session.

## Deferred / fast-follow (non-blocking, from earlier phase reviews)

- **Phase 1:** `require_admin` → middleware layer; reconcile write-path screen validation vs filesystem auto-discovery; minor hardening (`@params` marker inside `--[[ ]]`, surface `persist` rollback failures, `config_writer` 2-space-indent assumption).
- **Phase 2:** `AddonOptions` `Debug` could leak the token via `{:?}` (no current path) — consider a redacting `Debug`; `addon_options_test` assumes `BYONK_ADMIN_TOKEN` unset.
