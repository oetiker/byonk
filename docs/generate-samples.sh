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

echo "Generating sample images using $BYONK..."
echo "Working directory: $(pwd)"
echo "Fonts directory exists: $(test -d fonts && echo 'yes' || echo 'no')"
echo "Screens directory exists: $(test -d screens && echo 'yes' || echo 'no')"
echo "Config file exists: $(test -f config.yaml && echo 'yes' || echo 'no')"

FAILED=0

# Render each screen directly (no server needed)
render_screen() {
    local mac="$1"
    local name="$2"
    echo "  Rendering $name (MAC: $mac)..."
    if "$BYONK" render --mac "$mac" --output "$OUTPUT_DIR/$name.png" 2>&1; then
        echo "    -> OK: $OUTPUT_DIR/$name.png ($(stat -f%z "$OUTPUT_DIR/$name.png" 2>/dev/null || stat -c%s "$OUTPUT_DIR/$name.png" 2>/dev/null) bytes)"
    else
        echo "    -> FAILED"
        FAILED=$((FAILED + 1))
    fi
}

# Generate samples for configured devices
render_screen "94:A9:90:8C:6D:18" "transit"
render_screen "TE:ST:GR:AY:00:00" "graytest"
render_screen "00:00:00:00:00:00" "default"
render_screen "TE:ST:HE:LL:00:00" "hello"

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
