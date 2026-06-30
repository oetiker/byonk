# Handover — Byonk ↔ Home Assistant

_Last updated: 2026-06-30 — **Phase 5 (HA-owned devices via discovery) is implemented, reviewed, and live-validated** on branch `feat/ha-phase4-release-docs` (committed, **NOT pushed** — user will merge later; chose "keep as-is"). Next session: tackle the **device-page findings** surfaced during live validation (a real bug + 3 enhancements) — see "Next work"._

## TL;DR for the next session

- **Phases 1–3 merged to `main`.** Branch `feat/ha-phase4-release-docs` carries **Phase 4** (the `tools/ha-vm/` HAOS test harness + bug fixes) **and Phase 5** (HA-owned devices). Both validated; branch **not pushed** by user choice.
- **Phase 5 rearchitected device surfacing:** each TRMNL is now its **own HA config entry** (not a subentry of the hub), so a pending device appears as a **native "Discovered" card** (Apple-TV style) → Configure → per-device entry. **Home Assistant owns devices**; byonk ships with **no devices** and prunes byonk mappings that have no HA entry. The old subentry "Add device" flow, the Repairs pending-issue, and the pending-count sensor are **gone**.
- **Live-validated** on the HAOS VM against **two real TRMNLs**: empty default config, zero-touch re-add, both TRMNLs as Discovered cards, configure→per-device entry + write-through to byonk, screen round-trip, hub entities (new-device-screen "(built-in)", no pending sensor). The headline goal — TRMNLs onboarding like an Apple TV — **works**.
- **`make check` / `make ha-check` (50 tests) / `make docs` all green.**
- The next work is **device-page polish (call it Phase 6)** — the user wants to do it in a fresh session and merge Phase 5 separately. Start with the **panel/dither read-back bug** (finding #1).

## Next work — device-page findings (Phase 6 candidate)

Surfaced by the user during hands-on live testing of the onboarded device's page. **Phase 5 itself is done; these are follow-ups.** Full detail in the SDD ledger.

1. **BUG (priority) — per-device Panel/Dither write but don't read back.** Setting **Panel** (`reterminal_e1002`) or **Dither** (`atkinson`) via the device "Steuerung" selects **writes to byonk** (verified on disk: `addon_configs/local_byonk/config.yaml` has `panel:`/`dither:`), but HA shows **"unknown"** even after a forced fresh coordinator poll (hub reload). **Screen, Model, and telemetry DO read back.** So byonk's `GET /api/admin/devices` returns `panel:null`/`dither:null` while the on-disk config has them.
   - Inspected and all *looks* correct: write path (`device_block` emits panel/dither, `patch_device` merges `body.panel.or(existing.panel)` — `src/api/admin/write.rs`), read path (`list_devices` §1: `dc.and_then(|c| c.panel.clone())` from the *same* `dc` that yields `screen` — `src/api/admin/read.rs`), and `persist()` calls `reload_config(state)`. **Root cause not found by inspection** — needs a TDD test, not live poking.
   - **Repro test to write first:** Rust integration test — POST a device, `PATCH /api/admin/devices/:key {"panel":"trmnl_x"}`, then `GET /api/admin/devices` and assert the returned row has `panel == "trmnl_x"`. If it fails → byonk bug (suspect `reload_config`/arc-swap not reflecting the field, or a second config instance, or a read-merge subtlety where `dc` is resolved via a path that drops panel/dither). If it passes → the bug is HA-side (coordinator/`select.py`), but screen/model read back fine via the same mechanism, so byonk-side is more likely. Files: `src/server.rs` (`reload_config`), `src/api/admin/{read,write}.rs`, `src/models/config.rs` (`DeviceConfig`, `get_device_config`); HA: `custom_components/byonk/{coordinator,select}.py`. (The selects are **instant-apply, no Save button — that is correct HA UX**; the Screen change proved the write path works end-to-end.)
2. **Model shows "og" for a reTerminal E1002.** The Model diagnostic shows byonk's *detected* model; for device `9C:13:9E:AB:99:D4` it defaults to `og`. byonk isn't mapping this device's Board/Model header to a reterminal model. (Detected **Model** is separate from the **Panel** profile the user set.) Look at `src/models/device.rs` (`DeviceModel`) + model detection from the Model/Board header on `/api/display` & `/api/setup`.
3. **Signal strength hidden + expose more metadata.** The RSSI sensor exists but is `entity_registry_enabled_default=False` (`custom_components/byonk/sensor.py`) — shows as "+1 deaktivierte Entität". Flip to enabled, and consider exposing more of the last request's metadata. Check what byonk's registry `Device` captures vs what's surfaced.
4. **Per-device refresh interval configurable.** Today refresh is the screen-level `default_refresh` **and is Lua-controllable** (a screen's Lua can return its own `refresh_rate`). The user wants a **per-device override**. Needs design (precedence: per-device override > Lua-returned > screen `default_refresh`), a byonk field, and an HA control (Number entity or reconfigure field). Brainstorm → spec before implementing.

**Suggested approach:** fix #1 first (clear defect, TDD); #3 is trivial; #2 and #4 want a short brainstorm. The user is open to a "Phase 6" device-page effort.

## Phase 5 — what shipped (branch `feat/ha-phase4-release-docs`, commits `7fed9f6..d58af44`)

Architecture: **hub config entry** (zero-touch add-on link + shared polling coordinator + global settings) **+ one config entry per TRMNL** (`unique_id = MAC`, `data = {device_key, hub_entry_id}`). Device entries resolve the hub's coordinator from `hass.data[DOMAIN][hub_entry_id]`; raise `ConfigEntryNotReady` until the hub is up.

| Commit | What |
|---|---|
| `7fed9f6` | byonk ships **empty default config**: new tracked `default-config.yaml` (registration/auth_mode/panels/screens, `devices: {}`, no `default_screen`) is the embedded asset (was `config.yaml`). `config.yaml` stays the dev test config. **Caught a latent `config_writer` bug**: inserting a device into `devices: {}` produced invalid YAML (fixed + test). |
| `f2e2121` | `PATCH /api/admin/settings` accepts `registration_screen`; empty string = built-in (handled in `display.rs`). `config_writer::set_scalar` gained a Replace→Add fallback so an absent optional key can be created. |
| `8e546ad` | Test consts (`CONF_DEVICE_KEY`, `CONF_HUB_ENTRY_ID`) + shared `tests_ha` fixtures (`byonk` fixture, `make_hub_entry`/`make_device_entry`). |
| `77897d8` | Per-device config-entry plumbing: hub/device branch in `__init__`; coordinator in `hass.data`; `ConfigEntryNotReady`; `async_remove_entry` deletes the byonk mapping; platforms branch on `CONF_DEVICE_KEY`. |
| `b9f89df` | Discovery config flow: `async_step_integration_discovery` → `configure` → `dev_params` creates the device entry + POSTs to byonk; `reconfigure`; hub single-instance fix; removed the subentry flow. |
| `ea703a6` | Coordinator: inject `integration_discovery` flows for pending devices; **removal grace** (HA entry not registered → remove, 2-strike) + **orphan-prune** (byonk mapping with no HA entry → delete, 2-strike); discovery teardown. Dropped Repairs. (Two deliberate, opus-verified deviations: reconcile skipped on first successful refresh via `self.data is None`; `eager_start=False` on the discovery `async_create_task`.) |
| `0aeffc5` | Hub **new-device-screen select** (options `[(built-in), *screens]`, `""`=built-in); removed pending sensor; deleted `repairs.py`. |
| `3771012` | strings/translations for the discovery flow; dropped subentry/pending strings. |
| `34433bc` | docs (`ha-integration.md` rewritten for discovery) + CHANGES. |
| `905e3d6` | Final whole-branch-review fix wave: guard `async_step_reconfigure` on the **hub** entry (was `KeyError`); discovery sync skips **ignored** device entries; hub-None guards; cleanups. |
| `d58af44` | **Validation-surfaced repo fix:** `Dockerfile` + `Dockerfile.release` now `COPY default-config.yaml ./config.yaml` (published image is **device-free**, not the dev `config.yaml`). `make run`/`run-release`/`watch` set `CONFIG_FILE=config.yaml SCREENS_DIR=screens FONTS_DIR=fonts` so **local testing still uses the dev devices** (R1 had silently switched `make run`'s embedded config to clean). |

Reviews: every task got a two-stage (spec + quality) review; the opus whole-branch review verified the cross-cutting design (onboarding↔orphan-prune race closed by POST-before-create atomicity; removal-grace & orphan-prune act on disjoint sets; first-refresh ordering; comment-preserving YAML on empty+populated configs; HA-owned invariant self-heals). Deferred Minors (non-blocking) are in the ledger.

## Live validation — what was proven (HAOS VM + 2 real TRMNLs, driven via Claude-in-Chrome + WS API)

- **Empty default ships:** deleting byonk's persisted config + restart re-seeded `devices: {}` (167-line clean default, no `default_screen`) — confirms `7fed9f6` end-to-end and that the local add-on rebuilt from Phase-5 source.
- **Zero-touch re-add** → `create_entry "Byonk"` (auto-provision still works under Phase-5 code).
- **Both TRMNLs as Discovered cards** in HA "Entdeckt", alongside Apple TV/Sonos/Thread: `9C:13:9E:AB:99:D4` (code `XXSFEVAGTB`) and `DC:B4:D9:0E:BC:20` (code `QHUTUREZWE`).
- **Configure flow:** "Set up TRMNL device" dialog (code-labeled) → screen picker → HA native name/area step → per-device entry; device "Verbunden über Byonk" (`via_device`→hub), real telemetry (battery, fw, last_seen, model).
- **Write-through:** byonk config gained `9C:13:9E:AB:99:D4: {screen: calibrator}` inserted into the empty `devices: {}` (validates the `7fed9f6` flow-mapping fix live), comments preserved; Screen select reads back `calibrator`.
- **Hub entities:** `select.byonk_new_device_screen` = "(built-in)", auth-mode select, registration switch; **no** pending sensor, **no** Repairs.
- Onboarding one TRMNL left the other as a card (no interference).

**Current VM state:** HAOS VM running (qemu detached); HA `:8123` (German UI, owner `byonk`/`byonk`); `local_byonk` add-on on `:3000` running the Phase-5 build with a clean default + the one onboarded device (`9C:13` → calibrator, panel `reterminal_e1002`, dither `atkinson` on disk). HA has the Phase-5 integration loaded: hub "Byonk" + device entry `TRMNL 9C:13:9E:AB:99:D4`; `DC:B4:D9:0E:BC:20` still a Discovered card.

## Test harness — resume validation (`tools/ha-vm/`, **Phase-5 updated**)

Scripted headless HAOS in QEMU on macOS (Apple Silicon). See `tools/ha-vm/README.md`.

- **Boot:** `make ha-vm` (hostfwd 8123/3000/4445; `BYONK_PORT=13000 make ha-vm` if your dev byonk holds :3000). Stop `make ha-vm-stop`; reset `make ha-vm-clean`. Boot detached so it survives the agent: `nohup make ha-vm > tools/ha-vm/work/boot.log 2>&1 & disown`. Don't hard-`pkill` mid-op.
- **Deploy the integration:** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`, then restart HA (loads new Python). HA UI defaulted to **German**.
- **Samba shares:** `config` = HA `/config` (integration deploys to `custom_components/byonk`); `addons` = `/addons` (local add-on build context); `addon_configs` = per-add-on `/config` — **byonk's runtime config is `addon_configs/local_byonk/config.yaml`**.
- **Local `local_byonk` add-on (from-source build; the validation workaround — no published image has Phase 1–5):** staging context is `tools/ha-vm/work/addon-staging/byonk/` (git-ignored). **Phase-5 Dockerfile fix already applied there:** the embed file is now `default-config.yaml` — builder does `COPY default-config.yaml ./default-config.yaml` (NOT `./config.yaml`), because Phase-5 `src/assets.rs` embeds `default-config.yaml`. Builder also needs `fonts/ screens/ static/` + `apk add build-base musl-dev perl curl` (curl for `utoipa-swagger-ui`).
  - **Re-stage after byonk changes:** `rsync -a --delete src crates fonts screens static tools/ha-vm/work/addon-staging/byonk/`, `cp Cargo.toml Cargo.lock default-config.yaml tools/ha-vm/work/addon-staging/byonk/`; strip `.DS_Store`/`._*`; push to `/addons/byonk` via the `addons` share.
  - **Rebuild** via WS: `hass.callWS({type:'supervisor/api', endpoint:'/store/reload', method:'post'})` then `.../addons/local_byonk/rebuild`. ~75 s with Docker layer cache, ~5 min cold. (Or HA UI: ⋮ → Rebuild.)
  - **Reset byonk to a clean config:** delete `addon_configs/local_byonk/config.yaml` via Samba, then restart the add-on (WS `/addons/local_byonk/restart`) → re-seeds `devices: {}` from the embedded default.
- **Driving HA from Chrome** (`mcp__claude-in-chrome` + the page `hass` object): `config_entries/get`, `config_entries/flow/progress`, `supervisor/api` (`/addons`, `/addons/local_byonk/{info,rebuild,restart}`); REST via `hass.callApi`: `POST config/config_entries/flow` (start a flow), `POST config/config_entries/entry/{id}/reload`, `DELETE config/config_entries/entry/{id}` (the WS `config_entries/delete` is **not** available — use REST). **Token:** in the add-on options; redacted from the agent's view but usable in-page as `window.__tok`. **byonk's admin API is CORS-blocked from the HA page** — you cannot `fetch` byonk directly from the browser; check byonk state host-side via `curl :3000/api/admin/config` (`401`=up+tokened, `404`=dormant, `000`=down) or via the integration's entities.

## Phase 1 admin API the integration consumes

Token-gated `/api/admin/*` (bearer; **404 when no token configured**, 401 when wrong):

| Method + path | Purpose |
|---|---|
| `GET /api/admin/devices` | configured + seen devices, telemetry + resolved screen/dither/panel |
| `GET /api/admin/pending` | connected-but-unregistered devices (+ registration code) |
| `GET /api/admin/config` | effective config as JSON (admin token stripped) |
| `GET /api/admin/screens` | screens + per-screen `@params` schemas + panels + dither algorithms |
| `POST /api/admin/devices` | add a device→screen mapping |
| `PATCH /api/admin/devices/:key` | update mapping (top-level merge; `params` is a full replacement) |
| `DELETE /api/admin/devices/:key` | remove a mapping |
| `PATCH /api/admin/settings` | registration on/off, auth_mode, default_screen, **registration_screen** (empty = built-in) |

Comment-preserving `config.yaml` writes (`config_writer`); config hot-reload via `reload_config` (arc-swap). Per-screen interactive forms come from the Lua `@params` schema (`src/models/param_schema.rs` → screens endpoint → `custom_components/byonk/param_form.py`); 4 screens declare `@params` (transit, floerli, gphoto, fontdemo-bitmap), calibrator does not. Key source: `src/api/admin/{mod,read,write}.rs`, `src/api/display.rs`, `src/models/{device,config,param_schema}.rs`, `src/addon_options.rs`, `src/services/{device_registry,config_writer}.rs`.

## Config files (important distinction)

- **`config.yaml`** = the **developer's local test config** (has demo devices on purpose). Used by `make run`/`watch` (now via `CONFIG_FILE=config.yaml`). Not embedded, not shipped.
- **`default-config.yaml`** = the **shipped/embedded default** (device-free; `devices: {}`). Embedded by `src/assets.rs` and copied into the Docker images. A fresh install has zero devices (HA owns them).

## Build / verify

- `make check` — Rust fmt + clippy + tests.
- `make ha-setup` (one-time: `.venv` via uv, **Py ≤ 3.13** — HA Core has no 3.14), then `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`). Note: `make ha-check` lints only `custom_components/byonk`, **not** `tests_ha/` — run `.venv/bin/ruff check tests_ha` separately.
- `make docs` — mdBook build.
- `make ha-vm` / `ha-vm-stop` / `ha-vm-clean` / `ha-deploy` — the test VM.

## Reference docs

- Phase 1–4 specs/plans: `docs/superpowers/specs|plans/2026-06-2{8,9}-byonk-homeassistant-phase{1,2,3,4}-*.md`
- **Phase 5 spec/plan:** `docs/superpowers/specs/2026-06-30-byonk-homeassistant-phase5-ha-owned-devices-design.md`, `…/plans/2026-06-30-byonk-homeassistant-phase5-ha-owned-devices.md`
- User docs: `docs/src/guide/ha-addon.md`, `docs/src/guide/ha-integration.md`; harness: `tools/ha-vm/README.md`
- **SDD ledger (git-ignored): `.superpowers/sdd/progress.md`** — full per-task review log, commit ranges, both Phase-5 deviations with traces, the live-validation log, and the device-page findings (1–4) with detail.

## Deferred / fast-follow (non-blocking)

- **Phase 5 minors:** `config_writer::set_scalar` swallows the Replace error before trying Add (unreachable today — all call sites pass static valid paths); strike-dict can retain a stale count if a key vanishes from both sides mid-strike (memory micro-leak, never a wrong action); a redundant test patch; `tests_ha/` not wired into the ruff target; an `ha-integration.md` sentence could clarify that "(built-in)" renders the registration code.
- **Earlier phases:** `require_admin` → middleware; reconcile write-path screen validation vs filesystem auto-discovery; `AddonOptions` redacting `Debug`.
- **Still open from Phase 4 plan (not started):** add-on `version:` automation, HACS default-list + `home-assistant/brands` prep (Phase 4b/4c/4d); a byonk **release** containing Phase 1–5 so a real published image exists (only then can the published-image add-on path be validated, vs the from-source local build).
- **Deferred Fix C:** HA-worded registration screen asset → point `registration.screen` at it (now trivial via the new-device-screen select).
