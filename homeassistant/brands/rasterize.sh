#!/usr/bin/env bash
# Resize the byonk brand master PNGs into the sizes required by
# home-assistant/brands and the HA add-on store. Masters are the committed
# *.src.png artwork; this script only downscales them (reproducible). Uses
# macOS `sips` (Scriptable Image Processing System).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

command -v sips >/dev/null || { echo "sips not found (macOS built-in)"; exit 1; }

icon_src=byonk-icon.src.png   # square master (2048x2048)
logo_src=byonk-logo.src.png   # wide master (~2:1)

# home-assistant/brands: square icon 256/512, logo max 512 wide (aspect kept).
sips -z 256 256 "$icon_src" --out icon.png            >/dev/null
sips -z 512 512 "$icon_src" --out 'icon@2x.png'       >/dev/null
sips --resampleWidth 512  "$logo_src" --out logo.png       >/dev/null
sips --resampleWidth 1024 "$logo_src" --out 'logo@2x.png'  >/dev/null

# HA add-on store: square icon + a store logo.
cp icon.png ../byonk/icon.png
sips --resampleWidth 250 "$logo_src" --out ../byonk/logo.png >/dev/null

echo "resized: $(ls icon.png 'icon@2x.png' logo.png 'logo@2x.png')"
