#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="$here/testdata/integration"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
cp "$data/manifest.input.json" "$tmp/manifest.json"
"$here/bump-integration-version.sh" "0.16.0" "$tmp/manifest.json"
if diff -u "$data/manifest.expected.json" "$tmp/manifest.json"; then
  echo "OK: bump-integration-version tests passed"
else
  echo "FAIL: manifest.json mismatch"; exit 1
fi
