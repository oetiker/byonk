# Handover — Byonk

_Last updated: 2026-07-17 — **Fetch bug fix + "packages" → "screen repos" rename (everything incl. API) + HA error-visibility feature. VM-verified live; PR #27 OPEN.** Branch `fix/screen-repos-and-fetch-scratch` (off `main` `e61658d`), pushed. All local gates green; VM live-verify passed via the from-source add-on. Remaining: PR #27 CI + review → squash-merge → release 0.16.1._

## Status: PR #27 open, VM-verified
- **PR:** https://github.com/oetiker/byonk/pull/27 — CI running at hand-off.
- **VM live-verify (from-source `local_byonk`):** Supervisor accepted the new `screen_repos` schema; `GET /api/admin/screen-repos` serves the `tobitest` repo (`https://github.com/oetiker/byonk-dist-test.git`) at `status: ready`, sha `97578c5f`, serving `tobitest/hello`; old `/api/admin/packages` → 404; `GET /screens` wrapper field is `screen_repos`. The `/tmp` fetch error is gone. **VM state now:** `local_byonk` running (my branch, throwaway admin_token `verifytoken123` + tobitest), published `43664941_byonk` **stopped**, the existing integration entry still points at the stopped published add-on (in error) — re-onboard the integration (needs HA UI login) to see the renamed entities/button/repair issue, or restore by `ha addons stop local_byonk && ha addons start 43664941_byonk`.
- Integration UI not verified live (HA login = password entry, disallowed) but covered by 77 pytest + the API wire check.

## TL;DR — what happened this session

User reported: added a screen-repo (their `byonk-dist-test` repo) in the add-on, clicked refresh, got "an error but no further information." Root-caused and fixed, then did a full user-requested rename.

1. **Bug fix (`1b2ed93`)** — screen-repo fetch was **completely broken in the published container**: the release image is `FROM scratch` (no `/tmp`), and `git_fetch.rs` cloned the intermediate bare repo into `std::env::temp_dir()` (`/tmp/…`), which gix can't create there → `Could not open data at '/tmp/byonk-git-fetch-…'`, status `error`. Fix: clone into a **sibling of `dest`** (under the package cache on `/data`, guaranteed to exist). Also added `tracing` warn/info so fetch failures hit the add-on log, not only `GET`.
   - **Proven**: identical fetch succeeded on Mac; and with `TMPDIR=/no/such/tmp` (simulating the scratch container) the fixed fetch returns `PROBE_OK`. Unit test `test_scratch_is_sibling_of_dest_not_system_temp`.

2. **Rename "packages" → "screen repos" (`ae57f43`, `f685be9`, `5b9fec7`)** — user chose: term **"Screen Repos"**, scope **everything incl. the HTTP API**, and **surface errors visibly**. A screen repo = a git repository of screens.
   - **API**: `/api/admin/packages*` → `/api/admin/screen-repos*`; `GET /screens` grouping wrapper field `packages` → `screen_repos`; settings field `package_refresh_interval` → `screen_repo_refresh_interval`. Per-repo fields (handle, repo, pin, builtin, status, error, …) unchanged.
   - **config.yaml / add-on options + schema**: `packages` → `screen_repos`, `package_refresh_interval` → `screen_repo_refresh_interval`; env `PACKAGES_CACHE_DIR` → `SCREEN_REPOS_CACHE_DIR` (value `/data/packages` kept).
   - **Rust internals**: `Package{Manager,Loader,Cache,Status,State,Ref,Manifest,Info,Source}` → `ScreenRepo*`; files `src/services/screen_repo_*.rs`, `src/models/screen_repo_manifest.rs`; `config_writer` upsert/remove fns renamed.
   - **HA integration**: API client, coordinator (`screen_repos`, `non_builtin_screen_repos`, `screen_repo()`), `screen_repo_entities.py` / `ByonkScreenRepoStatusSensor`, button "Update screen repos", strings/translations.
   - **KEPT**: `byonk-builtin` handle value, `byonk-screens.yaml` filename, `/data/packages` path value — so device screen paths and on-disk layout are unaffected.

3. **Error-visibility feature (`f685be9`)** — a screen repo in `error` state now raises a **HA Repair issue** ("Screen repo X failed to update") carrying the real fetch error, auto-cleared on recovery. Translation `issues.screen_repo_error` in strings.json/en.json. Tests: `tests_ha/test_screen_repo_issues.py`.

## Verification status (all GREEN at `5b9fec7`)
- Rust: `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test` (73/5/2/… all ok) — green.
- HA: `make ha-check` (ruff + **77 pytest**) — green.
- Docs: `make docs` (mdbook) — clean (only harmless mermaid version warning).
- Fetch fix functionally proven via `TMPDIR=/no/such/tmp` probe (`PROBE_OK`).

## Remaining
1. **(optional) VM live-verify** — needs the **from-source** local add-on (`local_byonk`) rebuilt on the QEMU VM (the VM currently runs the *published* 0.16.0 scratch image with the OLD naming + the bug). Recipe: memory `ha-vm-from-source-addon-build` + `tools/ha-vm/README.md`; re-scaffold `local_byonk` (config.yaml sans `image:` + Debian Dockerfile), `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild`, `make ha-deploy`. Then in the add-on Config tab add a `screen_repos:` entry for `https://github.com/oetiker/byonk-dist-test.git` and confirm it fetches + shows as a screen repo + the button works; point at a bad repo to see the Repair issue. NOTE: the Debian from-source image HAS `/tmp`, so it validates the **rename/boundary**, not the scratch `/tmp` fix (already proven by the probe). The scratch fix is confirmed in production only at the 0.16.1 release image.
2. **Push + PR + release 0.16.1** — not started; user not yet asked. Branch is off `main`; `main-protect` ruleset blocks direct pushes (use PR; release uses `RELEASE_TOKEN` + admin bypass, per the 0.16.0 memory).

## Reference
- Rename mapping spec (durable): `…/scratchpad/rename-spec.md` (session scratch).
- Memories: `ha-vm-from-source-addon-build`, `ha-vm-addon-manifest-sync-gap`, `ha-addon-owned-global-config`, `ha-vm-admin-api-testing`, `changelog-user-facing-only`, `no-git-add-all`, `byonk-is-ours-change-apis-freely`.
- Build/verify: `make check` (Rust), `make ha-check` (Python), `make docs`.
