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

## SSH Access (scriptable add-on rebuilds)

For iterating on the byonk **server** (Rust), SSH into the VM so rebuilds can be
triggered from the host instead of clicking through the UI. Host port `${SSH_PORT}`
(default `2222`) is forwarded to guest `22`.

One-time setup (needs the UI once):

1. Boot the VM (`make ha-vm`) — the SSH port forward is already in `run.sh`.
2. In the UI, install the official **Terminal & SSH** add-on (Add-on Store → *Terminal & SSH*).
3. In its **Configuration**:
   - **Authorized Keys**: add the public key `tools/ha-vm/ssh/id_ed25519.pub`
     (generate the pair once with `ssh-keygen -t ed25519 -f tools/ha-vm/ssh/id_ed25519 -N ""`).
   - **Network → SSH Port**: set to `22` (it defaults to Ingress-only/disabled).
4. Start the add-on (enable *Start on boot*).

Then, from the host:

```bash
make ha-ssh                                  # interactive shell in the VM
make ha-ssh CMD="ha addons info local_byonk" # run a single command
```

Deploy a server change and rebuild the add-on in one step (needs SMB creds too):

```bash
SMB_USER=byonk SMB_PASS=byonk make ha-rebuild
```

`ha-rebuild` rsyncs the build inputs into the local add-on source over Samba, syncs
`screens/` into the add-on's `SCREENS_DIR`, then runs `ha addons rebuild local_byonk`
over SSH. The private key lives in `tools/ha-vm/ssh/` (gitignored).

## Networking Limitation

The harness uses **user-mode NAT**. This exposes the forwarded ports (8123, 3000, 4445, 2222) to the Mac host only. Real LAN TRMNL e-ink devices **cannot** reach the VM. Bridged networking is out of scope for Phase 4.

## Reset

To reset the VM environment:

```bash
make ha-vm-clean
```

This deletes the downloaded HAOS image, VM disk, and varstore. You can then `make ha-vm` again for a fresh install.

## Validation Checklist

The integration is ready for release when all of the following pass:

- [ ] **Add-on store**: Add the `https://github.com/oetiker/byonk` repository to Add-on stores → the *Byonk* add-on shows up, installs (pulls the published `ghcr.io/oetiker/byonk` image), and starts → Port 3000 serves a screen
- [ ] **Integration discovery**: `custom_components/byonk` deployed → Restart Home Assistant → *Byonk* is discoverable in "Add Integration"
- [ ] **Zero-touch trust** (add-on NOT pre-installed): Adding the integration auto-adds the repo, installs and starts the add-on, provisions the admin token into the add-on options, and reads it back; the config entry stores no token
- [ ] **Add-on-owned global config**: `auth_mode`, `package_refresh_interval`, and the `packages` registry are edited on the **add-on Options tab** and applied on restart; the integration presents them read-only/monitoring; an admin-API write to any of them returns **409** pointing back to the Options tab
- [ ] **Reserved `DEFAULT` device**: `GET /api/admin/devices` includes a `{"key":"DEFAULT","reserved":true,…}` entry; the integration auto-provisions a **"Byonk Default"** device with a live **Screen select** (no dither/panel), exempt from reconcile/orphan-prune. `PATCH /api/admin/devices/DEFAULT {"screen":…}` → 200 live (no restart); `DELETE /api/admin/devices/DEFAULT` → **409**. Deleting the HA "Byonk Default" device does not lose `devices.DEFAULT`, and HA re-provisions the entry on the next refresh (~60s)
- [ ] **Screen resolution**: an unregistered device shows its pairing **code** (the `byonk-builtin/default` screen is registration-aware); a registered-but-unassigned device shows the `DEFAULT` device's screen
- [ ] **HA-owned per-device flow**: a pending device raises the onboarding path; adding it creates a per-device HA config entry (keyed by MAC) with a **Discovered** card; its screen/param/dither/panel entities write through live to the admin API and back; per-screen `@params` render as HA selectors
- [ ] **Screen packages**: a configured package `handle`/`repo`/`pin` is fetched; its screens are selectable per device; the integration's **Update packages** button triggers a live refresh
- [ ] **Re-authentication**: Blanking/invalidating the add-on token raises *Re-authentication required* and resolving re-provisions without manual input; a transient connection error does NOT trigger a reauth loop
- [ ] **Removal grace**: A device that disappears survives the documented grace window before its HA entry is pruned
