#!/bin/bash
# Generate sample screen images for documentation
#
# Prerequisites:
# - Byonk server running at http://localhost:3000
# - curl installed
#
# Usage:
#   ./scripts/generate-samples.sh

set -e

BYONK_URL="${BYONK_URL:-http://localhost:3000}"
OUTPUT_DIR="$(dirname "$0")/../content/en/public/samples"

mkdir -p "$OUTPUT_DIR"

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
echo "To use in docs, reference as: /samples/transit.png"
