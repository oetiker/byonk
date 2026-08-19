# The Byonk app installs its own integration

*Design, 2026-08-19. Branch `feat/addon-installs-integration`, cut from `main` at
958b14f.*

## Problem

Installing Byonk into Home Assistant currently requires HACS. HACS is itself a
manual install, and Byonk's HACS default-store PR (#9310) has been queued since
2026-07-18. Users without HACS have no route in.

The integration already installs the app for them
(`custom_components/byonk/config_flow.py:94` → `addon.py:39`), so the app half is
solved. The integration half is not.

## Goal

A Home Assistant user installs **one** thing — the Byonk app, from the app store —
and Byonk works. HACS is not involved, and the user never has to know that Byonk
has two halves.

## Decisions

1. **The app carries the integration.** On start it writes
   `custom_components/byonk/` into the Home Assistant config directory. Byonk
   needs the app anyway, so a throwaway installer app (the "Get HACS" pattern)
   would be pure overhead.
2. **HACS is dropped entirely**, not merely made optional. `hacs.json`, the HACS
   CI job and the HACS docs all go. PR #9310 should be closed (maintainer action,
   outside this branch).
3. **The restart nudge is a persistent notification.** Home Assistant loads
   `custom_components/` only at start, so one restart is unavoidable. A log line
   would be missed; an app that restarts Home Assistant by itself is worse.
4. **Version skew warns, it does not block.** When the app is newer than the
   loaded integration, entities keep working and the integration raises a repair
   issue pointing at the restart.
5. **The docs merge into one task-ordered page.** The split between "app" and
   "integration" leaves the table of contents.

## Facts this design rests on

Each was read from the current upstream source, not assumed.

| Fact | Source |
|---|---|
| `addon_config:rw` mounts at `/config`; `homeassistant_config:rw` mounts at `/homeassistant`. No clash with byonk's existing `/config` use. | `supervisor/docker/const.py:193-194`, `supervisor/docker/app.py:456-491` |
| The security rating ignores `map` and `homeassistant_api`. It scores AppArmor, ingress, privileges, `hassio_role` and host namespaces only. | `supervisor/apps/utils.py:19-86` |
| Discovery service names are free-form strings; the only rule is that the app lists the service in its `discovery:` key. | `supervisor/api/discovery.py` (`SCHEMA_DISCOVERY`, `set_discovery`) |
| `/discovery.*` needs no API role — it is in the security middleware's `api_bypass` set. | `supervisor/api/middleware/security.py:105` |
| Discovery messages dedupe on (app, service): `Message` marks `config` and `uuid` as `compare=False`. Posting on every start is idempotent. | `supervisor/discovery/__init__.py:31-99` |
| If Home Assistant is down, the message is stored and the push skipped; Home Assistant re-reads the whole list at `EVENT_HOMEASSISTANT_START`. So first install produces no error and the card is waiting after the restart. | `supervisor/discovery/__init__.py:117-121`, `core/components/hassio/discovery.py:36-52` |
| Home Assistant turns a discovery message into a config flow for the domain named by `service`, source `hassio`. Custom integrations resolve normally. | `core/components/hassio/discovery.py:113-140` |
| `/core/api/*` is gated on `homeassistant_api: true`; only `hassio*` paths are denied to apps. `services/persistent_notification/create` is reachable. | `supervisor/api/proxy.py:38,99-116,170-175` |
| `hacs.json` is optional — HACS recognises a repository from `custom_components/<domain>/manifest.json`. Deleting it breaks no existing install. | HACS publish docs |
| The release workflow already fails unless `Cargo.toml`, `custom_components/byonk/manifest.json` and `homeassistant/byonk/config.yaml` carry the same version. | `.github/workflows/release-publisher.yml:64-66` |

## A. App side

### `homeassistant/byonk/config.yaml`

```yaml
map:
  - addon_config:rw
  - homeassistant_config:rw
homeassistant_api: true
discovery:
  - byonk
```

### `Dockerfile.release`

Both arch stages gain `COPY custom_components/byonk ./custom_components/byonk`.
The build context is the repository root, so no embedding crate is needed and the
install works with no network.

### New module `src/ha_integration.rs`

A sibling of `src/addon_options.rs`, run once at startup and gated on the existing
`addon_mode` flag (`src/server.rs:82`).

1. Read the version from `<ha_config>/custom_components/byonk/manifest.json`. If
   it equals ours, return — nothing to do.
2. Copy `/app/custom_components/byonk` into
   `<ha_config>/custom_components/.byonk-new`, then swap it into place. Create
   `<ha_config>/custom_components/` if it does not exist (a Home Assistant install
   that has never had a custom integration has no such directory), and remove a
   leftover `.byonk-new` from a crashed earlier run before starting.
3. `POST http://supervisor/core/api/services/persistent_notification/create` with
   the Supervisor token: *"Byonk integration installed — restart Home Assistant to
   finish setup."*
4. `POST http://supervisor/discovery` with `{"service": "byonk", "config": {}}`.

Both HTTP calls use `SUPERVISOR_TOKEN` from the environment.

**Deletion safety.** Step 2 removes a directory inside the user's Home Assistant
config, so the code refuses unless the target path is exactly
`<ha_config>/custom_components/byonk` **and** it either does not exist or contains
a `manifest.json` whose `domain` is `byonk`. Anything else: log a warning, change
nothing. `<ha_config>` is never walked, globbed or cleaned beyond that one
directory and its `.byonk-new` sibling.

**Best effort.** Every failure logs a warning and lets startup continue. A
read-only config directory must never stop the server.

