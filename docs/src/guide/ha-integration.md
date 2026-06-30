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
| Auth mode | Select | Authentication mode for device requests |
| Default screen | Select | Fallback screen shown to unregistered devices |
| Pending devices | Sensor | Count of devices waiting to be onboarded |

### Per-device entities (one device per TRMNL)

| Entity | Type | Description |
|--------|------|-------------|
| Battery | Sensor | Battery voltage (V) |
| Signal | Sensor | Wi-Fi RSSI (dBm) |
| Last seen | Sensor | Timestamp of last check-in |
| Firmware | Sensor | Firmware version string |
| Model | Sensor | Panel model identifier |
| Screen | Select | Active screen assigned to this device |
| Dither | Select | Dither algorithm override |
| Panel | Select | Panel profile override |

## Onboarding a New Device

When a TRMNL device boots for the first time it displays a **registration code** on
its e-ink screen.  Byonk records the device as pending.

1. A **Repairs** issue titled *Pending Byonk device* appears in Home Assistant to alert
   you (it is informational — there is no **Fix** button).
2. Open the Byonk integration and use the **Add device** action.
3. In the form, choose the registration code that matches the code shown on the device
   and select the screen you want displayed on it.
4. Optionally set a dither algorithm and panel profile.
5. Submit — the device is now registered and will start fetching its assigned screen.

> **Note:** Byonk tracks connected-but-unregistered devices in memory, so the
> pending list clears if the Byonk add-on restarts. A device reappears as pending
> the next time it checks in (TRMNL devices poll on their refresh interval).

## Editing Device Settings

To change the screen or parameters for an already-registered device, open the Byonk
integration, locate the device subentry, and click **Configure**.  The reconfigure
form lets you update screen parameters without removing and re-adding the device.

## Re-authentication

If the admin token stored in the add-on options becomes invalid (for example after
reinstalling the add-on), Home Assistant will raise a *Re-authentication required*
notification.  Click **Re-authenticate**, and the integration will read or
re-provision the token automatically — no manual input is needed.
