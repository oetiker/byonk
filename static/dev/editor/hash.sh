#!/bin/sh
# SHA-256 over the editor sources, portable between macOS and Linux.
#
# pnpm-lock.yaml is included so a dependency bump also invalidates the
# stamp. `find` tolerates a missing path (falls through to whatever it did
# find) instead of aborting, which matters before the very first
# `pnpm install` -- but in this tree pnpm-lock.yaml is committed, so that
# state should never actually occur.
cd "$(dirname "$0")" || exit 1
find src package.json pnpm-lock.yaml -type f 2>/dev/null | LC_ALL=C sort | xargs cat | (sha256sum 2>/dev/null || shasum -a 256) | cut -c1-64