**Test hooks.** `BYONK_HA_CONFIG_DIR` and `BYONK_INTEGRATION_SRC` override the two
paths, mirroring the existing `BYONK_OPTIONS_FILE`.

## B. Integration side

### The Discovered card

`config_flow.py` gains the standard pair:

- `async_step_hassio` — abort with `single_instance_allowed` if an entry exists,
  then hand off to the confirm step.
- `async_step_hassio_confirm` — a one-button form, then the existing provisioning
  path unchanged: `async_ensure_addon_installed` → `async_read_token` /
  `async_provision_token` → `_async_probe_ready` → create entry.

That tail is currently inline in `async_step_user`. Factor it into one private
helper both steps call; do not duplicate it. `async_step_user` stays, so **Add
Integration** still works by hand.

New strings in `strings.json` and `translations/en.json`.

### The version-mismatch repair issue

No new byonk API. The integration compares:

- its own version, from `async_get_integration(hass, DOMAIN).version`, against
- `info.version` from `AddonManager.async_get_addon_info()`, which `addon.py`
  already calls.

**Read the version through the loader, never from the file.**
`async_get_integration` returns the cached `Integration` for a component that is
already set up, so its `version` is the manifest as read when Home Assistant
started. That is precisely what "the loaded integration" means here. Reading
`manifest.json` from disk instead would report the version the app just wrote, the
two numbers would always agree, and the warning would never fire.

If they differ, raise a repair issue with `translation_key: "version_mismatch"`,
severity `WARNING`, and clear it when they agree — the pattern already at
`coordinator.py:125-160`, including its sweep for issues that no longer apply.

Text: *Byonk app is 0.19.0 but the loaded integration is 0.18.0. Restart Home
Assistant to load the matching integration.*

## C. Docs, and removing HACS

### Deletions

| File | Change |
|---|---|
| `hacs.json` | delete |
| `.github/workflows/ci.yml:101-102` | drop the `hacs/action@main` job; `hassfest` stays |
| `tests_ha/test_manifest.py:21` | drop `test_hacs_json_parses` |
| `docs/superpowers/ha-publishing.md` | drop the HACS store half; keep the brand images half |

### One page

`docs/src/guide/ha-addon.md` and `docs/src/guide/ha-integration.md` merge into
`docs/src/guide/home-assistant.md`, with a single nav entry **Home Assistant** in
`docs/src/SUMMARY.md`. Ordered by what the user does:

1. **Install** — add `https://github.com/oetiker/byonk` to the app store, install
   and start **Byonk**, restart Home Assistant when the notification asks, click
   **Configure** on the Discovered card.
2. **Point your TRMNL device at Byonk** — host port 3000.
3. **Onboard a device** — the Discovered card flow.
4. **Entities** — hub, Byonk Default, per-device.
5. **Settings** — the app options table.
6. **Upgrading from an earlier install** — one line for HACS users.

The words *app* and *integration* appear only where the user must click on one.
The current "A full Home Assistant setup is **two parts** ... Install both."
opening (`ha-addon.md:13-15`) goes.

`README.md:25,32` gets the same four-step flow.
`homeassistant/byonk/DOCS.md` — shown inside Home Assistant, and now the first
thing a user reads — gets it too.

`docs/superpowers/ha-publishing.md` is a live maintainer runbook, not a record —
its stated subject is "getting byonk into the HACS default store". Strip the HACS
store sections and retitle it; keep the brand-images half, which is about the
integration icon and is unaffected by any of this.

The remaining files under `docs/superpowers/` are historical records of past
phases and stay untouched.

### The HACS upgrade note

Deleting `hacs.json` breaks nothing for an existing HACS install. The reason to
remove Byonk from HACS is ownership: HACS would keep offering integration updates
independently of the app, and a user who takes one ends up with an integration
newer than their server. One line at the bottom of the page, droppable after a
release or two. No migration code — the app simply overwrites the directory,
which passes the deletion-safety check because that `manifest.json` does say
`domain: byonk`.

`CHANGES.md` gets a user-facing entry under Unreleased.

## Error handling

| Situation | Behaviour |
|---|---|
| Not running as an app (`addon_mode` false) | Do nothing. Byonk still runs standalone. |
| `/homeassistant` missing or read-only | Log a warning, continue serving. |
| Target exists but is not byonk's | Log a warning, change nothing, continue. |
| Notification or discovery POST fails | Log a warning. The files are already written; the user can restart anyway. |
| Home Assistant down when discovery is posted | Supervisor stores the message; Home Assistant picks it up at next start. |
| App newer than loaded integration | Entities keep working; repair issue points at the restart. |

## Testing

**Rust**, against temp dirs via the two env overrides: fresh write; same-version
no-op; version change rewrites; a foreign `custom_components/byonk` is refused
untouched; unwritable directory logs and startup continues.

**Python**, next to the existing equivalents: hassio discovery flow (new entry,
and abort when one exists) in `tests_ha/test_config_flow.py`; the version-mismatch
issue in a new test beside `tests_ha/test_screen_repo_issues.py`.

**End to end on the QEMU HAOS VM**, via the `ha-vm-testing` skill, built from
source on a clean system: install the app → files appear in
`/homeassistant/custom_components/byonk` → notification shows → restart → the
Discovered card appears → configure it → entities appear. This is the only test
that proves the whole chain, and it is how Task 9 was validated.

## Out of scope

- Removing the integration when the app is uninstalled. Supervisor offers no
  uninstall hook. The stale directory makes the config entry fail to set up,
  which is visible and harmless.
- An option to switch the install off. Nobody has a reason to want it now that
  HACS is gone.
- Any change to the integration → app direction. `async_ensure_addon_installed`
  stays as it is.
