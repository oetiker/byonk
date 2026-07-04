# Handover — Byonk

_Last updated: 2026-07-04 — **Screen Packages Plan 3 (HA package management): spec + implementation plan WRITTEN and committed, NOT yet executed.** Plan 2 (distribution) is DONE and was **live-tested end-to-end on the HA VM this session** (success). Branch `feat/screen-packages-p2-distribution` @ **`603dd44`**, working tree clean. Next session: **execute Plan 3** via `superpowers:subagent-driven-development` (execution approach was offered to the user — subagent-driven recommended — answer still pending). Branch NOT pushed/merged._

## TL;DR — resume here

- **Active initiative: Plan 3 — HA package management.** Surfaces byonk's Plan-2 package distribution in Home Assistant. Spec + plan are written, committed, self-reviewed. **Nothing implemented yet.**
- **Resume by:** read `docs/superpowers/plans/2026-07-04-screen-packages-p3-ha-config.md` and execute it with `superpowers:subagent-driven-development` (fresh implementer+reviewer per task). The plan is self-contained — full code in every step, 11 TDD tasks.
- **Branch:** `feat/screen-packages-p2-distribution` @ `603dd44`. This one branch carries **Plan 1 + Plan 2 (code) + Plan 3 (docs only)**. Plan 3 depends on Plan 2's server API, which lives only on this branch, so execute Plan 3 here (or on a branch off this one).
- **Execution-approach question is OPEN:** I asked subagent-driven (recommended) vs inline; the user hasn't answered. Start there.

## Plan 3 — what it builds (spec §9a.4)

Design approved in brainstorming; spec `docs/superpowers/specs/2026-07-04-screen-packages-p3-ha-config-design.md` (`1fb745f`), plan `…/plans/2026-07-04-screen-packages-p3-ha-config.md` (`603dd44`).

