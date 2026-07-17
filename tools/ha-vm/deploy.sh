#!/usr/bin/env bash
# rsync the byonk integration into the running VM's /config via the Samba add-on.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

: "${SMB_USER:?set SMB_USER to the Samba add-on username}"
: "${SMB_PASS:?set SMB_PASS to the Samba add-on password}"
SMB_PORT="${SMB_PORT:-4445}"

mnt="$(mktemp -d)"
cleanup() { umount "$mnt" 2>/dev/null || true; rmdir "$mnt" 2>/dev/null || true; }
trap cleanup EXIT

mount -t smbfs "//${SMB_USER}:${SMB_PASS}@localhost:${SMB_PORT}/config" "$mnt"
mkdir -p "$mnt/custom_components/byonk"
# The Samba add-on vetoes files matching "._*" (its default Veto Files list).
# rsync's default temp files are named ".<basename>.<rand>" — for "__init__.py"
# that is ".__init__.py.XXXX", which matches "._*" and is rejected (Permission
# denied). Use --inplace --whole-file so rsync writes directly to the final
# names with no dot-temp files. SMB can't honour perms/owner/times, so drop them.
rsync -rlD --inplace --whole-file --no-perms --no-owner --no-group --no-times --delete \
  --exclude '__pycache__' --exclude '*.pyc' \
  "$REPO_ROOT/custom_components/byonk/" "$mnt/custom_components/byonk/"
echo "Deployed. Restart HA (Developer Tools → Restart, or 'ha core restart' on the serial console)."
