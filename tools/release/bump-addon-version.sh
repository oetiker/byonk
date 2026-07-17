#!/usr/bin/env bash
# Bumps the HA add-on version to match a published ghcr.io/oetiker/byonk tag.
# Usage: bump-addon-version.sh <version> <release_type> [addon_dir]
set -euo pipefail

version="${1:?usage: bump-addon-version.sh <version> <release_type> [addon_dir]}"
release_type="${2:?missing release_type (bugfix|feature|major)}"
addon_dir="${3:-homeassistant/byonk}"

cfg="$addon_dir/config.yaml"
log="$addon_dir/CHANGELOG.md"

# 1. Rewrite the top-level `version:` line (column 0 anchor — never touches
#    breaking_versions entries, which are indented list items).
perl -i -pe 's/^version:.*/version: "'"$version"'"/' "$cfg"

# 2. Prepend a CHANGELOG section right after the `# Changelog` header.
perl -i -0777 -pe 's/(# Changelog\n)/$1\n## '"$version"'\n\n- Update to byonk '"$version"' (see the main CHANGES.md for details).\n/' "$log"

# 3. On a major release, record the breaking version so Supervisor warns.
if [ "$release_type" = "major" ]; then
  if grep -qE '^breaking_versions:' "$cfg"; then
    # Append under the existing key.
    perl -i -pe 's/^(breaking_versions:.*)/$1\n  - "'"$version"'"/ if !$done && /^breaking_versions:/ && ($done=1)' "$cfg"
  else
    printf 'breaking_versions:\n  - "%s"\n' "$version" >> "$cfg"
  fi
fi
