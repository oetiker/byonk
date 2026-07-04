# Handover — Byonk

_Last updated: 2026-07-04 — **DIRECTION CHANGE.** Screen Packages **Plan 3** (HA package management) is fully implemented, reviewed clean, and merge-ready — but live-VM verification surfaced that its **placement is wrong**. The user wants byonk's server-global config in the **add-on** Options screen, not the integration. A **redirection spec is written and committed** (`2f6e82e`), now **under user review**. **Plan 3's merge is HELD.** Branch `feat/screen-packages-p2-distribution` @ `2f6e82e`, tree clean. Next: user approves the redirection spec → `superpowers:writing-plans` → execute (fresh session)._

## TL;DR — resume here

1. **Read the redirection spec:** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md` (@ `1d0e24a`). It supersedes Plan 3's config placement. **All §11 decisions are now RESOLVED** (see below) — the spec is complete and ready for a final read.
2. **Decisions locked this session:** default_screen + registration_screen → **reserved DEFAULT device** (§4a: a synthetic DEFAULT TRMNL device, live per-device screen-select, unifies both); `registration_enabled` → **live integration switch** (not app Options); `POST /packages/update` content-refresh → **allowed** in add-on mode (registry edits stay read-only). App Options static config = `auth_mode` + `package_refresh_interval` + `packages[]`.
3. **Next:** confirm the user is happy with the final spec, then invoke `superpowers:writing-plans` → execute (subagent-driven, as Plan 3 was).
4. **Do NOT merge Plan 3** — its config-write UI is being reverted (see the spec's §9 reuse/revert matrix).

## What changed this session

- **Executed Plan 3 end-to-end** (11 tasks, subagent-driven) + final whole-branch review (opus) + the review's Issue-1 fix. All green (`tests_ha` 84 passed, ruff clean, `make docs` clean). The SDD ledger `.superpowers/sdd/progress.md` has the full per-task record; Plan-3 range = `89a35be..c2022c6` (15 commits).
- **Live-VM verified Plan 3's UI** (browser automation against the HAOS VM). Add-package flow, status sensors, Options Flow (wrote `package_refresh_interval=900`, verified via admin API), removed selects — all worked. Found one real bug (formatjs `{error}` MISSING_VALUE on the add/reconfigure step descriptions — the initial `async_show_form` never passes `description_placeholders={"error": ""}`). **That bug is now MOOT** — those subentry flows are being reverted.
- **The user then clarified the real intent** (a genuine spec-phase misunderstanding): global config belongs in the **add-on Configure screen**, the integration should be **read-only monitoring**. Brainstormed the redirection, confirmed the model, checked HA dev docs on app-options apply behavior, and **wrote + committed the redirection spec**.

## The redirection in one paragraph

Server-global config (settings + the **package registry**) moves to the byonk **add-on Options form** (HAOS schema → `/data/options.json` → byonk reads it by extending `src/addon_options.rs`; **restart-to-apply**, accepted). In **add-on mode** global-config admin writes go read-only (app Options is the only editor); **per-device** writes stay. **Standalone byonk unchanged** (`config.yaml` + full admin API). The **integration** becomes read-only monitoring (keeps Plan-3 Task 9 status sensors) + keeps per-device mapping + two live *operational* controls (registration toggle, Update-packages button). Reverts Plan-3 Tasks 3–7. Memory: `ha-addon-owned-global-config.md`.

## Branch / merge state

- **Branch `feat/screen-packages-p2-distribution` @ `2f6e82e`** (tree clean). Carries **Plan 1 + Plan 2 (code) + Plan 3 (code) + the redirection spec (docs)**.
- **Plan 3 merge is HELD** pending the redirection. Do not merge/push yet.
- Plan-3 reuse/revert matrix is in the spec §9 (reuse: Tasks 1,2,8,9,10; revert: Tasks 3–7). Whether to revert on this branch or branch fresh is a **writing-plans** decision.

## Current VM state (IMPORTANT — it now runs Plan 3)

HAOS VM is **running** (qemu; `:8123` HA, `:3000` byonk `0.16.0-dev` WITH Plan-2 distribution, `:2222` ssh, `:4445` samba). During this session I **deployed the Plan-3 integration** to it and, via the UI, **registered a `disttest` package** (`https://github.com/oetiker/byonk-dist-test.git`, pin main → `status=ready`, sha `97578c5f…`) and **set `package_refresh_interval=900`**. So byonk's config (`/addon_configs/local_byonk/config.yaml`) now has that package + interval. This is throwaway test state — fine to leave or reset. The integration on the VM is the **Plan-3** build (will need redeploy once the redirection lands).

## Build / verify

- **byonk (Rust):** `make check` (fmt + clippy `-D warnings` + tests), `make docs` (mdBook). The options reader is `src/addon_options.rs` (currently `admin_token` + `log_level`; the redirection extends it).
- **HA integration (Python):** `make ha-setup` once, then `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` (currently 84 passing). Deploy to VM: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"`.
- **Add-on manifest** (the redirection's schema change): `homeassistant/byonk/config.yaml` (`options:`/`schema:`). Rebuild add-on on the VM: `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild`.
- **Admin-API verification on the VM without printing the token** (memory `ha-vm-admin-api-testing`): fetch the token into a shell var via `bash tools/ha-vm/ssh.sh 'ha addons info local_byonk --raw-json | jq -r .data.options.admin_token'` (grep the hex; never echo it), then `curl` `localhost:3000/api/admin/*` from the **Mac host**.

## Reference

- **Redirection spec (active):** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md`.
- **Plan 3 (superseded placement, code merge-ready):** spec `…/specs/2026-07-04-screen-packages-p3-ha-config-design.md`, plan `…/plans/2026-07-04-screen-packages-p3-ha-config.md`. Root spec `…/specs/2026-07-02-screen-packages-design.md`.
- **SDD ledger:** `.superpowers/sdd/progress.md` (Plan 1 + 2 + 3 per-task reviews/commits + the final review + Issue-1 fix).
- **Memories:** `ha-addon-owned-global-config` (the redirection), `ha-vm-admin-api-testing`, `ha-addon-phase2`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Admin API (token-gated `/api/admin/*`)

`GET /devices|pending|config|screens|packages` (reads) · `POST/PATCH/DELETE /devices[/:key]` (per-device) · `PATCH /settings` · `POST/PATCH/DELETE /packages/:handle` + `POST /packages[/:handle]/update` (registry — to become read-only in add-on mode per the redirection).

## Config files (distinction)

- **`config.yaml`** (repo root) = developer's local test config. **`default-config.yaml`** = shipped/embedded default (device-free). On the VM, byonk's live app config is `/addon_configs/local_byonk/config.yaml` (byonk-owned; the integration never touches it — API only).
