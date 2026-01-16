#!/bin/bash
# Manage versioned documentation for GitHub Pages
# Usage: manage-doc-versions.sh <site-dir> <action> [version]
#   Actions:
#     add-version <version>  - Add a new version directory
#     add-dev                - Add/update dev version
#     cull                   - Remove old versions (keep last 4 minor releases)
#     update-json            - Update versions.json
#     generate-redirect      - Generate root index.html redirect

set -euo pipefail

SITE_DIR="${1:?Site directory required}"
ACTION="${2:?Action required}"
VERSION="${3:-}"

VERSIONS_JSON="$SITE_DIR/versions.json"
KEEP_MINOR_VERSIONS=4

# Initialize versions.json if it doesn't exist
init_versions_json() {
    if [[ ! -f "$VERSIONS_JSON" ]]; then
        echo '{"latest": null, "versions": []}' > "$VERSIONS_JSON"
    fi
}

# Get all version directories (excluding dev)
get_version_dirs() {
    find "$SITE_DIR" -maxdepth 1 -type d -name 'v*' | \
        xargs -I{} basename {} | \
        sort -V
}

# Parse version into components: v0.8.1 -> "0 8 1"
parse_version() {
    echo "$1" | sed 's/^v//' | tr '.' ' '
}

# Get minor version: v0.8.1 -> "0.8"
get_minor() {
    local v="$1"
    echo "$v" | sed 's/^v//' | cut -d. -f1,2
}

# Cull old versions - keep only latest patch of last N minor versions
cull_versions() {
    local versions
    versions=$(get_version_dirs)

    if [[ -z "$versions" ]]; then
        echo "No versions to cull"
        return
    fi

    # Group by minor version, keep only latest patch
    declare -A minor_to_latest

    for v in $versions; do
        minor=$(get_minor "$v")
        current_latest="${minor_to_latest[$minor]:-}"

        if [[ -z "$current_latest" ]]; then
            minor_to_latest[$minor]="$v"
        else
            # Compare patch versions - keep higher
            current_patch=$(echo "$current_latest" | sed 's/^v//' | cut -d. -f3)
            new_patch=$(echo "$v" | sed 's/^v//' | cut -d. -f3)
            if [[ "$new_patch" -gt "$current_patch" ]]; then
                # Remove old patch version
                echo "Removing older patch: $current_latest (keeping $v)"
                rm -rf "${SITE_DIR:?}/$current_latest"
                minor_to_latest[$minor]="$v"
            else
                echo "Removing older patch: $v (keeping $current_latest)"
                rm -rf "${SITE_DIR:?}/$v"
            fi
        fi
    done

    # Now keep only the last N minor versions
    local kept_minors
    kept_minors=$(printf '%s\n' "${!minor_to_latest[@]}" | sort -V | tail -n "$KEEP_MINOR_VERSIONS")

    for minor in "${!minor_to_latest[@]}"; do
        if ! echo "$kept_minors" | grep -q "^${minor}$"; then
            local v="${minor_to_latest[$minor]}"
            echo "Removing old minor version: $v"
            rm -rf "${SITE_DIR:?}/$v"
        fi
    done
}

# Update versions.json based on what's in the site directory
update_versions_json() {
    init_versions_json

    local versions
    versions=$(get_version_dirs | sort -Vr)
    local latest=""
    local json_versions="[]"

    # Find latest stable version
    if [[ -n "$versions" ]]; then
        latest=$(echo "$versions" | head -1)
    fi

    # Build versions array
    local first=true
    json_versions="["

    # Add dev first if it exists
    if [[ -d "$SITE_DIR/dev" ]]; then
        json_versions+="{\"version\":\"dev\",\"path\":\"/byonk/dev/\",\"prerelease\":true}"
        first=false
    fi

    # Add stable versions (newest first)
    for v in $versions; do
        if [[ "$first" == "false" ]]; then
            json_versions+=","
        fi
        json_versions+="{\"version\":\"$v\",\"path\":\"/byonk/$v/\"}"
        first=false
    done

    json_versions+="]"

    # Write the JSON
    cat > "$VERSIONS_JSON" << EOF
{
  "latest": "${latest:-null}",
  "versions": $json_versions
}
EOF

    echo "Updated versions.json:"
    cat "$VERSIONS_JSON"
}

# Generate root index.html that redirects to latest stable
generate_redirect() {
    init_versions_json

    local latest
    latest=$(cat "$VERSIONS_JSON" | grep -o '"latest": *"[^"]*"' | cut -d'"' -f4)

    if [[ -z "$latest" || "$latest" == "null" ]]; then
        # No stable version yet, redirect to dev
        latest="dev"
    fi

    cat > "$SITE_DIR/index.html" << 'EOF'
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>Byonk Documentation</title>
    <script>
        // Redirect to latest stable version
        fetch('/byonk/versions.json')
            .then(r => r.json())
            .then(data => {
                const target = data.latest ? `/byonk/${data.latest}/` : '/byonk/dev/';
                window.location.replace(target);
            })
            .catch(() => {
                // Fallback redirect
EOF
    echo "                window.location.replace('/byonk/${latest}/');" >> "$SITE_DIR/index.html"
    cat >> "$SITE_DIR/index.html" << 'EOF'
            });
    </script>
    <noscript>
EOF
    echo "        <meta http-equiv=\"refresh\" content=\"0; url=/byonk/${latest}/\">" >> "$SITE_DIR/index.html"
    cat >> "$SITE_DIR/index.html" << 'EOF'
    </noscript>
</head>
<body>
    <p>Redirecting to documentation...</p>
</body>
</html>
EOF

    echo "Generated redirect to $latest"
}

# Main action handler
case "$ACTION" in
    add-version)
        [[ -z "$VERSION" ]] && { echo "Version required for add-version"; exit 1; }
        echo "Adding version $VERSION"
        # The actual copying is done by the workflow
        ;;
    add-dev)
        echo "Adding dev version"
        # The actual copying is done by the workflow
        ;;
    cull)
        cull_versions
        ;;
    update-json)
        update_versions_json
        ;;
    generate-redirect)
        generate_redirect
        ;;
    *)
        echo "Unknown action: $ACTION"
        exit 1
        ;;
esac
