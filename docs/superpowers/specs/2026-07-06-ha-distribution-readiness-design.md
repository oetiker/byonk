# Byonk ↔ Home Assistant — Distribution Readiness (Design)

_Date: 2026-07-06 — revises the 2026-06-29 "Phase 4: Release, Validation & Docs"
design for the model as actually shipped (screen-packages P1–3, add-on-owned global
config, reserved `DEFAULT` device, HA-owned per-device config entries)._

## Context

The HA stack is functionally complete and the distribution scaffolding already
exists in-repo — more than the original Phase 4 spec assumed:

- **Add-on repository is committed and installable today.** Root `repository.yaml`
  plus `homeassistant/byonk/` with a production `config.yaml` (direct prebuilt image
  `ghcr.io/oetiker/byonk`, `arch: [amd64, aarch64]`, current options schema including
  `packages`), `DOCS.md`, `CHANGELOG.md`, and `translations/en.yaml`.
- **Integration is HACS custom-repo ready:** `custom_components/byonk/` with
  `hacs.json` and `manifest.json`.
- **Release pipeline** (`.github/workflows/release.yml`) builds the multi-arch
  `ghcr.io/oetiker/byonk` image (`Dockerfile.release`, `linux/amd64,linux/arm64`) and
  publishes versioned docs.
- **VM test harness** (`tools/ha-vm/`) boots headless HAOS on Apple Silicon.

What is missing or stale is the *release-and-publish* machinery around that
scaffolding. This design closes those gaps and makes byonk **HACS default-list**
distributable.

### The gaps this design closes

1. **Version automation is not wired.** `release.yml` bumps only `Cargo.toml` /
   `CHANGES.md`. It never touches `homeassistant/byonk/config.yaml` `version:`, the
   add-on `CHANGELOG.md`, or `custom_components/byonk/manifest.json` `version`. Today
   the add-on is *manually* aligned at `0.15.0` (a real image tag, so it installs) and
   the integration manifest is frozen at `0.1.0`. On the next release the image ships
   as `0.16.0` while the add-on stays pinned at `0.15.0` — it keeps pulling the old
   image and never offers an update. This is the core rot.
2. **No HA validation CI.** `ci.yml` is Rust-only (fmt/clippy, test, build). No
   hassfest, no HACS action, no add-on config lint — all prerequisites for HACS
   default-list / brands review.
3. **No brand assets, no publishing doc.** No `icon.png` / `logo.png`; no drafted
   `home-assistant/brands` + `hacs/default` external-PR instructions.
4. **The full stack was never validated end-to-end on real HAOS,** and the original
   Phase 4 validation checklist is stale — it predates the reserved `DEFAULT` device,
   HA-owned per-device config entries, screen packages, and add-on-owned global config.

## Goal

1. **Validate the whole add-on + integration stack end-to-end on real HAOS** against
   the model as shipped — the gate before any external PR is filed.
2. Automate add-on / integration **versioning** so all three artifacts track the byonk
   release version.
3. Bring byonk to **HACS default-list** + **home-assistant/brands** readiness: an
   icon, CI validation, and drafted external PRs for the maintainer to file.
4. Align user **docs** with the shipped model.

## Non-goals / out of scope

- Bridged networking / real TRMNL hardware against the VM (user-mode NAT is enough to
  validate add-on + integration logic).
- **Any byonk Rust runtime change.** This design touches the release *workflow*, a
  bump *script*, CI, brand assets, and docs — not byonk's runtime.
- Auto-filing the external brands / hacs-default PRs (they need the maintainer's
  GitHub identity and external review).
- Supervised-on-Debian install path (the HAOS VM is the chosen test bed).

## Invariants (must not regress)

- The integration stays **Supervised/HAOS-only**.
- **Zero-touch / no-redundancy trust:** the admin token's single home is the add-on
  option; the config entry stores no token.
- **Source of truth for global config is the add-on Options tab** (`options.json`,
  restart-to-apply); the admin API is read-only for globals (`auth_mode`,
  `package_refresh_interval`, `packages`) and returns 409 on attempts to change them.
- Per-device screen/dither/panel assignment, the registration switch, and the
  "Update packages" button remain live over the admin API.

---

## Sequencing (given "re-spec now, merge later")

This spec becomes the implementation plan. VM validation and the branch merge are
**early tasks inside it**, not a separate pre-step:

```
4a  VM validation (refresh checklist + run) ─┐
    └─ merge feat/screen-packages-p2-distribution to main
                                             │  (4b/4c/4d land on merged main)
                                             ▼
4b version automation   4c HACS/brands + CI   4d docs   (parallel once merged)
                                             │
                                             ▼
                    first "real" release containing the integration
                                             │
                                             ▼
              maintainer files home-assistant/brands + hacs/default PRs
```

External PRs are the final manual step; nothing external is filed until 4a passes and
a release containing the integration exists.

---

## 4a — End-to-end HAOS validation (gating for the external PRs)

