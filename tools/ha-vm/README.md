# Home Assistant VM Harness

This directory contains a QEMU-based test harness for the Byonk Home Assistant integration. It automates HAOS image fetching, VM boot, and integration deployment.

## Prerequisites

- **QEMU**: Install via `brew install qemu`
- **Bash 4+**: Required for the harness scripts
- **GitHub API access**: Used to resolve the latest HAOS release automatically

## Boot

Start the VM with:

```bash
make ha-vm
```

The first boot pulls the Home Assistant Supervisor and Core containers and takes several minutes to complete. Once ready, access the Home Assistant UI at:

```
http://localhost:8123
```

The HAOS image is **`generic-aarch64`**. The harness automatically fetches the latest release from the GitHub API. To override the version:

```bash
HAOS_VERSION=17.12 make ha-vm
```

The VM runs headless on the serial console. To quit QEMU, press `Ctrl-A` followed by `X`, or run `make ha-vm-stop` from another terminal.

### Port Forwarding

The harness forwards host ports to the guest. These are configurable via environment variables:

| Service | Guest Port | Host Default | Env Variable |
|---------|-----------|--------------|--------------|
| Home Assistant UI | 8123 | 8123 | `HA_PORT` |
| Byonk Server | 3000 | 3000 | `BYONK_PORT` |
| Samba Share | 445 | 4445 | `SMB_PORT` |

If you already run a local Byonk dev server on host port 3000, start the VM with a remapped port:

```bash
BYONK_PORT=13000 make ha-vm
```

Then reach the VM's Byonk server at `http://localhost:13000`.

## One-time HA Setup

After the UI is available at `http://localhost:8123`:

1. **Complete onboarding**: Create the owner account and configure Home Assistant
2. **Install Samba share**: From the Add-on Store, install the official "Samba share" add-on
3. **Configure credentials**: Set username and password (e.g., `byonk` / `byonk`)
4. **Enable and start**: Toggle the add-on to enable it and click "Start"

The harness forwards host port `${SMB_PORT}` (default `4445`) to guest port `445` for Samba access.

## Deploy the Integration

On the Mac host, deploy the Byonk integration with:

```bash
SMB_USER=byonk SMB_PASS=byonk make ha-deploy
```

If you remapped the SMB port, include it:

```bash
SMB_USER=byonk SMB_PASS=byonk SMB_PORT=4445 make ha-deploy
```

After deployment, restart Home Assistant via the UI (**Developer Tools** → **Restart**) or the serial console:

```bash
ha core restart
```

## Networking Limitation

The harness uses **user-mode NAT**. This exposes the forwarded ports (8123, 3000, 4445) to the Mac host only. Real LAN TRMNL e-ink devices **cannot** reach the VM. Bridged networking is out of scope for Phase 4.

## Reset

To reset the VM environment:

```bash
make ha-vm-clean
```

This deletes the downloaded HAOS image, VM disk, and varstore. You can then `make ha-vm` again for a fresh install.

## Validation Checklist

The integration is ready for release when all of the following pass:

- [ ] **Add-on store**: Add the `https://github.com/oetiker/byonk` repository to Add-on stores → Install and start the *Byonk* add-on → Port 3000 serves a screen
- [ ] **Integration install**: Run `make ha-deploy` → Restart Home Assistant → *Byonk* is discoverable in "Add Integration"
- [ ] **Zero-touch trust** (add-on NOT pre-installed): Adding the integration auto-installs and starts the add-on, provisions the admin token into the add-on options, reads it back; integration entry stores no token; *Byonk Server* hub device appears
- [ ] **Hub entities**: Registration switch, auth-mode select, default-screen select, and pending-devices sensor are present and reflect `config.yaml`
- [ ] **Onboarding**: Pending device → *Pending Byonk device* repairs issue → **Add device** lists the pairing code → Device registers by MAC → Per-TRMNL device created with documented sensors and selects
- [ ] **Subentry mirror and edit**: Changing screen and parameters via **Configure** writes through to `config.yaml` and back to entities; `@params` render as selectors
- [ ] **Re-authentication**: Blank or invalid token raises *Re-authentication required*, resolved by re-provisioning; transient connection errors do NOT loop indefinitely
- [ ] **Removal grace**: A disappeared device survives one poll cycle before its subentry is removed
