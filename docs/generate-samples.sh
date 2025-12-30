#!/bin/bash
# Generate sample screen images for documentation
#
# Usage:
#   ./generate-samples.sh           # Uses running server or starts one
#   ./generate-samples.sh --no-auto # Requires server already running
#
# Environment:
#   BYONK_URL - Server URL (default: http://localhost:3000)
#   BYONK_BIN - Path to byonk binary (default: auto-detect)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BYONK_URL="${BYONK_URL:-http://localhost:3000}"
OUTPUT_DIR="$SCRIPT_DIR/src/images"
AUTO_START=true
SERVER_PID=""

# Parse arguments
if [[ "$1" == "--no-auto" ]]; then
    AUTO_START=false
fi

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

# Check if server is responding
server_ready() {
    curl -s -o /dev/null -w "%{http_code}" "$BYONK_URL/api/display" -H "ID: test" -H "Access-Token: test" 2>/dev/null | grep -q "200"
}

# Cleanup on exit
cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "Stopping byonk server (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Start server if needed
start_server_if_needed() {
    if server_ready; then
        echo "Using existing server at $BYONK_URL"
        return 0
    fi

    if [[ "$AUTO_START" != "true" ]]; then
        echo "Error: Server not running at $BYONK_URL"
        echo "Start byonk first or run without --no-auto"
        exit 1
    fi

    local byonk_bin
    byonk_bin=$(find_byonk)
    if [[ -z "$byonk_bin" ]]; then
        echo "Error: byonk binary not found. Run 'make build' or 'make release' first."
        exit 1
    fi

    echo "Starting byonk server from $byonk_bin..."
    cd "$PROJECT_DIR"
    "$byonk_bin" &
    SERVER_PID=$!

    # Wait for server to be ready (max 10 seconds)
    echo "Waiting for server to start..."
    for i in {1..20}; do
        if server_ready; then
            echo "Server ready!"
            return 0
        fi
        sleep 0.5
    done

    echo "Error: Server failed to start within 10 seconds"
    exit 1
}

mkdir -p "$OUTPUT_DIR"

start_server_if_needed

echo "Generating sample images from $BYONK_URL..."

# Function to get signed image URL and download
fetch_sample() {
    local device_id="$1"
    local output_name="$2"

    echo "  Fetching $output_name..."

    # Get display response with signed image URL
    local display_response
    display_response=$(curl -s -H "ID: $device_id" -H "Access-Token: sample" "$BYONK_URL/api/display")

    # Extract image URL (simple grep, works for our JSON)
    local image_url
    image_url=$(echo "$display_response" | grep -o '"image_url":"[^"]*"' | cut -d'"' -f4)

    if [ -z "$image_url" ]; then
        echo "    Failed to get image URL for $device_id"
        return 1
    fi

    # Make URL absolute if needed
    if [[ "$image_url" == /* ]]; then
        image_url="${BYONK_URL}${image_url}"
    fi

    # Download the image
    curl -s "$image_url" -o "$OUTPUT_DIR/$output_name.png"
    echo "    Saved to $OUTPUT_DIR/$output_name.png"
}

# Generate samples for configured devices
# Note: These MAC addresses should match your config.yaml

# Transit screen
fetch_sample "94:A9:90:8C:6D:18" "transit"

# Gray test screen
fetch_sample "TE:ST:GR:AY:00:00" "graytest"

# Default screen (any unknown device)
fetch_sample "00:00:00:00:00:00" "default"

echo ""
echo "Done! Sample images saved to $OUTPUT_DIR/"
echo ""
echo "To use in docs, reference as: images/transit.png"
