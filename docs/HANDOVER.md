# Handover — Byonk

_Last updated: 2026-07-11 — **HA Distribution Readiness: all code/doc tasks (1–8) implemented, per-task-reviewed, and whole-branch-reviewed clean (opus: READY TO MERGE).** Branch `feat/ha-distribution-readiness` @ `2667b89`, tree clean. Bump-script bash tests + `cargo test --test addon_manifest_test` + `make docs` all GREEN locally. **Not yet VM-verified end-to-end; not merged.** The three remaining tasks are all MANUAL/human (drive the QEMU VM, merge, file external PRs)._

## TL;DR — resume here

The distribution plan `docs/superpowers/plans/2026-07-06-ha-distribution-readiness.md` has 11 tasks. **Tasks 1–8 (all automatable code/doc work) are DONE + reviewed.** What remains is manual:

1. **Task 9 — VM validation (do first).** Deploy the whole stack to the HAOS test VM and work the refreshed 10-item checklist now in `tools/ha-vm/README.md` (§"Validation Checklist"). This is the pre-publish gate and also subsumes the old "VM-verify Plan B" step (no runtime changed since). Commands: `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (add-on), `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` + `make ha-ssh CMD="ha core restart"` (integration). Probe the admin API from the Mac host (`curl localhost:3000/...`), never printing the token (memory `ha-vm-admin-api-testing`).
2. **Task 10 — Merge** (`superpowers:finishing-a-development-branch`). This branch is stacked on Plan B, so the PR carries Plan 1+2+3+A+B **and** the distribution work — it all merges to `main` together. **On the PR run, confirm the new `home-assistant` (hassfest + HACS) and `release-scripts` CI jobs go green** — that's the first real exercise of hassfest/HACS (they can't run locally).
3. **Task 11 — External PRs (post-first-release), maintainer-filed.** Follow `docs/superpowers/ha-publishing.md`: file the `home-assistant/brands` PR, then the `hacs/default` PR, then remove `ignore: brands` from `ci.yml` and switch `ha-integration.md` to default-store wording.

## What the distribution work shipped (Tasks 1–8, range `586e3d3..2667b89`)

- **Version automation (4b):** two tested bash scripts — `tools/release/bump-addon-version.sh` and `bump-integration-version.sh` — wired into `release.yml`. The integration `manifest.json` bumps in the `version` job **before the tag** (HACS installs from the tag); the add-on `config.yaml` bumps in a new `update-addon-version` job `needs:[version,build-container]`, committing to `main` **after** the image publishes (so the add-on `version:` always equals a published `ghcr.io/oetiker/byonk` tag). All three versions now track the byonk release version.
- **Validation CI (4c):** new `ci.yml` jobs — `home-assistant` (hassfest + `hacs/action`, with `ignore: brands` until the brands PR merges + `GITHUB_TOKEN` passed) and `release-scripts` (runs both bump-script test harnesses). The add-on `config.yaml` is already validated by the existing `tests/addon_manifest_test.rs` (extended with a semver guard).
- **Brand assets (4c):** `homeassistant/brands/` — **user-supplied pixel-art** (a shinkansen departure board with the green **BYONK** sign), committed as `*.src.png` masters (1024px) + `rasterize.sh` (sips resize) → brands `icon.png`/`icon@2x`/`logo`/`logo@2x` + add-on `icon.png`/`logo.png`.
- **Runbook + docs (4c/4d):** `docs/superpowers/ha-publishing.md` (brands + hacs/default PR steps); `ha-integration.md` HACS custom-repo install note; `CHANGES.md` Unreleased bullet; refreshed 10-item VM checklist.

## Verification status (local, all GREEN at `2667b89`)

- Bump scripts: `tools/release/test-bump-addon-version.sh` + `test-bump-integration-version.sh` pass.
- Rust: `cargo test --test addon_manifest_test` passes (2/2). Full `make check` not re-run this session (no `src/` change — only a test file); known pre-existing flaky `tests/e2e_flow_test.rs::test_content_cache_reuse` is unrelated.
- Docs: `make docs` clean.
- **Whole-branch review (opus, `8ddba14..2667b89`): READY TO MERGE**, no Critical/Important; release timing verified sound.

## Deferred Minors (fast-follow — do NOT gate the merge)

- **M1 (most useful):** `update-addon-version` is a leaf job. If it FAILS, the release still tags/publishes/creates the GitHub release, but the add-on `config.yaml` on `main` stays at the old version — silently re-introducing the exact "add-on never offers the update" rot this work fixes. Recoverable (re-run from clean `main`, idempotent). **Fix:** a runbook line that a red `update-addon-version` must be re-run before a release counts as done.
- **M2:** `bump-addon-version.sh` major-bump EOF append (`printf >> config.yaml`) is newline-fragile if `config.yaml` ever loses its trailing newline (safe today). Fix: guarded newline / perl append.
- **M3:** hassfest + `hacs/action` are first exercised only on the Task-10 PR run (unverifiable locally).

## Branch / merge state

- **Branch `feat/ha-distribution-readiness` @ `2667b89`** (tree clean). Stacked on `feat/screen-packages-p2-distribution` (Plan B). Local-only, not pushed, not merged.
- Distribution range **`586e3d3..2667b89`** = 11 commits (spec + plan + 8 tasks + 1 fix). Fork point from Plan B = `8ddba14`.

## Reference

- **Plan (executed 1–8):** `docs/superpowers/plans/2026-07-06-ha-distribution-readiness.md`.
- **Spec:** `docs/superpowers/specs/2026-07-06-ha-distribution-readiness-design.md` (revised Phase 4; §4a = the VM checklist).
- **SDD ledger:** `.superpowers/sdd/progress.md` — per-task review record + the deferred-Minor detail. Trust it + `git log` over memory after a compaction.
- **Publishing runbook (Task 11):** `docs/superpowers/ha-publishing.md`.
- **Memories:** `ha-addon-owned-global-config`, `ha-addon-phase2`, `ha-vm-admin-api-testing`, `ha-vm-addon-manifest-sync-gap`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.

## Build / verify quick ref

- Bump scripts: `tools/release/test-bump-addon-version.sh && tools/release/test-bump-integration-version.sh`.
- Rust test: `cargo test --test addon_manifest_test`. Docs: `make docs`. Brand assets: `homeassistant/brands/rasterize.sh` (needs `sips`).
- VM deploy: `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (add-on) / `make ha-deploy` (integration).
