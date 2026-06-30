# Phase 5 — HA-owned devices via discovery

_Design spec. Date: 2026-06-30. Branch: `feat/ha-phase4-release-docs` (Phase 4 work, not yet pushed)._

## Problem

The current HA integration (Phase 3) surfaces TRMNL devices in a way that does not
match a Home Assistant user's mental model:

- **Demo clutter.** byonk's shipped `config.yaml` contains ~15 personal/test device
  mappings (`B4:A9:90:8C:6D:18`, gphoto albums, fontdemo devices, …). That same file is
  embedded into the binary (`src/assets.rs` `#[folder = "."] #[include = "config.yaml"]`)
  and seeded as the add-on's default config, so a **fresh HA install materializes ~8 demo
  devices**, most with empty telemetry.
- **Unintuitive onboarding.** A connected-but-unconfigured ("pending") TRMNL does **not**
  appear as a device. It surfaces as a non-fixable **Repairs issue** plus a count on a hub
  sensor, and is onboarded through an in-integration "Add device" subentry flow with a
  dropdown. Users expect a new device to appear the way a new Apple TV does: a **Discovered
  card** on _Settings → Devices & Services_ with a **Configure** button.

## Guiding principle (from the project owner)

> byonk is a Home Assistant **service provider**; it does **not** own devices of its own.
> A device is known to byonk **only if it is known to HA**. The one special case is a
> global setting for what byonk should display on a **new** (un-onboarded) device's screen.

Home Assistant is the **single source of truth** for which devices exist. byonk persists
per-device screen mappings only as a **write-through cache** of HA's decisions.

## Why the architecture must change

Apple TV gets its Discovered card because **each Apple TV is its own config entry**,
discovered via zeroconf. The "Discovered card" experience is produced **only** by the main
config-flow manager with a discovery source, and those flows create **config entries**.

The current integration represents each device as a **config subentry** of the hub.
A subentry can only be _created_ from a `SOURCE_USER` flow — verified in HA Core:
`ConfigSubentryFlow.async_create_entry` raises `ValueError` if `self.source != SOURCE_USER`
(`homeassistant/config_entries.py:3542`, HA Core as pinned by
`pytest-homeassistant-custom-component==0.13.316`). Subentries are added via the hub's "Add device"
button, never via a discovery card.

Therefore the Discovered-card UX **requires per-device config entries**. Phase 4 is not yet
pushed and no byonk image with the admin API has shipped, so this is the cheapest possible
moment to make the change.

## Architecture

A single **hub** config entry plus **one config entry per TRMNL device**, all in the
`byonk` domain.

- **Hub entry** — unchanged role: zero-touch Supervisor provisioning, add-on connection,
  admin token, the polling **coordinator**, and global settings. Created via the existing
  `async_step_user` / reauth flows.
- **Device entry** — one per TRMNL, `unique_id = MAC`. `entry.data = {device_key, hub_entry_id}`.
  Device entries are thin: they look up the shared hub coordinator and contribute that
  device's entities. Hub vs device entries are distinguished by the presence of
  `device_key` in `entry.data`.

### Coordinator sharing

The hub coordinator polls byonk every 60 s (`/api/admin/{devices,pending,screens,config}`)
and is stored in `hass.data[DOMAIN][hub_entry_id]`. Device entries do **not** create their
own coordinator; in `async_setup_entry` they resolve the hub coordinator via `hub_entry_id`.
If the hub coordinator is not yet available (load-order race), the device entry raises
`ConfigEntryNotReady` and HA retries. When the hub reloads, device entries reload too.

## Device lifecycle & discovery

1. A device contacts byonk. byonk does not recognize it → renders the **new-device screen**
   (registration code) and lists it in `GET /api/admin/pending`.
2. The coordinator sees a pending device with **no** matching device entry and **no**
   in-progress discovery flow → injects
   `hass.config_entries.flow.async_init(DOMAIN, context={"source": SOURCE_INTEGRATION_DISCOVERY}, data={"key": mac, "code": code, "model": …})`.
   HA renders a **Discovered card** with the MAC and registration code.
3. The user clicks **Configure** (`async_step_integration_discovery` → confirm/params step):
   set `unique_id = mac`, `_abort_if_unique_id_configured`, present a screen picker
   (+ optional params / dither / panel). On submit the flow:
   - POSTs the mapping to byonk (`POST /api/admin/devices`), then
   - calls `async_create_entry` to create the **device config entry**.
