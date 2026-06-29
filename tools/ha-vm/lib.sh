#!/usr/bin/env bash
# Shared paths + tool resolution for the byonk HAOS test harness.
set -euo pipefail

HA_VM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HA_VM_DIR/../.." && pwd)"
WORK_DIR="${BYONK_HA_VM_WORK:-$HA_VM_DIR/work}"
DISK="$WORK_DIR/haos.qcow2"
VARS="$WORK_DIR/vars.fd"
HAOS_VERSION_FALLBACK="15.2"

haos_version() {
  if [ -n "${HAOS_VERSION:-}" ]; then echo "$HAOS_VERSION"; return; fi
  local tag
  tag="$(curl -fsSL https://api.github.com/repos/home-assistant/operating-system/releases/latest \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1 || true)"
  echo "${tag:-$HAOS_VERSION_FALLBACK}"
}

resolve_tools() {
  local qprefix qbin efi
  qbin=""
  if command -v qemu-system-aarch64 >/dev/null 2>&1; then
    qbin="$(command -v qemu-system-aarch64)"
  elif qprefix="$(brew --prefix qemu 2>/dev/null)" && [ -x "$qprefix/bin/qemu-system-aarch64" ]; then
    qbin="$qprefix/bin/qemu-system-aarch64"
  elif [ -x /Applications/UTM.app/Contents/Resources/qemu/qemu-system-aarch64 ]; then
    qbin="/Applications/UTM.app/Contents/Resources/qemu/qemu-system-aarch64"
  fi
  [ -n "$qbin" ] || { echo "ERROR: qemu-system-aarch64 not found (try: brew install qemu)" >&2; exit 1; }
  QEMU="$qbin"

  efi=""
  for cand in \
    "$(brew --prefix qemu 2>/dev/null)/share/qemu/edk2-aarch64-code.fd" \
    "/Applications/UTM.app/Contents/Resources/qemu/edk2-aarch64-code.fd"; do
    [ -f "$cand" ] && { efi="$cand"; break; }
  done
  [ -n "$efi" ] || { echo "ERROR: edk2-aarch64-code.fd firmware not found" >&2; exit 1; }
  EFI_CODE="$efi"
  export QEMU EFI_CODE
}
