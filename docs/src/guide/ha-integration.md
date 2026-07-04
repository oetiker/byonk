# Home Assistant Integration

The Byonk Home Assistant integration connects Home Assistant to a Byonk server running
as a Supervisor add-on. It manages the add-on lifecycle, provisions authentication
automatically, and exposes Byonk devices and settings as Home Assistant entities.

## Requirements

- Home Assistant Supervised or HAOS (the integration controls the Byonk add-on via
  the Supervisor API and will not work on plain Home Assistant Core or Container).
- The [Byonk add-on](ha-addon.md) does **not** need to be pre-installed — the
  integration installs and starts it automatically.

## Installation via HACS

1. In Home Assistant, open **HACS → Integrations**.
2. Click the three-dot menu (top right) and choose **Custom repositories**.
3. Add `https://github.com/oetiker/byonk` as an **Integration** repository.
4. Search for *Byonk* in HACS and install it.
5. Restart Home Assistant.

## Adding the Integration

1. Go to **Settings → Devices & Services → Add Integration** and search for *Byonk*.
2. The integration will:
   - Add the Byonk add-on repository to Supervisor.
   - Install and start the Byonk add-on.
   - Generate and store an admin token automatically (zero-touch — you never enter a
     token).
3. A *Byonk Server* hub device appears once setup completes.

## Entities

### Hub device (Byonk Server)

| Entity | Type | Description |
|--------|------|-------------|
| Registration enabled | Switch | Allow new TRMNL devices to register |
| Update packages | Button | Trigger an immediate refresh of all screen packages (see below) |
| *Package status* (one per package) | Sensor | Diagnostic sensor per non-builtin screen package — see *Managing screen packages* below |

Server-wide settings — new-device screen, auth mode, and package refresh
interval — live in the **Configure** (⚙) dialog on the *Byonk Server* hub
device, not as separate entities. Click **Configure** on the device card to
change them.

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

> **Note:** The **new-device screen** setting in the *Byonk Server* hub device's
> **Configure** dialog controls what an un-onboarded device displays on its e-ink
> panel while waiting to be configured in Home Assistant.

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

## Managing Screen Packages

Screen packages (see [Packages Section](configuration.md#packages-section) in
the Configuration guide) can be added, edited, and removed entirely from Home
Assistant, as subentries of the *Byonk Server* hub device.

**Add a package**: on the *Byonk Server* device card, click **Add package** (or
**Settings → Devices & Services → Byonk → Add sub-entry**) and fill in:

- **Handle** — the short name the package is referenced by (e.g. `disttest`).
- **Repository URL** — the git URL to fetch the package from.
- **Pin (branch, tag or sha)** — the branch, tag, or commit SHA to track
  (defaults to `main`).
- **Access token (optional)** — an optional git credential for private repos.

The **access token field is write-only**: it is sent to byonk and never stored
in, or readable back from, Home Assistant. To rotate a token, use **Edit
package** on the package subentry and enter a new one; leaving it blank keeps
the existing token unchanged.

Each package gets a diagnostic **status sensor** (e.g.
`sensor.byonk_disttest_status`) whose state is the fetch status (`fetching`,
`ready`, `error`, ...) and whose attributes include the resolved commit
(`resolved_sha`), `last_fetched` time, `repo`, `pin`, and any `error`.

Press the hub device's **Update packages** button to trigger an immediate
refresh of every package (equivalent to waiting for the package refresh
interval set in **Configure**); the status sensors update once the fetch
completes.

**Deleting a package**: removing a package's subentry deletes it from byonk.
However, **a package still referenced by a device's screen cannot be
deleted** — reassign that device to a different screen first. If you delete a
still-referenced package anyway, byonk rejects the deletion and the subentry
**reappears** on the next refresh (a warning is logged); reassign the device,
then delete again.

## Re-authentication

If the admin token stored in the add-on options becomes invalid (for example after
reinstalling the add-on), Home Assistant will raise a *Re-authentication required*
notification.  Click **Re-authenticate**, and the integration will read or
re-provision the token automatically — no manual input is needed.
