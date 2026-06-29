#!/usr/bin/env bash
# Boot the byonk HAOS test VM headless. Ctrl-A X (then Enter) to quit the serial console.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
resolve_tools

[ -f "$DISK" ] || bash "$HA_VM_DIR/fetch.sh"

# Prepare a writable UEFI varstore matching the firmware size.
if [ ! -f "$VARS" ]; then
  vtmpl="$(brew --prefix qemu 2>/dev/null)/share/qemu/edk2-arm-vars.fd"
  if [ -f "$vtmpl" ]; then
    cp "$vtmpl" "$VARS"
  else
    truncate -s "$(stat -f%z "$EFI_CODE")" "$VARS"
  fi
fi

echo "HA UI:  http://localhost:8123   (first boot takes several minutes)"
echo "byonk:  http://localhost:3000"
exec "$QEMU" \
  -name byonk-haos \
  -M virt,accel=hvf,highmem=on \
  -cpu host -smp "${VM_CPUS:-4}" -m "${VM_RAM_MB:-4096}" \
  -drive "if=pflash,format=raw,readonly=on,file=$EFI_CODE" \
  -drive "if=pflash,format=raw,file=$VARS" \
  -drive "if=virtio,format=qcow2,file=$DISK" \
  -netdev user,id=net0,hostfwd=tcp::8123-:8123,hostfwd=tcp::3000-:3000,hostfwd=tcp::4445-:445 \
  -device virtio-net-pci,netdev=net0 \
  -device virtio-rng-pci \
  -display none -serial mon:stdio
