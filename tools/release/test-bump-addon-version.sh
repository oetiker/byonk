#!/usr/bin/env bash
# Unit test for bump-addon-version.sh — runs it against fixtures in a temp dir
# and diffs the result against golden files. No live release.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="$here/testdata/addon"
fail=0

run_case() {
  local name="$1" version="$2" rtype="$3" expect_cfg="$4" expect_log="$5"
  local tmp; tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  cp "$data/config.input.yaml" "$tmp/config.yaml"
  cp "$data/changelog.input.md" "$tmp/CHANGELOG.md"

  "$here/bump-addon-version.sh" "$version" "$rtype" "$tmp"

  if ! diff -u "$data/$expect_cfg" "$tmp/config.yaml"; then
    echo "FAIL [$name]: config.yaml mismatch"; fail=1
  fi
  if [ -n "$expect_log" ] && ! diff -u "$data/$expect_log" "$tmp/CHANGELOG.md"; then
    echo "FAIL [$name]: CHANGELOG.md mismatch"; fail=1
  fi
}

run_case "feature" "0.16.0" "feature" "config.feature.expected.yaml" "changelog.expected.md"
run_case "major"   "1.0.0"  "major"   "config.major.expected.yaml"   ""

if [ "$fail" -eq 0 ]; then echo "OK: bump-addon-version tests passed"; else exit 1; fi
