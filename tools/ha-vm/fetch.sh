#!/usr/bin/env bash
# Download + decompress the HAOS generic-aarch64 image into the work dir.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
resolve_tools
mkdir -p "$WORK_DIR"

if [ -f "$DISK" ]; then
  echo "Disk already present: $DISK (delete it or run 'make ha-vm-clean' to re-fetch)"
  exit 0
fi

VER="$(haos_version)"
ASSET="haos_generic-aarch64-${VER}.qcow2.xz"
URL="https://github.com/home-assistant/operating-system/releases/download/${VER}/${ASSET}"
echo "Fetching HAOS ${VER} from ${URL}"
curl -fL --retry 3 -o "$WORK_DIR/$ASSET" "$URL"
echo "Decompressing…"
xz -dc "$WORK_DIR/$ASSET" > "$DISK"
rm -f "$WORK_DIR/$ASSET"
echo "Ready: $DISK"
