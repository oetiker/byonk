# HA Distribution Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make byonk's Home Assistant add-on + integration HACS-default-list distributable — version automation, validation CI, brand assets, publishing docs, and an end-to-end HAOS validation gate.

**Architecture:** No byonk runtime changes. Two testable bash bump scripts couple the add-on `config.yaml` and integration `manifest.json` versions to the byonk release version, wired into `release.yml` with correct timing (integration before the git tag; add-on after the image publishes). A new CI job runs hassfest + the HACS action. A committed SVG mark rasterizes to brand PNGs. Docs and a maintainer-facing external-PR guide close it out.

**Tech Stack:** Bash (bump scripts, matching `release.yml`'s existing sed/perl style), GitHub Actions (`home-assistant/actions/hassfest`, `hacs/action`), `rsvg-convert` (SVG→PNG), mdBook (docs), existing Rust `cargo test` (add-on manifest validation).

## Global Constraints

- **No byonk Rust runtime changes.** Only the release *workflow*, bash scripts, CI, brand assets, docs, and (allowed) an existing Rust *test* may change.
- **All three versions couple to the byonk release version** (the value `release.yml`'s `version` job computes).
- **Timing is load-bearing:** integration `manifest.json` bumps in the `version` job **before** the git tag (HACS installs from the tag); add-on `config.yaml` bumps in a new job **after** `build-container` publishes the image, committing to `main` (HA reads the add-on from `main`).
- Add-on image is `ghcr.io/oetiker/byonk`; `arch: [amd64, aarch64]`.
- The `hacs/action` must set **`ignore: brands`** until the `home-assistant/brands` PR is merged (otherwise the brands check fails on an unpublished icon).
- **Never `git add -A` / `git add .`** — add explicit paths only, and verify `git diff --cached` before each commit (repo guardrail `no-git-add-all`).
- Bump scripts take an explicit target-path argument so they are testable against fixtures in a temp dir (no hardcoded repo paths).

## Execution notes

- **Tasks 1–8 are code/doc work** — subagent-able, each ends with a commit.
- **Tasks 9–11 are manual human checkpoints** (drive the QEMU VM / eyeball the icon & e-ink / file external PRs with the maintainer's GitHub identity). A subagent cannot do these; they are executed interactively.
- Work continues on the current stacked branch `feat/ha-distribution-readiness` (on top of Plan B). The single merge to `main` is Task 10 ("merge later").

---

### Task 1: `bump-addon-version.sh` (TDD)

**Files:**
- Create: `tools/release/bump-addon-version.sh`
- Create: `tools/release/testdata/addon/config.input.yaml`
- Create: `tools/release/testdata/addon/changelog.input.md`
- Create: `tools/release/testdata/addon/config.feature.expected.yaml`
- Create: `tools/release/testdata/addon/config.major.expected.yaml`
- Create: `tools/release/testdata/addon/changelog.expected.md`
- Create: `tools/release/test-bump-addon-version.sh`

**Interfaces:**
- Produces: `bump-addon-version.sh <version> <release_type> [addon_dir]` — rewrites `<addon_dir>/config.yaml` `version:`, prepends a `<addon_dir>/CHANGELOG.md` entry, and (when `release_type == major`) adds/appends `<version>` under `breaking_versions:` in `config.yaml`. `addon_dir` defaults to `homeassistant/byonk`. `release_type` ∈ `{bugfix, feature, major}`.

- [ ] **Step 1: Write the fixtures**

`tools/release/testdata/addon/config.input.yaml`:
```yaml
name: Byonk
version: "0.15.0"
slug: byonk
image: ghcr.io/oetiker/byonk
arch:
  - amd64
  - aarch64
```

`tools/release/testdata/addon/changelog.input.md`:
```markdown
# Changelog

## 0.15.0

- Initial Home Assistant add-on for Byonk.
```

`tools/release/testdata/addon/config.feature.expected.yaml` (feature bump → version only):
```yaml
name: Byonk
version: "0.16.0"
slug: byonk
image: ghcr.io/oetiker/byonk
arch:
  - amd64
  - aarch64
```

`tools/release/testdata/addon/config.major.expected.yaml` (major bump → version + `breaking_versions` appended at EOF):
```yaml
name: Byonk
version: "1.0.0"
slug: byonk
image: ghcr.io/oetiker/byonk
arch:
  - amd64
  - aarch64
breaking_versions:
  - "1.0.0"
```

`tools/release/testdata/addon/changelog.expected.md` (feature bump to 0.16.0 → new section prepended):
```markdown
# Changelog

## 0.16.0

- Update to byonk 0.16.0 (see the main CHANGES.md for details).

## 0.15.0

- Initial Home Assistant add-on for Byonk.
```

- [ ] **Step 2: Write the test harness**

`tools/release/test-bump-addon-version.sh`:
```bash
#!/usr/bin/env bash
# Unit test for bump-addon-version.sh — runs it against fixtures in a temp dir
# and diffs the result against golden files. No live release.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="$here/testdata/addon"
fail=0

run_case() {
  local name="$1" version="$2" rtype="$3" expect_cfg="$4" expect_log="$5"
  local tmp; tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  cp "$data/config.input.yaml" "$tmp/config.yaml"
  cp "$data/changelog.input.md" "$tmp/CHANGELOG.md"

  "$here/bump-addon-version.sh" "$version" "$rtype" "$tmp"

  if ! diff -u "$data/$expect_cfg" "$tmp/config.yaml"; then
    echo "FAIL [$name]: config.yaml mismatch"; fail=1
  fi
  if [ -n "$expect_log" ] && ! diff -u "$data/$expect_log" "$tmp/CHANGELOG.md"; then
    echo "FAIL [$name]: CHANGELOG.md mismatch"; fail=1
  fi
}

run_case "feature" "0.16.0" "feature" "config.feature.expected.yaml" "changelog.expected.md"
run_case "major"   "1.0.0"  "major"   "config.major.expected.yaml"   ""

if [ "$fail" -eq 0 ]; then echo "OK: bump-addon-version tests passed"; else exit 1; fi
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `chmod +x tools/release/test-bump-addon-version.sh && tools/release/test-bump-addon-version.sh`
Expected: FAIL — `bump-addon-version.sh: No such file or directory` (script not yet created).

- [ ] **Step 4: Implement `bump-addon-version.sh`**

`tools/release/bump-addon-version.sh`:
```bash
#!/usr/bin/env bash
# Bumps the HA add-on version to match a published ghcr.io/oetiker/byonk tag.
# Usage: bump-addon-version.sh <version> <release_type> [addon_dir]
set -euo pipefail

version="${1:?usage: bump-addon-version.sh <version> <release_type> [addon_dir]}"
release_type="${2:?missing release_type (bugfix|feature|major)}"
addon_dir="${3:-homeassistant/byonk}"

cfg="$addon_dir/config.yaml"
log="$addon_dir/CHANGELOG.md"

# 1. Rewrite the top-level `version:` line (column 0 anchor — never touches
#    breaking_versions entries, which are indented list items).
perl -i -pe 's/^version:.*/version: "'"$version"'"/' "$cfg"

# 2. Prepend a CHANGELOG section right after the `# Changelog` header.
perl -i -0777 -pe 's/(# Changelog\n)/$1\n## '"$version"'\n\n- Update to byonk '"$version"' (see the main CHANGES.md for details).\n/' "$log"

# 3. On a major release, record the breaking version so Supervisor warns.
if [ "$release_type" = "major" ]; then
  if grep -qE '^breaking_versions:' "$cfg"; then
    # Append under the existing key.
    perl -i -pe 's/^(breaking_versions:.*)/$1\n  - "'"$version"'"/ if !$done && /^breaking_versions:/ && ($done=1)' "$cfg"
  else
    printf 'breaking_versions:\n  - "%s"\n' "$version" >> "$cfg"
  fi
fi
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `tools/release/test-bump-addon-version.sh`
Expected: PASS — `OK: bump-addon-version tests passed`.

- [ ] **Step 6: Harden the existing add-on manifest test (semver guard)**

Modify `tests/addon_manifest_test.rs` — in `addon_config_matches_design`, after the existing non-empty version assertion, assert the version is semver-shaped so a future bad bump can't land:
```rust
    // version must look like X.Y.Z (a valid image tag / add-on version)
    let ver = cfg["version"].as_str().unwrap_or("");
    assert!(
        ver.split('.').count() == 3 && ver.split('.').all(|p| p.parse::<u32>().is_ok()),
        "version must be semver X.Y.Z, got {ver:?}"
    );
```

- [ ] **Step 7: Run the Rust test to verify it passes**

Run: `cargo test --test addon_manifest_test`
Expected: PASS (current committed version `0.15.0` is valid semver).

- [ ] **Step 8: Commit**

```bash
git add tools/release/bump-addon-version.sh tools/release/test-bump-addon-version.sh tools/release/testdata/addon tests/addon_manifest_test.rs
git diff --cached --stat
git commit -m "feat(release): add tested bump-addon-version.sh + semver guard"
```

---

### Task 2: `bump-integration-version.sh` (TDD)

**Files:**
- Create: `tools/release/bump-integration-version.sh`
- Create: `tools/release/testdata/integration/manifest.input.json`
- Create: `tools/release/testdata/integration/manifest.expected.json`
- Create: `tools/release/test-bump-integration-version.sh`

**Interfaces:**
- Produces: `bump-integration-version.sh <version> [manifest_path]` — rewrites the `"version"` value in `manifest.json`. `manifest_path` defaults to `custom_components/byonk/manifest.json`. Preserves all other keys and formatting.

- [ ] **Step 1: Write the fixtures**

`tools/release/testdata/integration/manifest.input.json`:
```json
{
  "domain": "byonk",
  "name": "Byonk",
  "version": "0.1.0",
  "documentation": "https://github.com/oetiker/byonk",
  "codeowners": ["@oetiker"],
  "config_flow": true,
  "iot_class": "local_polling"
}
```

`tools/release/testdata/integration/manifest.expected.json` (only `version` changed):
```json
{
  "domain": "byonk",
  "name": "Byonk",
  "version": "0.16.0",
  "documentation": "https://github.com/oetiker/byonk",
  "codeowners": ["@oetiker"],
  "config_flow": true,
  "iot_class": "local_polling"
}
```

- [ ] **Step 2: Write the test harness**

`tools/release/test-bump-integration-version.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="$here/testdata/integration"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
cp "$data/manifest.input.json" "$tmp/manifest.json"
"$here/bump-integration-version.sh" "0.16.0" "$tmp/manifest.json"
if diff -u "$data/manifest.expected.json" "$tmp/manifest.json"; then
  echo "OK: bump-integration-version tests passed"
else
  echo "FAIL: manifest.json mismatch"; exit 1
fi
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `chmod +x tools/release/test-bump-integration-version.sh && tools/release/test-bump-integration-version.sh`
Expected: FAIL — `bump-integration-version.sh: No such file or directory`.

- [ ] **Step 4: Implement `bump-integration-version.sh`**

`tools/release/bump-integration-version.sh`:
```bash
#!/usr/bin/env bash
# Bumps the HA integration manifest version to the byonk release version.
# Usage: bump-integration-version.sh <version> [manifest_path]
set -euo pipefail

version="${1:?usage: bump-integration-version.sh <version> [manifest_path]}"
manifest="${2:-custom_components/byonk/manifest.json}"

# Rewrite only the "version" string value; leave all other keys/formatting intact.
perl -i -pe 's/("version":\s*")[^"]*(")/${1}'"$version"'${2}/' "$manifest"
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `tools/release/test-bump-integration-version.sh`
Expected: PASS — `OK: bump-integration-version tests passed`.

- [ ] **Step 6: Commit**

```bash
git add tools/release/bump-integration-version.sh tools/release/test-bump-integration-version.sh tools/release/testdata/integration
git diff --cached --stat
git commit -m "feat(release): add tested bump-integration-version.sh"
```

---

### Task 3: Wire both bump scripts into `release.yml`

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `bump-addon-version.sh` (Task 1), `bump-integration-version.sh` (Task 2), `needs.version.outputs.version`, `inputs.release_type`.

- [ ] **Step 1: Bump the integration manifest inside the `version` job (before the tag)**

In `.github/workflows/release.yml`, in the `version` job, **after** the "Update CHANGES.md" step and **before** the "Commit and tag" step, add:
```yaml
      - name: Update integration manifest version
        run: |
          chmod +x tools/release/bump-integration-version.sh
          tools/release/bump-integration-version.sh "${{ steps.version.outputs.version }}"
```
Then extend the existing "Commit and tag" step's `git add` to include the manifest:
```yaml
          git add Cargo.toml CHANGES.md custom_components/byonk/manifest.json
```
(Leave the rest of the commit/tag/push step unchanged. The manifest is now in the tagged tree, which is what HACS installs.)

- [ ] **Step 2: Add the `update-addon-version` job (after the image publishes)**

Append a new job to `.github/workflows/release.yml` (top-level under `jobs:`), after `build-container`:
```yaml
  update-addon-version:
    name: Bump Add-on Version
    needs: [version, build-container]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
        with:
          ref: main

      - name: Bump add-on config + changelog
        run: |
          chmod +x tools/release/bump-addon-version.sh
          tools/release/bump-addon-version.sh \
            "${{ needs.version.outputs.version }}" \
            "${{ inputs.release_type }}"

      - name: Commit and push to main
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add homeassistant/byonk/config.yaml homeassistant/byonk/CHANGELOG.md
          git commit -m "Bump add-on to ${{ needs.version.outputs.version }}"
          git pull --rebase origin main
          git push origin main
```
(The `ref: main` checkout + `git pull --rebase` before push handles the tag commit the `version` job already pushed. The add-on now points at the just-published `ghcr.io/oetiker/byonk:${version}` tag.)

- [ ] **Step 3: Verify the workflow YAML parses and the job graph is correct**

Run: `python3 -c "import yaml,sys; d=yaml.safe_load(open('.github/workflows/release.yml')); j=d['jobs']; assert j['update-addon-version']['needs']==['version','build-container']; assert 'Update integration manifest version' in [s.get('name') for s in j['version']['steps']]; print('release.yml wiring OK')"`
Expected: `release.yml wiring OK`

- [ ] **Step 4: Local dry-run against real files in a temp copy (no live release)**

Run:
```bash
tmp=$(mktemp -d); mkdir -p "$tmp/homeassistant/byonk" "$tmp/cc"
cp homeassistant/byonk/config.yaml homeassistant/byonk/CHANGELOG.md "$tmp/homeassistant/byonk/"
cp custom_components/byonk/manifest.json "$tmp/cc/manifest.json"
tools/release/bump-addon-version.sh 9.9.9 feature "$tmp/homeassistant/byonk"
tools/release/bump-integration-version.sh 9.9.9 "$tmp/cc/manifest.json"
grep '^version:' "$tmp/homeassistant/byonk/config.yaml"; grep '"version"' "$tmp/cc/manifest.json"; head -5 "$tmp/homeassistant/byonk/CHANGELOG.md"; rm -rf "$tmp"
```
Expected: add-on `version: "9.9.9"`, manifest `"version": "9.9.9"`, CHANGELOG shows a new `## 9.9.9` section. (This mutates only the temp copy — the repo files are untouched.)

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml
git diff --cached --stat
git commit -m "feat(release): couple add-on + integration versions to the release"
```

---

### Task 4: HA validation CI job (hassfest + HACS action) + release-script tests in CI

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `tools/release/test-bump-addon-version.sh`, `tools/release/test-bump-integration-version.sh` (Tasks 1–2).

- [ ] **Step 1: Add a `home-assistant` validation job**

Append to `.github/workflows/ci.yml` under `jobs:`:
```yaml
  home-assistant:
    name: HA Validation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Hassfest
        uses: home-assistant/actions/hassfest@master

      - name: HACS validation
        uses: hacs/action@main
        with:
          category: integration
          # TODO: remove `ignore: brands` once the home-assistant/brands PR is
          # merged (see docs/superpowers/ha-publishing.md).
          ignore: brands
```

- [ ] **Step 2: Add a release-scripts test job**

Append another job to `.github/workflows/ci.yml`:
```yaml
  release-scripts:
    name: Release Scripts
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Test bump scripts
        run: |
          chmod +x tools/release/*.sh
          tools/release/test-bump-addon-version.sh
          tools/release/test-bump-integration-version.sh
```

- [ ] **Step 3: Verify the CI YAML parses and jobs are present**

Run: `python3 -c "import yaml; j=yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']; assert 'home-assistant' in j and 'release-scripts' in j; print('ci.yml jobs OK')"`
Expected: `ci.yml jobs OK`

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git diff --cached --stat
git commit -m "ci: add hassfest + HACS validation and release-script tests"
```

Note: hassfest/HACS actions only exercise on GitHub. Their green status is confirmed on the PR run in Task 10; the `ignore: brands` is removed in Task 11 after the brands PR merges.

---

### Task 5: Brand mark — SVG source, rasterize script, PNGs, add-on icon (VISUAL APPROVAL)

**Files:**
- Create: `homeassistant/brands/byonk.svg`
- Create: `homeassistant/brands/byonk-logo.svg`
- Create: `homeassistant/brands/rasterize.sh`
- Create (generated): `homeassistant/brands/icon.png`, `homeassistant/brands/icon@2x.png`, `homeassistant/brands/logo.png`, `homeassistant/brands/logo@2x.png`
- Create (generated): `homeassistant/byonk/icon.png`, `homeassistant/byonk/logo.png`

**Interfaces:**
- Produces: brand PNGs consumed verbatim by the `home-assistant/brands` PR (Task 6) and the add-on store (`homeassistant/byonk/`).

- [ ] **Step 1: Write the icon SVG (starting candidate)**

`homeassistant/brands/byonk.svg` — minimal, e-ink-appropriate, 2-colour (black on white), an e-ink panel enclosing an ink droplet:
```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <rect width="256" height="256" fill="#ffffff"/>
  <rect x="28" y="28" width="200" height="200" rx="28"
        fill="none" stroke="#000000" stroke-width="14"/>
  <path d="M128 66 C 168 116, 186 148, 128 196 C 70 148, 88 116, 128 66 Z"
        fill="#000000"/>
</svg>
```

- [ ] **Step 2: Write the logo SVG (wordmark)**

`homeassistant/brands/byonk-logo.svg` — the mark plus the wordmark, sized for a wide logo:
```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 160" width="640" height="160">
  <rect width="640" height="160" fill="#ffffff"/>
  <rect x="16" y="16" width="128" height="128" rx="20"
        fill="none" stroke="#000000" stroke-width="10"/>
  <path d="M80 44 C 106 76, 118 96, 80 128 C 42 96, 54 76, 80 44 Z" fill="#000000"/>
  <text x="176" y="112" font-family="Helvetica, Arial, sans-serif"
        font-size="96" font-weight="700" fill="#000000">byonk</text>
</svg>
```

- [ ] **Step 3: Write the rasterize script**

`homeassistant/brands/rasterize.sh`:
```bash
#!/usr/bin/env bash
# Rasterize the byonk brand SVGs to the PNGs required by home-assistant/brands
# and the HA add-on store. Requires rsvg-convert (librsvg).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

command -v rsvg-convert >/dev/null || { echo "install librsvg (brew install librsvg)"; exit 1; }

rsvg-convert -w 256 -h 256 byonk.svg      -o icon.png
rsvg-convert -w 512 -h 512 byonk.svg      -o 'icon@2x.png'
rsvg-convert -w 640 -h 160 byonk-logo.svg -o logo.png
rsvg-convert -w 1280 -h 320 byonk-logo.svg -o 'logo@2x.png'

# Add-on store assets reuse the same mark.
cp icon.png ../byonk/icon.png
rsvg-convert -w 250 -h 64 byonk-logo.svg -o ../byonk/logo.png

echo "rasterized: $(ls icon.png icon@2x.png logo.png logo@2x.png)"
```

- [ ] **Step 4: Run the rasterize script**

Run: `chmod +x homeassistant/brands/rasterize.sh && homeassistant/brands/rasterize.sh`
Expected: four brand PNGs + two add-on PNGs written; final line lists them.

- [ ] **Step 5: Verify PNG dimensions**

Run: `for f in homeassistant/brands/icon.png:256 homeassistant/brands/icon@2x.png:512; do p=${f%:*}; s=${f#*:}; python3 -c "from struct import unpack; d=open('$p','rb').read(24); w,h=unpack('>II',d[16:24]); assert (w,h)==($s,$s), '$p is %dx%d'%(w,h); print('$p ok', w,h)"; done`
Expected: `icon.png ok 256 256` and `icon@2x.png ok 512 512`.

- [ ] **Step 6: VISUAL APPROVAL CHECKPOINT**

Present the rendered `homeassistant/brands/icon.png` and `logo.png` to the user (open the PNGs / show them). Iterate on the SVG path + wordmark until the user approves the mark. **Do not commit until approved.** The mark can still be swapped later without reopening the plan.

- [ ] **Step 7: Commit**

```bash
git add homeassistant/brands homeassistant/byonk/icon.png homeassistant/byonk/logo.png
git diff --cached --stat
git commit -m "feat(brands): add byonk icon + logo (SVG source + rasterized PNGs)"
```

---

### Task 6: Publishing guide `docs/superpowers/ha-publishing.md`

**Files:**
- Create: `docs/superpowers/ha-publishing.md`

- [ ] **Step 1: Write the guide**

Create `docs/superpowers/ha-publishing.md` with copy-pasteable, exact instructions the maintainer follows (using their own GitHub identity):

```markdown
# Byonk — Home Assistant Publishing (maintainer runbook)

Two external PRs make byonk installable from the HACS default store with a
proper icon. **File them in order** — HACS validation checks brands. Both
require a published GitHub release whose tag contains `custom_components/byonk/`.

## 1. home-assistant/brands PR (do first)

1. Fork `https://github.com/home-assistant/brands`.
2. Add these files (rasterized by `homeassistant/brands/rasterize.sh` in this repo):
   - `custom_integrations/byonk/icon.png`     ← `homeassistant/brands/icon.png` (256×256)
   - `custom_integrations/byonk/icon@2x.png`  ← `homeassistant/brands/icon@2x.png` (512×512)
   - `custom_integrations/byonk/logo.png`     ← `homeassistant/brands/logo.png`
   - `custom_integrations/byonk/logo@2x.png`  ← `homeassistant/brands/logo@2x.png`
3. Open the PR; the domain `byonk` must match `manifest.json`'s `domain`.

## 2. hacs/default PR (after brands merges)

1. Fork `https://github.com/hacs/default`.
2. In the `integration` file, add `oetiker/byonk` on its own line, keeping the
   list alphabetically sorted.
3. Open the PR. The HACS bot validates the repo (release present, `hacs.json`,
   `manifest.json`, brands icon reachable).

## 3. After both merge

- Remove `ignore: brands` from the `hacs/action` step in `.github/workflows/ci.yml`.
- Update `docs/src/guide/ha-integration.md` install steps from the custom-repo URL
  flow to the default-store search flow.
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/ha-publishing.md
git commit -m "docs: add HA publishing runbook (brands + hacs/default PRs)"
```

---

### Task 7: Docs polish — install story + CHANGES.md

**Files:**
- Modify: `docs/src/guide/ha-integration.md`
- Modify: `docs/src/guide/ha-addon.md` (only if it still contains stale/"later release" language)
- Modify: `CHANGES.md`

- [ ] **Step 1: Confirm the current HACS install wording**

Run: `grep -niE "hacs|custom repositor|default (store|list)|later release|coming soon" docs/src/guide/ha-integration.md docs/src/guide/ha-addon.md`
Expected: shows the current install lines to edit. (`ha-addon.md` was already refreshed for the shipped model; touch it only if this grep surfaces stale wording.)

- [ ] **Step 2: Set the HACS install story to custom-repo-now**

In `docs/src/guide/ha-integration.md`, ensure the HACS install section documents the **custom-repository** flow (HACS → ⋮ → Custom repositories → add `https://github.com/oetiker/byonk`, category *Integration*), with a one-line note: "Once byonk is accepted into the HACS default store, you'll be able to find it directly by searching *Byonk* — until then, use the custom-repository step above." (The switch to default-store wording is Task 11, post-acceptance.)

- [ ] **Step 3: Add CHANGES.md entries**

In `CHANGES.md`, under `## Unreleased`, add to the appropriate subsections:
- Under `### New`: `- Home Assistant: the add-on and integration are now HACS-ready — brand icon, hassfest + HACS validation in CI, and automated version coupling so the add-on and integration track each byonk release.`

- [ ] **Step 4: Verify docs build**

Run: `make docs`
Expected: mdBook build completes with no errors.

- [ ] **Step 5: Commit**

```bash
git add docs/src/guide/ha-integration.md CHANGES.md
# add docs/src/guide/ha-addon.md only if it was edited in Step 1
git diff --cached --stat
git commit -m "docs: HACS install story + changelog for distribution readiness"
```

---

### Task 8: Refresh the 4a validation checklist in the VM README

**Files:**
- Modify: `tools/ha-vm/README.md`

- [ ] **Step 1: Locate the existing checklist**

Run: `grep -niE "checklist|default.?screen|subentr|validation" tools/ha-vm/README.md`
Expected: shows the current (stale) validation section anchored on default-screen select + subentry mirroring.

- [ ] **Step 2: Replace it with the shipped-model checklist**

Replace the validation section in `tools/ha-vm/README.md` with the 10-item checklist from the spec (`docs/superpowers/specs/2026-07-06-ha-distribution-readiness-design.md`, §4a) — add-on store install, integration discovery, zero-touch trust, **add-on-owned global config (409 on admin writes)**, **reserved DEFAULT device (live PATCH, 409 DELETE, HA "Byonk Default" re-provision)**, **screen resolution (unregistered→code, registered-unassigned→DEFAULT screen)**, **HA-owned per-device entries + Discovered card**, **screen packages + Update button**, reauth, device-removal grace.

- [ ] **Step 3: Commit**

```bash
git add tools/ha-vm/README.md
git commit -m "docs(ha-vm): refresh validation checklist for the shipped model"
```

---

### Task 9 (MANUAL): Run the 4a validation on the HAOS VM

Not subagent-able — drives the QEMU VM and eyeballs the e-ink + HA UI.

- [ ] **Step 1: Deploy the stack**

```bash
SMB_USER=byonk SMB_PASS=byonk make ha-rebuild      # add-on (no manifest dance for this model)
SMB_USER=byonk SMB_PASS=byonk make ha-deploy       # integration
make ha-ssh CMD="ha core restart"
```

- [ ] **Step 2: Work the refreshed checklist (Task 8)**

Probe the byonk-API items from the Mac host (`curl localhost:3000/api/admin/*`, token read via `ha addons info local_byonk --raw-json`, never printed — memory `ha-vm-admin-api-testing`). Eyeball the e-ink screens + HA cards for the visual items.

- [ ] **Step 3: Record pass/fail in the SDD ledger** (`.superpowers/sdd/progress.md`). Fix any failure in the relevant code before proceeding.

---

### Task 10 (MANUAL): Finish the branch — merge to main

Single merge covering Plan B + this distribution work ("merge later").

- [ ] **Step 1:** Invoke `superpowers:finishing-a-development-branch`.
- [ ] **Step 2:** Open the PR; confirm the new CI jobs (`home-assistant`, `release-scripts`) go green on the PR run (first real exercise of hassfest/HACS — HACS passes because `ignore: brands` is set).
- [ ] **Step 3:** Merge `feat/ha-distribution-readiness` → `main`.

---

### Task 11 (MANUAL, post-first-release): File external PRs + flip to default store

After the first release from `main` publishes a tag containing `custom_components/byonk/`:

- [ ] **Step 1:** Follow `docs/superpowers/ha-publishing.md` — file the `home-assistant/brands` PR, then (after it merges) the `hacs/default` PR.
- [ ] **Step 2:** Remove `ignore: brands` from the `hacs/action` step in `.github/workflows/ci.yml`; commit.
- [ ] **Step 3:** Update `docs/src/guide/ha-integration.md` install steps from the custom-repository flow to the default-store search flow; commit.

---

## Self-Review

**Spec coverage:**
- 4a (validation + refreshed checklist + merge) → Tasks 8, 9, 10. ✅
- 4b (version automation: add-on script, integration script, workflow wiring, timing) → Tasks 1, 2, 3. ✅
- 4c (CI validation, brand icon, publishing doc) → Tasks 4, 5, 6. ✅ (add-on config lint is covered by the existing `tests/addon_manifest_test.rs`, hardened in Task 1.)
- 4d (docs polish, CHANGES.md) → Task 7. ✅
- External PR gating (brands → hacs/default, `ignore: brands` lifecycle) → Tasks 4, 6, 11. ✅
- Invariant "no Rust runtime change" → only `tests/addon_manifest_test.rs` (a test) changes. ✅

**Type/interface consistency:** `bump-addon-version.sh <version> <release_type> [addon_dir]` and `bump-integration-version.sh <version> [manifest_path]` are called with matching argument order in their tests (Tasks 1–2), the workflow (Task 3), and the CI job (Task 4). ✅

**Placeholder scan:** every code step carries real content (SVG paths, bash, YAML, Rust). The only `TODO` is the deliberate `ignore: brands` marker with a removal owner (Task 11). ✅
