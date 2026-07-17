# Handover — Byonk

_Last updated: 2026-07-17 — **HA Distribution Readiness: code/doc tasks 1–8 DONE + reviewed clean, AND Task 9 (fresh-VM end-to-end validation) DONE — 9/10 checklist items PASS on a clean HAOS build-from-source; item 1 (published image) is inherently post-release.** Branch `feat/ha-distribution-readiness` @ `01d8d1c` (pushed, PR #23 OPEN, CI green), tree clean, not merged. Remaining: Task 10 (merge → release 0.16.0) and Task 11 (external brands/HACS PRs, post-release). Two findings surfaced during validation — see below._

## TL;DR — resume here

The distribution plan `docs/superpowers/plans/2026-07-06-ha-distribution-readiness.md` has 11 tasks. **Tasks 1–9 are DONE.** What remains is manual:

1. ~~**Task 9 — VM validation.**~~ **DONE 2026-07-17** on a wiped/fresh HAOS VM, byonk built from current source as a local add-on (see "VM validation results" below). Zero-touch install→trust→device-discovery→reauth→removal-grace all validated end-to-end.
2. **Task 10 — Merge + release (do next).** `superpowers:finishing-a-development-branch`. PR #23 carries Plan 1+2+3+A+B + distribution work; merges to `main` together. Confirm the `home-assistant` (hassfest+HACS) and `release-scripts` CI jobs stay green on the merge. Then trigger the release (`workflow_dispatch` on `release.yml`, minor → **0.16.0**) for the first (non-publicised) release.
3. **Task 11 — External PRs (post-first-release), maintainer-filed.** Follow `docs/superpowers/ha-publishing.md`: file `home-assistant/brands` PR (this is what lights up the currently-missing integration icon — see finding B), then `hacs/default`, then remove `ignore: brands` from `ci.yml` and switch `ha-integration.md` to default-store wording.

## VM validation results (2026-07-17, fresh HAOS + from-source add-on)

Method: `make ha-vm-clean` + fresh boot; onboarded via Chrome; Samba + Terminal&SSH add-ons installed; byonk built **from current source** as local add-on `local_byonk` (published-image path is post-release only). Checklist (`tools/ha-vm/README.md`):

| # | Item | Result |
|---|------|--------|
| 1 | Add-on store / published image | ⏭️ deferred — post-release only (built from source) |
| 2 | Integration discovery | ✅ |
| 3 | Zero-touch trust (no token entry) | ✅ token auto-provisioned into add-on options |
| 4 | Add-on-owned global config | ✅ settings/packages writes → **409** "edit in add-on Configuration tab" |
| 5 | Reserved DEFAULT device | ✅ `reserved:true`, PATCH→200, DELETE→**409**, "Byonk Default" auto-provisioned |
| 6 | Screen resolution (unregistered) | ✅ device shows pairing code; display→200 |
| 7 | HA-owned per-device flow | ✅ Discovered card → Add → per-device entry (9 entities), code-labeled onboarding |
| 8 | Screen packages | ✅ external git package fetched (`disttest` ready, screen served) — **see finding A** |
| 9 | Re-authentication | ✅ blanked token → integration **auto-re-provisioned** (self-heal, no manual input) |
| 10 | Removal grace | ✅ device disappeared → HA entry survived 1 cycle, pruned at strike 2 (`REMOVE_STRIKES=2`×60s) |

### Findings (not merge-blockers for a non-publicised release; consider fast-follow)

- **Finding A — schemeless package `repo:` URLs — FIXED (validate, not normalize).** `repo: github.com/…` used to be handed to gix as a **local path** (`/app/github.com/…`) → obscure `status:error`. Now `git_fetch::fetch` calls `validate_repo()` up front: a `repo:` must carry an explicit scheme (`https://`/`http://`/`git://`/`ssh://`/`file://`) or be scp-like (`git@host:owner/repo`); schemeless values and bare paths are rejected with a clear message ("…must be `https://…`; a local repository must be `file:///path`"). Local repos now require `file:///` (per user direction — no normalization). Docs (`configuration.md`) updated; unit tests added; `make check` green. **Not yet committed** (working tree on `feat/ha-distribution-readiness`).
- **Finding B — integration icon missing until brands PR.** Fresh integration shows "icon not available" because HA fetches integration icons from `brands.home-assistant.io/byonk/` (populated by the Task-11 `home-assistant/brands` PR). The **add-on** icon renders fine (ships locally). Expected; not a blocker; assets are staged in `homeassistant/brands/`.

### From-source add-on build notes (for repeatable fresh-VM validation)

The local-add-on build-from-source path is **not committed** and had to be authored this session (scaffold in scratch: `config.yaml` sans `image:` + a Dockerfile). Key requirements discovered:
- Build context = the full source tree byonk embeds at compile time via rust-embed (`screens/ fonts/ byonk-base/ static/` + `src crates Cargo.toml Cargo.lock default-config.yaml`) — exactly what `rebuild.sh` syncs.
- Use a **Debian** rust base (`rust:1.88-slim-bookworm` + `apt install curl gcc libc6-dev`): `utoipa-swagger-ui`'s build script downloads Swagger UI via **`curl`** (Alpine attempts failed / were cache-replayed).
- BuildKit **replays cached failed layers** across Dockerfile edits — bump the add-on `version:` (changes the `BUILD_VERSION` build-arg referenced before `cargo build`) to force a real rebuild; a bare `ha store reload` won't re-read the manifest, need `ha supervisor restart` (see `ha-vm-addon-manifest-sync-gap`).
- **Consider committing** a `homeassistant/byonk/` local-build variant + wiring `make ha-rebuild` to scaffold it, so Task-9-style validation is one command next time.

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