The harness (`tools/ha-vm/`) already exists; this workstream is **refresh the
checklist for the shipped model, then run it** — and it absorbs the handover's
outstanding "VM-verify Plan B" step.

Deploy commands (creds `byonk`/`byonk`, from `CLAUDE.md` / `tools/ha-vm/README.md`):
- add-on: `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` (Plan B touches no manifest
  → no `store/reload` + `ha addons update` schema dance).
- integration: `SMB_USER=byonk SMB_PASS=byonk make ha-deploy` then
  `make ha-ssh CMD="ha core restart"`.
- admin-API probe without printing the token (memory `ha-vm-admin-api-testing`):
  read `.data.options.admin_token` via `ha addons info local_byonk --raw-json` into a
  shell var, then `curl localhost:3000/api/admin/*` from the **Mac host** (`:3000`).

### Refreshed validation checklist (the "test" for 4a)

Record pass/fail in the SDD ledger. **Bold = changed from the 2026-06-29 checklist.**

1. **Add-on store:** adding `https://github.com/oetiker/byonk` as a repository shows
   the *Byonk* add-on; it installs (pulls the published `ghcr.io/oetiker/byonk` image)
   and starts; port 3000 serves a screen.
2. **Integration install:** `custom_components/byonk` deployed; after restart *Byonk*
   is discoverable in **Add Integration**.
3. **Zero-touch trust:** adding the integration (add-on *not* pre-installed) auto-adds
   the repo, installs + starts the add-on, provisions the admin token into the add-on
   option, and reads it back. The config entry stores **no** token.
4. **Global config is add-on-owned:** `auth_mode`, `package_refresh_interval`, and the
   `packages` registry are edited on the **add-on Options tab** and applied on restart;
   the integration presents them **read-only / monitoring**, and an admin-API write to
   any of them returns **409** pointing back to the Options tab.
5. **Reserved `DEFAULT` device:** `GET /api/admin/devices` includes a
   `{"key":"DEFAULT","reserved":true,…}` entry; the integration auto-provisions a
   **"Byonk Default"** device with a live **Screen select** (no dither/panel), exempt
   from reconcile/orphan-prune. `PATCH /api/admin/devices/DEFAULT {"screen":…}` → 200
   live (no restart); `DELETE /api/admin/devices/DEFAULT` → **409**. Deleting the HA
   "Byonk Default" device does **not** lose `devices.DEFAULT`, and HA re-provisions the
   entry on the next refresh (~60s).
6. **Screen resolution:** an **unregistered** device shows its pairing **code** (the
   `byonk-builtin/default` screen is registration-aware); a **registered-but-unassigned**
   device shows the `DEFAULT` device's screen.
7. **HA-owned per-device flow:** a pending device raises the onboarding path; adding it
   creates a **per-device HA config entry** (keyed by MAC) with a **Discovered** card;
   its screen/param/dither/panel entities write through live to the admin API and back.
   Per-screen `@params` render as HA selectors.
8. **Screen packages:** a configured package `handle`/`repo`/`pin` is fetched; its
   screens are selectable per device; the integration's **Update packages** button
   triggers a live refresh.
9. **Reauth:** blanking/invalidating the add-on token raises *Re-authentication
   required* and resolving re-provisions without manual input; a transient connection
   error does **not** trigger a reauth loop.
10. **Device-removal grace:** a device that disappears survives the documented grace
    window before its HA entry is pruned.

Any failure is fixed in the relevant code **before** 4b/4c go live.

**Exit:** checklist green on the VM → merge `feat/screen-packages-p2-distribution` to
`main` (via `superpowers:finishing-a-development-branch`). 4b/4c/4d proceed on merged
`main`.

---

## 4b — Release & version automation

All three versions couple to the **byonk release version** (the value the `version`
job computes in `release.yml`).

### Integration `manifest.json` — bump before the tag

HACS installs from the **GitHub release tag**, so `custom_components/byonk/manifest.json`
`version` must be correct *in the tagged tree*. Fold this into the existing `version`
job (where `Cargo.toml` / `CHANGES.md` are bumped, before commit + tag): set
`manifest.json` `version` to the new byonk version.

### Add-on `config.yaml` — bump after the image is published

The add-on is a direct-image add-on: HA Supervisor pulls `{image}:{version}` =
`ghcr.io/oetiker/byonk:{config.yaml version}`. The add-on `version:` must therefore
equal an **already-published** image tag. Because `release.yml`'s `version` job
commits + tags **before** `build-container` pushes the image, the add-on bump must run
**after** the image exists.

- Extract a testable script **`tools/release/bump-addon-version.sh <version>`** that:
  - rewrites `homeassistant/byonk/config.yaml` `version:`;
  - prepends a `homeassistant/byonk/CHANGELOG.md` entry;
  - on `release_type == major`, appends the new version to `breaking_versions:` in the
    add-on `config.yaml` (Supervisor then shows an update warning).
