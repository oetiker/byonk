#!/usr/bin/env bash
# SSH into the HAOS test VM (via the Terminal & SSH add-on on guest :22).
#
# With no arguments, opens an interactive shell. With arguments, runs them as a
# remote command, e.g.:
#   bash tools/ha-vm/ssh.sh ha addons rebuild local_byonk
#
# Requires: the Terminal & SSH add-on installed and started with the public key
# from tools/ha-vm/ssh/id_ed25519.pub in its `authorized_keys`, and the VM booted
# with the SSH port forward (added to run.sh). SSH_PORT overrides the host port.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

SSH_PORT="${SSH_PORT:-2222}"
KEY="$HA_VM_DIR/ssh/id_ed25519"

[ -f "$KEY" ] || { echo "ERROR: $KEY missing — generate it with ssh-keygen (see README)" >&2; exit 1; }

# The VM host key changes on rebuild/clean, so don't persist or verify it — this
# is a local, host-only NAT forward to a disposable test VM.
exec ssh -i "$KEY" \
  -o StrictHostKeyChecking=no \
  -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR \
  -p "$SSH_PORT" root@localhost "$@"
