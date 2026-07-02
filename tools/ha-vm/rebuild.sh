#!/usr/bin/env bash
# Deploy a byonk *server* change into the running test VM and rebuild the add-on.
#
#   1. rsync the build inputs (src/crates/fonts/screens/etc.) into the local
#      add-on source dir (addons/byonk) over Samba — same transport as deploy.sh.
#   2. sync screens/ into the add-on's SCREENS_DIR (addon_configs/local_byonk/screens)
#      so runtime screen files match too (hot; no rebuild needed for these).
#   3. trigger `ha addons rebuild local_byonk` over SSH and wait for it to return.
#
# Needs SMB_USER/SMB_PASS (Samba add-on) and the SSH setup (see ssh.sh / README).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

: "${SMB_USER:?set SMB_USER to the Samba add-on username}"
: "${SMB_PASS:?set SMB_PASS to the Samba add-on password}"
SMB_PORT="${SMB_PORT:-4445}"
ADDON_SLUG="${ADDON_SLUG:-local_byonk}"

mnt_addons="$(mktemp -d)"; mnt_cfg="$(mktemp -d)"
cleanup() {
  umount "$mnt_addons" 2>/dev/null || true; rmdir "$mnt_addons" 2>/dev/null || true
  umount "$mnt_cfg" 2>/dev/null || true; rmdir "$mnt_cfg" 2>/dev/null || true
}
trap cleanup EXIT

# SMB-safe rsync flags (see deploy.sh for why): write in place, no dot-temp files,
# and drop perms/owner/times which SMB can't honour.
rs=(rsync -rlD --inplace --whole-file --no-perms --no-owner --no-group --no-times --delete
    --exclude '__pycache__' --exclude '*.pyc' --exclude '.DS_Store' --exclude 'target')

echo "==> syncing add-on build source"
mount -t smbfs "//${SMB_USER}:${SMB_PASS}@localhost:${SMB_PORT}/addons" "$mnt_addons"
dst="$mnt_addons/byonk"
mkdir -p "$dst"
for item in src crates fonts screens static Cargo.toml Cargo.lock default-config.yaml; do
  [ -e "$REPO_ROOT/$item" ] || continue
  if [ -d "$REPO_ROOT/$item" ]; then
    mkdir -p "$dst/$item"
    "${rs[@]}" "$REPO_ROOT/$item/" "$dst/$item/"
  else
    "${rs[@]}" "$REPO_ROOT/$item" "$dst/"
  fi
done

echo "==> syncing runtime screens (SCREENS_DIR)"
mount -t smbfs "//${SMB_USER}:${SMB_PASS}@localhost:${SMB_PORT}/addon_configs" "$mnt_cfg"
if [ -d "$mnt_cfg/$ADDON_SLUG/screens" ]; then
  "${rs[@]}" "$REPO_ROOT/screens/" "$mnt_cfg/$ADDON_SLUG/screens/"
fi

echo "==> rebuilding add-on ($ADDON_SLUG) over SSH — this compiles Rust, give it a few minutes"
bash "$HA_VM_DIR/ssh.sh" ha addons rebuild "$ADDON_SLUG"
echo "==> done. Reload the Byonk integration if you changed the screen/param API."