4. Next poll: the device is `registered` in byonk → its device + telemetry/select entities
   populate (battery, rssi, last_seen, firmware, model, screen/dither/panel).

**Dedup / teardown.** Discovery flows dedupe by `unique_id` (HA flow manager). When a
device leaves the pending list (configured or gone), the coordinator aborts the matching
in-progress discovery flow. Users may **Ignore** a card (HA native `SOURCE_IGNORE`).

## Reconcile: byonk device set must equal HA device-entry set

Run on every coordinator update (replacing the current subentry reconcile):

- **HA entry exists, byonk mapping missing** → write it (`POST /api/admin/devices`).
  Normally written at onboard; this is a safety net.
- **Orphan: byonk mapping with no HA entry** → `DELETE /api/admin/devices/:key`,
  **2-strike debounced** (a transient API blip must not delete a mapping). This enforces
  "known to byonk iff known to HA": the device falls back to the new-device screen and
  re-surfaces as a Discovered card for re-adoption.
- **Both exist** → leave live values alone. HA select entities PATCH byonk directly and read
  back; byonk is the write-through store for screen/dither/panel/params.
- **User removes an HA device entry** → delete the mapping from byonk (`DELETE`), via the
  entry's removal hook.

## byonk side (Rust)

- **Clean embedded default config.** Add a tracked `default-config.yaml` containing
  `registration`, `auth_mode`, `panels`, `screens`, an **empty `devices: {}`**, and **no**
  `default_screen`. Change `src/assets.rs` to embed `default-config.yaml` instead of
  `config.yaml`. The repo's `config.yaml` remains the owner's local dev config (still
  tracked, but no longer the shipped default). A fresh install therefore has **zero**
  devices.
- **New-device screen.** Keep the existing `registration.screen` setting — this is the one
  global "what new devices display" knob. No new Rust setting required.
- **No other Rust behavior change.** The admin API already supports the create/patch/delete
  writes HA needs (incl. the Phase 4 `026dce7` onboarding/upsert fix).

`default_screen` becomes irrelevant in the HA-owned model (every HA-known device has an
explicit screen). It is omitted from the shipped default; the field stays in the struct for
standalone (non-HA) users and is left unused by the integration.

## Hub entities & cleanup

- **Keep / repurpose:**
  - registration on/off **switch** (`registration.enabled`)
  - auth-mode **select** (`auth_mode`)
  - **new-device-screen select** — repurposed from the former "default-screen select" to
    drive `registration.screen` (options = available screens + "built-in default").
- **Remove:**
  - the **Repairs-issue** mechanism for pending devices (`repairs.py` pending issues) — the
    Discovered cards replace it.
  - the **pending-count sensor** on the hub — redundant with the cards.
- **Per-device entities** (battery, rssi, last_seen, firmware, model; screen/dither/panel
  selects) are unchanged in behavior — only re-homed from subentries to device config
  entries. `DeviceInfo` uses `identifiers={(DOMAIN, mac)}`, `via_device=(DOMAIN, hub_entry_id)`.

## Migration

None. No byonk image with the admin API has shipped, so the integration cannot have real
installs yet; the subentry→entry change is a clean break. Testers (the owner's VM) **remove
and re-add the integration**. Documented in the harness/handover notes.

## Testing

**Python (`tests_ha`):**
- Discovery flow: coordinator injects a flow for a pending device → card → Configure →
  device config entry created **and** mapping POSTed to byonk.
- Discovery dedup: no duplicate flow while one is in progress / device already configured.
- Discovery teardown: in-progress flow aborted when device leaves pending.
- Orphan prune: byonk mapping with no HA entry is deleted after 2 strikes, not 1.
- Reconcile: entry-exists-but-byonk-missing write; entry removal deletes byonk mapping.
- Hub settings: registration switch, auth-mode select, new-device-screen select round-trip.
- `ConfigEntryNotReady` when a device entry loads before its hub.

**Rust (`make check`):**
- Embedded default config parses and has empty `devices`.
- Existing admin-API read/write/delete tests stay green.

**Live VM (existing Phase 4 to-do):** revalidate the full discovery → configure → render
path against a from-source byonk on HAOS with real TRMNL hardware; verify a fresh install
shows no demo devices and a real pending device produces a Discovered card.

## Out of scope

- Phase 4b–d: add-on `version:` automation, HACS default-list + `home-assistant/brands`
  prep, docs polish — unchanged, still pending.
- HA-worded registration screen asset (deferred Fix C). The §"byonk side" new-device-screen
  setting makes pointing `registration.screen` at such an asset trivial later.