- New **`update-addon-version` job** in `release.yml`: `needs: [version,
  build-container]`, checks out `main`, runs the script with
  `needs.version.outputs.version`, commits + pushes to `main` as the github-actions bot.

**Consumption-timing note (correct, not a bug):** HA reads the add-on from the repo's
**default branch (`main`)**, so add-on users get `main`'s `config.yaml` — which, after
`update-addon-version`, points at the just-published tag. The GitHub *tag* tree won't
contain the add-on bump (it lands post-tag); that is fine because nothing consumes the
add-on from the tag. The integration, by contrast, *is* consumed from the tag, which is
why its bump lands **before** the tag.

### Verification

The bump script is unit-tested against fixtures (`config.yaml` / `CHANGELOG.md` /
`manifest.json` in → asserted rewritten output out) so the risky string-munging is
covered without a live release. Workflow job wiring (`needs:` graph, checkout ref,
commit/push) is reviewed for correctness.

---

## 4c — HACS default-list + brands prep

Target: the **HACS default store** + **home-assistant/brands**. In-repo readiness is
built here; the two external PRs are drafted for the maintainer to file.

### In-repo readiness

- **`hacs.json`:** keep minimal but correct for a single integration under
  `custom_components/byonk/` (`name`, `homeassistant` minimum-supported-core version,
  `render_readme`). Verify against current HACS expectations.
- **`manifest.json`:** confirm all HACS/hassfest-required keys are present and correct
  (`domain`, `name`, `version`, `documentation`, `issue_tracker`, `codeowners`,
  `iot_class`, `config_flow`, `integration_type`).
- **CI validation (new job in `ci.yml`):**
  - **hassfest** (`home-assistant/actions/hassfest`) against the integration.
  - **HACS action** (`hacs/action` with `category: integration`).
  - an **add-on config lint** for `homeassistant/byonk/config.yaml` (schema/keys sanity;
    the community add-on linter or an equivalent check, resolved at implementation).
  This is the in-repo proof the external reviews will pass, enforced on every push.

### Brand icon

- Design a minimal, e-ink-appropriate **high-contrast mono** byonk mark as **committed
  SVG source** and rasterize with a reproducible script (`rsvg-convert` / Inkscape,
  resolved at implementation) to the brands-required PNGs: `icon.png` (256×256),
  `icon@2x.png` (512×512), `logo.png` (+ `logo@2x.png` if a wordmark is wanted). Legible
  at 256px. Candidate marks are shown for approval during implementation of this task
  before anything is committed. The mark can be swapped later without reopening the spec.

### Drafted external submissions (maintainer files them)

A doc **`docs/superpowers/ha-publishing.md`** with copy-pasteable instructions + exact
file contents/locations for:
- **home-assistant/brands** PR: `custom_integrations/byonk/{icon,icon@2x,logo}.png`.
- **hacs/default** PR: add `oetiker/byonk` to the `integration` list (alphabetical).
- **Ordering:** brands first (HACS validation checks brands), then default-list; both
  require a published GitHub release containing the integration.

These are **not** auto-filed.

---

## 4d — Docs polish (alongside 4c)

- **`docs/src/guide/ha-addon.md`** and **`docs/src/guide/ha-integration.md`:** align
  with the shipped model (reserved `DEFAULT` device, HA-owned per-device entries,
  add-on-owned global config on the Options tab). These are largely current; the main
  edit is the HACS install story — **custom-repo steps now, switch to default-store
  search once accepted** to HACS default-list.
- **Dev docs:** the test-VM harness stays documented in `tools/ha-vm/README.md`, linked
  from the contributor/dev section (kept out of the user-facing install flow).
- **`CHANGES.md`** (Unreleased): note the release-automation + docs changes that are
  user-visible; the add-on `CHANGELOG.md` is handled by the 4b bump script going
  forward.

---

## Testing summary

| Deliverable | How verified |
|---|---|
| 4a validation | HAOS boots headless; refreshed checklist passes on the VM; recorded in the SDD ledger; branch merged |
| 4b automation | unit-tested `bump-addon-version.sh` against fixtures; `release.yml` job-wiring review |
| 4c readiness | hassfest + HACS action + add-on lint green in CI; icons render at target sizes |
| 4d docs | `make docs` clean; pages match shipped behavior |

## Reference

- Supersedes for distribution: `docs/superpowers/specs/2026-06-29-byonk-homeassistant-phase4-release-and-docs-design.md`.
- Shipped-model specs: `2026-07-04-addon-owned-global-config-design.md` (§4a/§5.6/§6 =
  reserved `DEFAULT` device), `2026-06-30-byonk-homeassistant-phase5-ha-owned-devices-design.md`,
  screen-packages P1–3 specs/plans.
- Executed model plan: `docs/superpowers/plans/2026-07-05-reserved-default-device.md`.
- Memories: `ha-addon-owned-global-config`, `ha-addon-phase2`, `ha-vm-admin-api-testing`,
  `ha-vm-addon-manifest-sync-gap`, `byonk-is-ours-change-apis-freely`, `no-git-add-all`.