**Key decisions (locked):**
- **Packages = native HA config subentries** of the hub entry (Add/Reconfigure flows → `POST/PATCH /packages`). Byonk stays source of truth; the coordinator **reconciles** subentries ↔ `GET /packages`. User wanted a **native-looking UI** → subentries over a menu-flow.
- **Token is write-only, NEVER persisted in HA** (subentry `data` holds only `handle`/`repo`/`pin`).
- **Delete propagation** via a hub-entry update-listener → `DELETE /packages/:handle`. Byonk 409 (referenced by a device) → the subentry **reappears** on next reconcile (documented self-heal, not a bug).
- **Singleton settings → Options Flow** ("Configure" ⚙): `registration_screen`, `auth_mode`, `package_refresh_interval`. The two hub select entities (`ByonkNewDeviceScreenSelect`, `ByonkAuthModeSelect`) are **removed** (full §9a.4; user OK'd migrating + regenerating config — no users yet).
- **Status/actions = hub entities:** one status sensor per non-builtin package (state=status; sha/last_fetched/error as attributes), one "Update packages" button. Registration switch stays.

**11 tasks:** (1) API client package methods + richer 409 msg · (2) coordinator fetches packages · (3) add-package subentry flow · (4) reconfigure flow · (5) subentry↔registry reconcile · (6) delete propagation + 409 self-heal · (7) options flow · (8) remove migrated selects · (9) status sensors · (10) update button · (11) docs + live VM check.

**Two verify-at-execution risks flagged in the plan:**
1. **Programmatic subentry APIs** (`hass.config_entries.async_add_subentry` / `async_update_subentry` / `async_remove_subentry`) — flow-based subentries are proven in-repo (commit range `80ea75e..b9f89df`), but the programmatic ones are newer. Confirm they exist in the target HA at Task 5; if absent, create subentries via the flow path instead.
2. **`ConfigEntry` attribute assignment** — the delete-listener snapshot is stored on the **coordinator** (not the entry) to avoid slots/frozen issues.

**Prior art:** flow-based `ConfigSubentryFlow` for this exact codebase/HA version lived at `b9f89df^:custom_components/byonk/config_flow.py` (class `ByonkDeviceSubentryFlow`) before Phase 5 removed device subentries. Reuse its idioms (`self._get_entry().runtime_data`, `_get_reconfigure_subentry`, "don't refresh in the add flow or reconcile double-creates"). Stale `.pyc` for `test_subentry_flow`/`test_runtime_subentry`/`test_repairs` in `tests_ha/__pycache__/` are leftovers from that era — ignore.

## Plan 2 live-test result (this session — SUCCESS)

- **VM now runs the Plan-2 build.** Booted the VM (`make ha-vm`), rebuilt the add-on from Plan-2 source (`make ha-rebuild`). `local_byonk` @ `:3000`, byonk `0.16.0-dev` **with distribution**, `state: started`, `/health` 200.
- **Test package repo (KEPT as a fixture, user said keep):** `github.com/oetiker/byonk-dist-test` (public) — a valid byonk screen package (`byonk-screens.yaml` + `hello/` = meta.yaml+script.lua+screen.svg). Has a v1 and v2 commit.
- **Verified end-to-end via the admin API (over SSH):** register → `fetching`→`ready`, `resolved_sha` matched the pushed commit, `pin_kind: branch`; screen enumerated with manifest metadata; **rendered** to PNG through a mapped device + `/api/display`; **update** (pushed v2 → `POST …/update` → sha advanced, hot-swap served v2); **delete guard** (409 while a device referenced it, then 200 after removing the device). Config-writer left the integration's 4 real devices untouched.
- **Technique saved to memory** `ha-vm-admin-api-testing.md`: get token via `ha addons info local_byonk --raw-json | jq -r .data.options.admin_token` into a shell var (**never print it**), curl from the **Mac host** `:3000`. The Terminal add-on **cannot** reach byonk internally and has **no docker**. `401` on `/api/admin/packages` does NOT prove Plan-2 is live (auth middleware rejects before routing); only a rebuild guarantees the binary.

## Build / verify

- **Plan 3 is HA/Python.** `make ha-setup` (one-time: `.venv` via uv, **Py ≤ 3.13**), then `make ha-check` (ruff `custom_components/byonk` + `pytest tests_ha`). `tests_ha/` isn't in the ruff target — run `.venv/bin/ruff check tests_ha` separately. Per-file: `.venv/bin/pytest tests_ha/<file> -v`.
- **Deploy Plan 3 (integration-only — NO add-on rebuild needed; server binary is already Plan-2):** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`, then `make ha-ssh CMD="ha core restart"`.
- **Rust side unchanged:** `make check` (fmt + clippy `-D warnings` + tests). `make docs` for mdBook.

## Deploying to the HA VM

- **byonk server (Rust change → add-on rebuild):** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild`. Gotchas: rebuild can leave the add-on stopped (`ha addons start local_byonk` after); `ha addons …` deprecated → `ha apps …`; transient Docker Hub 500s → retry. Add-on Dockerfile is **not tracked in-repo** (durability gap; `byonk-base` COPY lives only in gitignored VM staging).
- **Integration (Python):** `make ha-deploy` + `ha core restart`.
- **Never read/print the admin token** — verify through the HA UI, or use the memory'd server-side technique.

## Current VM state

HAOS VM (qemu; may be stopped between sessions — reboot with `nohup make ha-vm > tools/ha-vm/work/boot.log 2>&1 & disown`). HA `:8123` (German UI, owner `byonk`/`byonk`). `local_byonk` add-on `:3000` = **byonk `0.16.0-dev` WITH Plan-2 distribution**, `state: started`. Integration = the **pre-Plan-3** build (4 real devices registered). Deploy Plan 3 here to test the new UI.

---

## Screen Packages — the big picture (spec + plans)

- **Root spec:** `docs/superpowers/specs/2026-07-02-screen-packages-design.md` (format, resolution, registry/addressing, distribution §8, admin API §9a, **HA config placement §9a.4**, migration §7).
- **A package = a git repo**; **a screen = any dir with `meta.yaml`** (+`script.lua`+`screen.svg`), addressed `handle/path`. Repo root has mandatory `byonk-screens.yaml`. `byonk-builtin` is embedded (rust-embed), always registered, never fetched, cannot be deleted.
- **Plan 1 (format & loader): DONE** (on this branch). **Plan 2 (distribution): DONE** + live-tested. **Plan 3 (HA package management): spec+plan written, NOT executed.**

## The admin API (token-gated `/api/admin/*`; bearer)

| Method + path | Purpose |
|---|---|
| `GET /devices` · `GET /pending` · `GET /config` | device/config reads |
| `GET /screens` | package-grouped screens + panels + dither_algorithms |
| `GET /packages` | packages enriched with `pin_kind`/`resolved_sha`/`status`/`last_fetched`/`error` (+ `handle,repo,pin,builtin,token_set,screen_count`) |
| `POST/PATCH/DELETE /devices[/:key]` | device→screen mapping |
| `PATCH /settings` | `registration_enabled`, `auth_mode`, `default_screen`, `registration_screen`, `package_refresh_interval` |
| `POST /packages` · `PATCH/DELETE /packages/:handle` · `POST /packages/:handle/update` · `POST /packages/update` | package register/patch/delete/refresh (update endpoints fire-and-forget — client polls `GET /packages`) |

## Integrating this branch (decision still pending)

Branch kept as-is, NOT pushed/merged. Merge-base with `main` is `cfddbd4` (pre-Plan-1), so merging brings **Plan 1 + HA Phase 4/5 + Plan 2 (+ now Plan 3 docs)** all at once. Reconcile before merging (`superpowers:finishing-a-development-branch`).

## Reference docs & ledger

- Specs: `…/specs/2026-07-02-screen-packages-design.md`, `…/specs/2026-07-04-screen-packages-p3-ha-config-design.md`.
- Plans: `…/plans/2026-07-0{2-p1,3-p2,4-p3}-…md`.
- SDD ledger (git-ignored): `.superpowers/sdd/progress.md` — Plan 1 + Plan 2 per-task reviews/commit ranges + deferred Minors.
- HA harness: `tools/ha-vm/README.md`. Memory: `ha-vm-admin-api-testing.md`, `ha-*` phase notes.

## Config files (important distinction)

- **`config.yaml`** = developer's local test config (demo devices; `make run`/`watch`).
- **`default-config.yaml`** = shipped/embedded default (device-free; `default_screen: byonk-builtin/default`).

---

## Still open — lower priority (unchanged from Plan 2 handover)

**Plan 2 deferred Minors** (in the SDD ledger): git_fetch symlinks-as-files / gitlink skip / partial-fetch dest not cleaned / SSH-URL auth untested; config_writer insert-path + comment-preservation package tests; `admin_packages_test` fires a real git fetch (flaky — inject a fake fetcher); missing-pin defaults to `"main"`.

**HA device-page findings (Phase 6 candidate):** (1) per-device Panel/Dither write but don't read back; (2) Model shows "og" for reTerminal E1002; (3) RSSI sensor hidden by default; (4) per-device refresh override needs design.

**Other fast-follow:** Plan 1 follow-ups (#4 refresh precedence, #5 test-only `is_safe_rel` guard); add-on Dockerfile not tracked in-repo; HA earlier-phase minors (`require_admin`→middleware, `config_writer::set_scalar` Replace-error, strike-dict micro-leak, `AddonOptions` Debug redaction); Phase 4 (add-on `version:` automation, HACS/brands prep, a real byonk release so a published image exists).
