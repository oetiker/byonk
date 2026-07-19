# Home Assistant Integration

The Byonk Home Assistant integration connects Home Assistant to a Byonk server running
as a Supervisor app. It manages the app lifecycle, provisions authentication
automatically, and exposes Byonk devices as Home Assistant entities.

Byonk's server-**global** configuration — `auth_mode`, `screen_repo_refresh_interval`,
and the screen repo registry — is edited in the
**[Byonk app's Configuration tab](ha-addon.md)**, not here. This integration is
read-only monitoring for that global config (per-screen-repo status sensors), plus two
live operational controls (a registration switch and an "Update screen repos" button)
and full read/write control over **per-device** screen/dither/panel/parameter
assignment.

## Requirements

- Home Assistant Supervised or HAOS (the integration controls the Byonk app via
  the Supervisor API and will not work on plain Home Assistant Core or Container).
- The [Byonk app](ha-addon.md) does **not** need to be pre-installed — the
  integration installs and starts it automatically.

## Installation via HACS

1. In Home Assistant, open **HACS → Integrations**.
2. Click the three-dot menu (top right) and choose **Custom repositories**.
3. Add `https://github.com/oetiker/byonk` as an **Integration** repository.
4. Search for *Byonk* in HACS and install it.
5. Restart Home Assistant.

> **Note:** Once byonk is accepted into the HACS default store, you'll be able to
> find it directly by searching *Byonk* — until then, use the custom-repository
> step above.

## Adding the Integration

1. Go to **Settings → Devices & Services → Add Integration** and search for *Byonk*.
2. The integration will:
   - Add the Byonk app repository to Supervisor.
   - Install and start the Byonk app.
   - Generate and store an admin token automatically (zero-touch — you never enter a
     token).
3. A *Byonk Server* hub device appears once setup completes.

## Entities

### Hub device (Byonk Server)

| Entity | Type | Description |
|--------|------|-------------|
| Registration enabled | Switch | Allow new TRMNL devices to register |
| Update screen repos | Button | Trigger an immediate refresh of all screen repos (see below) |
| *Screen repo status* (one per screen repo) | Sensor | Diagnostic sensor per non-builtin screen repo — see *Monitoring screen repos* below |

The remaining server-global settings — `auth_mode` and `screen_repo_refresh_interval` —
are **not** exposed as entities here; they're edited in the
[Byonk app's Configuration tab](ha-addon.md) (changes apply on app restart).

### Byonk Default device

Alongside the hub, the integration automatically creates a **Byonk Default**
device — no setup step needed. Its single **Screen** select entity sets the
screen assigned to byonk's reserved `devices.DEFAULT` entry: the screen shown by
every un-onboarded device (with its pairing code) and by any registered device
with no screen of its own. Changes apply live, no restart required.

### Per-device entities (one device per TRMNL)

| Entity | Type | Description |
|--------|------|-------------|
| Battery voltage | Sensor | Battery voltage (V) |
| Signal strength | Sensor | Wi-Fi RSSI (dBm) |
| Last seen | Sensor | Timestamp of last check-in |
| Firmware version | Sensor | Firmware version string |
| Model | Sensor | Verbatim `Model` header reported by the device |
| Screen | Select | Active screen assigned to this device |
| Dither | Select | Dither algorithm override |
| Panel | Select | Panel profile override |
| Refresh interval | Number | Per-device refresh interval in seconds (`0` = no override). Precedence: screen's Lua `refresh_rate` > this override > screen's static default |
| *Screen parameter* (one per param) | Text / Number / Switch / Select | Each parameter declared in the current screen's parameter schema (the `params` block in its `meta.yaml`) appears as its own entity in the **Controls** card (type mapped from the schema: string→Text, int/float→Number, bool→Switch, enum→Select). Changes apply instantly. The set of entities updates automatically when you assign a different screen to the device. |

## Onboarding a New Device

Byonk ships with no devices configured — Home Assistant is the source of truth.
When a TRMNL device boots for the first time, it contacts Byonk and displays a
**registration code** on its e-ink screen while waiting to be claimed.

A **Discovered** card for the new device appears automatically in
**Settings → Devices & Services**.

1. Click **Configure** on the Discovered card.
2. In the *Set up TRMNL device* form, choose the screen you want displayed on the
   device. Optionally set a dither algorithm and panel type.
3. If the chosen screen declares parameters (via the `params` schema in its `meta.yaml`), a second form
   appears to fill in those values.
4. Submit — the device is now an HA device with its own config entry, and its
   screen mapping is written to Byonk. The device starts fetching its assigned screen
   on the next refresh.

> **Note:** What an un-onboarded (or registered-but-unassigned) device displays on
> its e-ink panel is controlled by the **Byonk Default** device's Screen select
> (see [Entities](#entities) above) — change it there any time, live, no restart
> needed.

Removing an HA device (via **Settings → Devices & Services → Delete**) removes its
mapping from Byonk. Byonk mappings that have no corresponding HA device are pruned
automatically.

## Editing Device Settings

To change the screen for an already-onboarded device, use the **Screen** select
entity on the device card.  To adjust dither algorithm or panel type, use the
**Dither** or **Panel** select entities.

To update the per-screen parameters, use the live entities in the device's **Controls**
card — each parameter of the current screen appears as its own Text, Number, Switch, or
Select entity and applies instantly.  The set of parameter entities updates automatically
when you change the device's screen.

**Device naming**: the device's name is owned by Home Assistant. Rename the device
the usual way (device card → pencil icon) and byonk will mirror the name automatically
when you rename the device in Home Assistant. No changes are needed in byonk's config directly.

## Monitoring Screen Repos

Screen repos (see [Screen Repos Section](configuration.md#screen-repos-section) in
the Configuration guide) are **added, edited, and removed in the
[Byonk app's Configuration tab](ha-addon.md)** — not here. This integration
gives you read-only monitoring and one operational control:

Each screen repo gets a diagnostic **status sensor** (e.g.
`sensor.byonk_disttest_status`) on the *Byonk Server* hub device, whose state is
the fetch status (`fetching`, `ready`, `error`, ...) and whose attributes include
the resolved commit (`resolved_sha`), `last_fetched` time, `repo`, `pin`, and any
`error`.

When a screen repo fails to fetch, the integration raises a Home Assistant
**Repair** issue (**Settings → System → Repairs**) carrying the fetch error, so a
broken screen repo surfaces visibly rather than only in the status sensor's
attributes. The issue clears automatically once the screen repo fetches
successfully again.

Press the hub device's **Update screen repos** button to trigger an immediate
content refresh of every already-configured screen repo (a git pull on the existing
pin — equivalent to waiting for the `screen_repo_refresh_interval` set in the app's
Configuration tab); the status sensors update once the fetch completes. This
button does not add, remove, or repin screen repos — only the app's Configuration
tab does that.

## Re-authentication

If the admin token stored in the app options becomes invalid (for example after
reinstalling the app), Home Assistant will raise a *Re-authentication required*
notification.  Click **Re-authenticate**, and the integration will read or
re-provision the token automatically — no manual input is needed.
