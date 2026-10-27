#!/bin/bash
# Generate sample screen images for documentation
#
# Usage:
#   ./generate-samples.sh
#
# Environment:
#   BYONK_BIN - Path to byonk binary (default: auto-detect)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$SCRIPT_DIR/src/images"

echo "Script directory: $SCRIPT_DIR"
echo "Project directory: $PROJECT_DIR"
echo "Output directory: $OUTPUT_DIR"

# Find byonk binary
find_byonk() {
    if [[ -n "$BYONK_BIN" ]]; then
        echo "$BYONK_BIN"
    elif [[ -x "$PROJECT_DIR/target/release/byonk" ]]; then
        echo "$PROJECT_DIR/target/release/byonk"
    elif [[ -x "$PROJECT_DIR/target/debug/byonk" ]]; then
        echo "$PROJECT_DIR/target/debug/byonk"
    else
        echo ""
    fi
}

BYONK=$(find_byonk)
if [[ -z "$BYONK" ]]; then
    echo "Error: byonk binary not found. Run 'make build' or 'make release' first."
    echo "Checked locations:"
    echo "  - $PROJECT_DIR/target/release/byonk"
    echo "  - $PROJECT_DIR/target/debug/byonk"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"
cd "$PROJECT_DIR"

# The render CLI reads assets and the device map from env vars. Without these
# it falls back to the EMBEDDED default-config.yaml, which knows only the
# reserved DEFAULT device -- so every sample device silently rendered the
# default screen instead of its own. The device map is docs/samples-config.yaml,
# not the repository's config.yaml: that one is a working dev config, and the
# docs must not change whenever it does.
export CONFIG_FILE="$SCRIPT_DIR/samples-config.yaml"
export SCREENS_DIR="$PROJECT_DIR/screens"
# The examples repo otherwise resolves to <SCREENS_DIR>/../examples, a seeded
# (git-ignored) copy that is stale on a dev machine and absent in CI.
export EXAMPLES_DIR="$PROJECT_DIR/screens/examples"
export FONTS_DIR="$PROJECT_DIR/fonts"

echo "Generating sample images using $BYONK..."
echo "Working directory: $(pwd)"
echo "Fonts directory exists: $(test -d fonts && echo 'yes' || echo 'no')"
echo "Screens directory exists: $(test -d screens && echo 'yes' || echo 'no')"
echo "Config file exists: $(test -f "$CONFIG_FILE" && echo 'yes' || echo 'no')"

FAILED=0

# Render each screen directly (no server needed)
render_screen() {
    local mac="$1"
    local name="$2"
    shift 2
    local extra_args="$@"
    echo "  Rendering $name (MAC: $mac)..."
    if "$BYONK" render --mac "$mac" --output "$OUTPUT_DIR/$name.png" $extra_args 2>&1; then
        echo "    -> OK: $OUTPUT_DIR/$name.png ($(stat -f%z "$OUTPUT_DIR/$name.png" 2>/dev/null || stat -c%s "$OUTPUT_DIR/$name.png" 2>/dev/null) bytes)"
    else
        echo "    -> FAILED"
        FAILED=$((FAILED + 1))
    fi
}

# Generate samples for configured devices
render_screen "TR:AN:SI:T0:00:00" "transit"
render_screen "TE:ST:GR:AY:00:00" "graytest"
render_screen "00:00:00:00:00:00" "default" --battery=4.12 --rssi=-65
render_screen "TE:ST:HE:LL:00:00" "hello"
render_screen "HI:NT:DE:MO:00:00" "hintdemo"
render_screen "X11Helv" "fontdemo-bitmap-helv"
render_screen "X11Misc" "fontdemo-bitmap-misc"

# Font rendering comparisons for the Font Hinting page and the Bitmap Fonts
# section. Hinting is shown on a black-and-white panel (where byonk draws text
# 1-bit) and on the default 4-grey panel (where it anti-aliases).
render_screen "FO:NT:HI:NT:00:00" "font-hinting-bw" --colors="#000000,#FFFFFF"
render_screen "FO:NT:HI:NT:00:00" "font-hinting-grey"
render_screen "FO:NT:ST:RK:00:00" "font-strikes" --colors="#000000,#FFFFFF"

# 6-color palette samples (simulating a color e-ink display)
PALETTE_6CLR="#000000,#FFFFFF,#00FF00,#0000FF,#FF0000,#FFFF00"
render_screen "TE:ST:GR:AY:00:00" "graytest-6color" --colors="$PALETTE_6CLR"
render_screen "00:00:00:00:00:00" "default-6color" --battery=4.12 --rssi=-65 --colors="$PALETTE_6CLR"

# Registration/enrollment screen (unregistered device with a fake code)
render_screen "UN:RE:GI:ST:ER:ED" "registration" --registration-code=ABCDEFGHJK --battery=4.12 --rssi=-65

echo ""
echo "Generated images:"
ls -la "$OUTPUT_DIR/"

if [[ $FAILED -gt 0 ]]; then
    echo ""
    echo "WARNING: $FAILED screen(s) failed to render"
    exit 1
fi

echo ""
echo "Done! Sample images saved to $OUTPUT_DIR/"
echo "To use in docs, reference as: images/transit.png"
