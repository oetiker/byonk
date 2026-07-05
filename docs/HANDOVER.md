# Handover — Byonk

_Last updated: 2026-07-05 — **Plan B (reserved DEFAULT device) is IMPLEMENTED, fully reviewed, and LOCAL-verified.** All 10 tasks + a final-review fix wave landed via subagent-driven execution; the whole-branch review (opus) plus the fix re-review are clean with no open Critical/Important. Branch `feat/screen-packages-p2-distribution` @ `236583a`, tree clean. Rust `make check` GREEN, HA `ruff` clean + `pytest tests_ha` 74 passing, `make docs` clean. **Not yet VM-verified; not merged.** Two things remain: (1) VM verification, (2) finish the branch._

## TL;DR — resume here

1. **VM-verify Plan B** (the one thing not yet done). Deploy to the HAOS test VM and eyeball the DEFAULT-device behavior — see §"VM verification" for exact commands + checks. **Plan B touches NO add-on manifest, so a plain `make ha-rebuild` suffices — no `store/reload`+`ha addons update` schema dance** (contrast the `ha-vm-addon-manifest-sync-gap` that bit the Plan-A session).
2. **Finish the branch** (`superpowers:finishing-a-development-branch`). The branch carries Plan 1 + 2 + 3 + A + B — the entire add-on-owned-global-config redirection. Merge it all together once VM-verify passes.
3. **Optional fast-follow polish** (non-blocking Minors, listed in §"Fast-follow"). None gate the merge.

## What Plan B did (shipped)

Replaced byonk's `AppConfig.default_screen` + `RegistrationConfig.screen` with a single reserved `DEFAULT` device (`config.devices["DEFAULT"]`, key `"DEFAULT"`). Resolution for every not-yet-configured device is now `device.screen → devices["DEFAULT"].screen → built-in fallback`. The shipped `byonk-builtin/default` screen is registration-aware (renders the pairing code for unregistered devices). `registration.enabled` kept. The DEFAULT device is read/written over the per-device admin API (live, allowed in add-on mode). In HA it auto-provisions a **"Byonk Default"** device entry with a live screen-select, exempt from reconcile/orphan-prune, and protected against manual deletion (both an HA-side no-op guard and a byonk-side 409 on `DELETE /devices/DEFAULT`). Core model change → standalone byonk and the add-on both use it.

## VM verification (the remaining pre-merge step)

Commands (creds `byonk`/`byonk`):
- **byonk add-on:** `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (syncs source + rebuilds; no manifest change → no schema dance).
- **integration:** `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` then `make ha-ssh CMD="ha core restart"`.
- **admin-API probe without printing the token** (memory `ha-vm-admin-api-testing`): `bash tools/ha-vm/ssh.sh 'ha addons info local_byonk --raw-json' | jq -r .data.options.admin_token` into a shell var, then `curl localhost:3000/api/admin/*` from the **Mac host** (`:3000`).

Checks (byonk side is API-verifiable from the host; the e-ink visual + HA-UI card are the user's eyeball):
- `GET /api/admin/devices` includes a `{"key":"DEFAULT","reserved":true,...}` entry.
- `PATCH /api/admin/devices/DEFAULT {"screen":"<known>"}` → 200 live in add-on mode (no restart); `DELETE /api/admin/devices/DEFAULT` → 409.
- An **unregistered** device shows its pairing **code** (DEFAULT screen is registration-aware); a **registered-but-unassigned** device shows the DEFAULT screen.
- **HA UI:** the **Byonk Default** device card exposes a Screen select (no dither/panel); changing it writes live. Deleting the Byonk Default device must NOT lose `devices.DEFAULT` in byonk, and HA must re-provision the entry on the next refresh (~60s).

## Verification status (local, all GREEN at `236583a`)

- **Rust:** `make check` (fmt + clippy `-D warnings` + tests) green. Known PRE-EXISTING flaky test `tests/e2e_flow_test.rs::test_content_cache_reuse` (reproduced on the pre-Plan-B HEAD — NOT a Plan B defect).
- **HA integration:** `ruff check custom_components/byonk tests_ha` clean; `pytest tests_ha -q` 74 passing.
- **Docs:** `make docs` clean.

## Branch / merge state

- **Branch `feat/screen-packages-p2-distribution` @ `236583a`** (tree clean). **Local-only — no upstream** (`git pull` needs an explicit remote/branch). Not merged, not pushed.
- **Plan-B range `746efdd..236583a`** = 11 commits (10 tasks + 1 fix wave). Plan-A range `8b7c7fe..dbf4613`. Plan-B plan doc + prior handover at `cff3b8f`/`746efdd`.

## Fast-follow (non-blocking Minors — do NOT gate the merge)

From the final review + carried per-task Minors (full detail in the SDD ledger):
- `render_unassigned_screen` is unreachable in production (registered-unassigned-with-unresolvable-DEFAULT returns a typed `_error` instead of the friendly screen). Near-impossible case since `byonk-builtin/default` always resolves. Optionally wire the registered-unassigned error path through `render_builtin_fallback(None, …)`.
- `_async_sync_discovery`'s abort loop could transiently abort the DEFAULT provision flow (self-heals next refresh); optionally exclude `DEFAULT_DEVICE_KEY` from the abort predicate.
- `entity.py` DEFAULT/non-DEFAULT `DeviceInfo` branch duplication; coordinator `_async_provision_default`/`_async_sync_discovery` share a 3-line lookup — readability only.
- `display.rs` keeps `.filter(|s| !s.is_empty())` after `default_device_screen()` but `main.rs` does not (cosmetic; both converge on the fallback for an empty string).
- CHANGES.md was not touched by the fix commit (pre-release bug-squash on an already-documented unreleased feature).

## Reference

- **Plan B (executed):** `docs/superpowers/plans/2026-07-05-reserved-default-device.md`.
- **Spec:** `docs/superpowers/specs/2026-07-04-addon-owned-global-config-design.md` (§4a/§5.6/§6 = Plan B).
- **Plan A (done + VM-verified byonk-side):** `docs/superpowers/plans/2026-07-04-addon-owned-global-config.md`.
- **SDD ledger:** `.superpowers/sdd/progress.md` — full Plan-A + Plan-B per-task record incl. the final review, fix wave, and re-review. Trust it + `git log` over memory after a compaction.
- **Memories:** `ha-addon-owned-global-config` (updated: Plan B written+executed), `ha-vm-addon-manifest-sync-gap`, `ha-vm-admin-api-testing`, `ha-addon-phase2`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Build / verify quick ref

- byonk: `make check`, `make docs`. Plan B touches no add-on manifest.
- HA: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` (74). Deploy: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"`.
- VM add-on rebuild: `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (no manifest dance for Plan B).
