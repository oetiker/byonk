#!/usr/bin/env bash
# Bumps the HA integration manifest version to the byonk release version.
# Usage: bump-integration-version.sh <version> [manifest_path]
set -euo pipefail

version="${1:?usage: bump-integration-version.sh <version> [manifest_path]}"
manifest="${2:-custom_components/byonk/manifest.json}"

# Rewrite only the "version" string value; leave all other keys/formatting intact.
perl -i -pe 's/("version":\s*")[^"]*(")/${1}'"$version"'${2}/' "$manifest"
