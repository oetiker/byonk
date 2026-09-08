#!/bin/sh
# SHA-256 over everything that determines the built bundle's content,
# portable between macOS and Linux.
#
# - src/, package.json, pnpm-lock.yaml: the code and its exact dependency
#   resolutions.
# - rollup.config.mjs: output.name/format, plugin list and order (e.g.
#   terser()) all change the built bytes without touching src/.
# - stamp.mjs: the logic that writes the stamp itself -- if it changes how
#   the bundle is packaged (or how the stamp is computed/written), the old
#   stamp should no longer read as trustworthy.
# - hash.sh (this file): the hashing method is part of what makes the stamp
#   meaningful; a change here invalidates every previously-stamped bundle
#   too. Hashing our own bytes is not circular -- we just read this file's
#   current content like any other input, the same way `sha256sum` reads
#   its own binary without recursing.
#
# `find` tolerates a missing path (falls through to whatever it did find)
# instead of aborting, which matters before the very first `pnpm install`
# -- but in this tree pnpm-lock.yaml is committed, so that state should
# never actually occur.
cd "$(dirname "$0")" || exit 1
find src package.json pnpm-lock.yaml rollup.config.mjs stamp.mjs hash.sh -type f 2>/dev/null | LC_ALL=C sort | xargs cat | (sha256sum 2>/dev/null || shasum -a 256) | cut -c1-64
