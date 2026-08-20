---
name: ha-vm-testing
description: Use when testing byonk end-to-end against the local QEMU Home Assistant OS VM — booting the VM, rebuilding the byonk add-on from source, deploying the custom integration, or reaching the VM over SSH or Samba.
---

# Home Assistant VM test setup

A local QEMU HAOS VM (`tools/ha-vm/`, see its `README.md`) runs both byonk and the
integration for end-to-end testing. It boots headless via `make ha-vm`; user-mode NAT
forwards host ports **8123** (HA UI), **3000** (byonk), **4445** (Samba), **2222** (SSH).

- **byonk server** runs as a *local add-on* built from source (`addons/byonk/` inside the
  VM, its own `Dockerfile`). The add-on reads screens/fonts from `SCREENS_DIR=/config/screens`
  (the `addon_configs/local_byonk/` Samba share) at runtime, and embeds `default-config.yaml`
  at compile time — so **screen file edits are hot** but **Rust changes need an add-on rebuild**.
- **Integration** (`custom_components/byonk/`): `custom_components` is now one of the build
  inputs `make ha-rebuild` syncs, and an app that carries the integration installs it at
  startup, so for normal testing you just restart the app and reload the integration in the
  HA UI. But the VM's add-on `Dockerfile` lives only inside the VM (not in this repo) and
  needs its own `COPY --from=builder /build/custom_components/byonk /app/custom_components/byonk`
  line in its runtime stage — without it, syncing the source changes nothing, and `install`
  silently fails on every start with "no readable manifest.json". `make ha-deploy` writes
  `custom_components/byonk/` straight into the VM's HA config over Samba instead — still
  useful for iterating on Python without an add-on rebuild, but the app will overwrite that
  hand-deployed copy the next time it starts.
- **SSH** (one-time: install the Terminal & SSH add-on with `tools/ha-vm/ssh/id_ed25519.pub`):
  - `make ha-ssh` — shell in the VM; `make ha-ssh CMD="ha addons info local_byonk"` — one command.
  - `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` — sync server source + rebuild the add-on.
- **Samba shares** (creds `byonk`/`byonk`, port 4445): `addons` (add-on source),
  `addon_configs` (running add-on's config + `screens/`), `config` (HA config).

Never commit `tools/ha-vm/ssh/` (gitignored) or read the admin token — verify through the HA UI.
