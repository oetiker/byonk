#!/usr/bin/env bash
# Points every declared byonk engine requirement at the release's series.
# Usage: bump-screen-engine.sh <version> [root]
#
# A screen declares `byonk: "0.17"`, which the engine reads as `^0.17` -- that
# is `>=0.17.0, <0.18.0`. So every minor release puts byonk's own screens
# outside their own declared range, and `GET /api/admin/screens` starts
# reporting a compat_warning on all of them, including the fallback screen.
# That drift shipped once already, in 0.16.0.
#
# Two places declare it and both must move together, or the docs end up
# contradicting the screens they quote:
#   <root>/screens/**/meta.yaml   -- the screens byonk ships
#   <root>/docs/src/**/*.md       -- the meta.yaml examples the tutorial teaches
#
# Only major.minor is written, matching `compat::engine_compat_req()`, which is
# what a newly scaffolded screen gets. A bugfix release therefore changes
# nothing, which is correct: 0.17.2 is inside ^0.17.
#
# The rewrite is anchored at column 0. In a meta.yaml that skips the nested
# keys under `params:`; in a Markdown file it confines the change to lines
# inside a fenced YAML block, which is where these examples live.
set -euo pipefail

version="${1:?usage: bump-screen-engine.sh <version> [root]}"
root="${2:-.}"

# major.minor -- the series, not the point release.
series="${version%.*}"
if [ "$series" = "$version" ] || [ -z "$series" ]; then
  echo "bump-screen-engine.sh: cannot derive a major.minor series from '$version'" >&2
  exit 1
fi

changed=0
bump() { # file
  local before
  before=$(cat "$1")
  # `[^"]*`, not `.*`: greedy matching would swallow a trailing comment that
  # happens to contain a quote, taking the comment with it.
  perl -i -pe 's/^byonk: *"[^"]*"/byonk: "'"$series"'"/' "$1"
  if [ "$before" != "$(cat "$1")" ]; then
    echo "  $1"
    changed=$((changed + 1))
  fi
}

if [ -d "$root/screens" ]; then
  while IFS= read -r f; do bump "$f"; done \
    < <(find "$root/screens" -name meta.yaml -type f | sort)
fi

if [ -d "$root/docs/src" ]; then
  while IFS= read -r f; do bump "$f"; done \
    < <(grep -rl '^byonk: *"' "$root/docs/src" --include='*.md' | sort)
fi

echo "bump-screen-engine.sh: ${changed} file(s) moved to byonk: \"${series}\""
