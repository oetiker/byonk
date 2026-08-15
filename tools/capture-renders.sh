#!/usr/bin/env bash
# Render every bundled screen for the three-state comparison around the
# resvg byonk-base integration. Runs against any checkout, because it drives
# only the `byonk render` CLI.
#
# Bucket assignment (det vs nondet) was settled by an actual repeat-run
# diff (Task 1, Step 4), not guessed: `examples/mandelbrot` seeds its RNG
# from `time_now()` and is NOT reproducible between runs, so it lives in
# nondet despite looking like a pure computation. `EXAMPLES_DIR` is set
# below so the `examples` screen repo (mandelbrot, demo/font/*, hello,
# gphoto, webscrape, swiss-departure-board) auto-registers — it is not yet
# a registered screen repo in `config.yaml` on this branch.
set -uo pipefail

OUT="${1:?usage: capture-renders.sh <output-dir>}"
CFG="${CONFIG_FILE:-tools/capture-config.yaml}"
EXAMPLES="${EXAMPLES_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/screens/examples}"
mkdir -p "$OUT/nondeterministic"

# mac:name:bucket   bucket = det | nondet
SCREENS="
AA:BB:CC:00:00:01:calibration-gamut:det
AA:BB:CC:00:00:02:calibration-tone:det
AA:BB:CC:00:00:03:calibration-grey:det
AA:BB:CC:00:00:04:calibration-color:det
AA:BB:CC:00:00:06:demo-font-bitmap:det
AA:BB:CC:00:00:07:demo-font-ttf:det
AA:BB:CC:00:00:08:demo-font-hinting:det
AA:BB:CC:00:00:05:mandelbrot:nondet
AA:BB:CC:00:00:11:hello:nondet
AA:BB:CC:00:00:12:builtin-default:nondet
AA:BB:CC:00:00:13:gphoto:nondet
AA:BB:CC:00:00:14:webscrape:nondet
AA:BB:CC:00:00:15:swiss-departure-board:nondet
"

: > "$OUT/MANIFEST.txt"
for entry in $SCREENS; do
  mac="${entry%:*:*}"
  rest="${entry#"$mac":}"
  name="${rest%:*}"
  bucket="${rest##*:}"
  [ "$bucket" = det ] && dir="$OUT" || dir="$OUT/nondeterministic"
  CONFIG_FILE="$CFG" EXAMPLES_DIR="$EXAMPLES" cargo run --release --quiet -- \
      render --mac "$mac" --output "$dir/$name.png" >/dev/null 2>&1
  echo "$name $bucket exit=$?" >> "$OUT/MANIFEST.txt"
done
cat "$OUT/MANIFEST.txt"
