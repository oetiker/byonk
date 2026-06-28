# Byonk HA Add-on (Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package byonk as a Home Assistant Supervisor add-on that runs the prebuilt `ghcr.io/oetiker/byonk` image directly, plus the minimal byonk change to read add-on options from `/data/options.json`.

**Architecture:** A declarative add-on package (`repository.yaml` at repo root + `homeassistant/byonk/`) references the prebuilt multi-arch image, wires byonk's paths/bind via static `environment:`, mounts an editable persistent `/config`, and publishes host port 3000 for LAN TRMNL devices. byonk gains one small read-only module that, at `serve` startup, reads `admin_token` + `log_level` from `/data/options.json` and feeds them into its existing in-memory `AppConfig.admin.token` and tracing filter. byonk never generates or persists a token; a blank token leaves the admin API dormant (404). The Phase 3 integration will provision the token automatically (out of scope here).

**Tech Stack:** Rust (axum, serde, serde_json, serde_yaml, tracing), Home Assistant Supervisor add-on YAML, mdBook docs.

## Global Constraints

- **Single source of truth for the admin token = the add-on option** (`/data/options.json`). byonk only *reads* it; it MUST NOT generate, persist, or log a token. Blank/absent → admin API stays dormant (Phase 1 returns 404).
- **byonk stays usable outside HA:** if the options file is absent, the reader is a complete no-op; existing Phase 1 behavior (`BYONK_ADMIN_TOKEN` env, then `config.admin.token`) is unchanged. Explicit `BYONK_ADMIN_TOKEN` env always wins over the option (preserved automatically because `server.rs` resolves env before `config.admin.token`).
- **The options reader is wired into `run_server()` ONLY** — not `dev`, `render`, or `status`.
- **No global `set_var`:** the reader injects the token into the in-memory `AppConfig.admin.token` (the same mechanism `TestApp::new_admin` uses), not into a process env var.
- **Image reference, not a wrapper:** `image: ghcr.io/oetiker/byonk` (multi-arch manifest, no `{arch}`). The add-on `version:` MUST equal a published image tag; initial value `0.15.0` (current release). Release automation of the bump is Phase 4.
- **Add-on options surface is exactly two keys:** `admin_token` (documented "managed by the integration — leave blank") and `log_level`. Do NOT add `registration`/`default_screen` (the Phase 3 integration owns those via the admin API).
- **No Ingress, no `/dev` exposure, no host_network.** LAN access is via `ports: {3000/tcp: 3000}`.
- TDD throughout: failing test → run-fail → implement → run-pass → commit. Run `make check` (fmt + clippy + tests) before finishing each code task.
- Spec: `docs/superpowers/specs/2026-06-28-byonk-homeassistant-phase2-addon-design.md`.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `src/addon_options.rs` (create) | Read/parse `/data/options.json`; map `log_level`→tracing filter; apply `admin_token`→`AppConfig.admin.token`. Pure + unit-tested. | 1 |
| `src/lib.rs` (modify) | Export `pub mod addon_options;` | 1 |
| `src/main.rs` (modify, `run_server`) | Wire the reader into startup: filter for tracing init, warn on malformed, inject token into config. | 2 |
| `tests/common/app.rs` (modify) | Add `TestApp::from_config(config)` helper. | 2 |
| `tests/addon_options_test.rs` (create) | Integration: options token activates admin API; blank keeps it dormant. | 2 |
| `repository.yaml` (create, repo root) | Add-on repository descriptor. | 3 |
| `homeassistant/byonk/config.yaml` (create) | Add-on manifest (image, arch, env, ports, map, options, schema). | 3 |
| `homeassistant/byonk/DOCS.md` (create) | HA "Documentation" tab content. | 3 |
| `homeassistant/byonk/CHANGELOG.md` (create) | HA "Changelog" tab content. | 3 |
| `homeassistant/byonk/translations/en.yaml` (create) | Option labels/help text. | 3 |
| `tests/addon_manifest_test.rs` (create) | Assert the manifest invariants stay consistent with the design. | 3 |
| `docs/src/guide/ha-addon.md` (create) | User-facing add-on guide. | 4 |
| `docs/src/SUMMARY.md` (modify) | Wire the new page into the book. | 4 |
| `CHANGES.md` (modify) | Unreleased changelog entry. | 4 |

---

### Task 1: byonk add-on-options reader module (pure, unit-tested)

**Files:**
- Create: `src/addon_options.rs`
- Modify: `src/lib.rs` (add `pub mod addon_options;`)
- Test: inline `#[cfg(test)]` in `src/addon_options.rs`

**Interfaces:**
- Consumes: `crate::models::AppConfig` (has `pub admin: AdminConfig` with `pub token: Option<String>`); `crate::assets::AssetLoader` (for the apply test's config fixture).
- Produces (used by Task 2):
  - `pub enum ReadResult { Missing, Parsed(AddonOptions), Malformed(String) }`
  - `pub struct AddonOptions { pub admin_token: Option<String>, pub log_level: Option<String> }`
  - `pub fn options_path() -> std::path::PathBuf`
  - `pub fn read_options(path: &std::path::Path) -> ReadResult`
  - `pub fn log_filter(result: &ReadResult) -> Option<String>`
  - `pub fn apply_to_config(result: &ReadResult, config: &mut AppConfig)`

- [ ] **Step 1: Add the module export**

In `src/lib.rs`, add the line alphabetically near the other `pub mod` lines (after `pub mod assets;`):

```rust
pub mod addon_options;
```

- [ ] **Step 2: Write the failing tests**

Create `src/addon_options.rs` with ONLY the test module first (the impl comes in Step 4). This makes the file compile-fail on missing items, which is the expected failure.

```rust
//! Reads Home Assistant add-on options from `/data/options.json`.
//!
//! When byonk runs as an HA Supervisor add-on, Supervisor writes the Configuration
//! tab values to `/data/options.json`. This module reads two of them — `admin_token`
//! and `log_level` — and feeds them into byonk's existing mechanisms (the in-memory
//! `AppConfig.admin.token` and the tracing filter). It never writes the file, never
//! generates a token, and never logs one. Outside HA (file absent) it is a no-op.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetLoader;
    use crate::models::AppConfig;

    fn embedded_config() -> AppConfig {
        let loader = AssetLoader::new(None, None, None);
        AppConfig::load_from_assets(&loader).expect("load embedded config")
    }

    #[test]
    fn missing_file_is_missing() {
        let result = read_options(std::path::Path::new("/no/such/options.json"));
        assert!(matches!(result, ReadResult::Missing));
    }

    #[test]
    fn valid_json_parses_both_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(&path, r#"{"admin_token":"secret","log_level":"info"}"#).unwrap();
        match read_options(&path) {
            ReadResult::Parsed(opts) => {
                assert_eq!(opts.admin_token.as_deref(), Some("secret"));
                assert_eq!(opts.log_level.as_deref(), Some("info"));
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(&path, r#"{"port":3000,"log_level":"warn"}"#).unwrap();
        match read_options(&path) {
            ReadResult::Parsed(opts) => {
                assert_eq!(opts.admin_token, None);
                assert_eq!(opts.log_level.as_deref(), Some("warn"));
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(read_options(&path), ReadResult::Malformed(_)));
    }

    #[test]
    fn log_filter_maps_known_level() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("info".to_string()),
        });
        assert_eq!(log_filter(&r).as_deref(), Some("byonk=info,tower_http=info"));
    }

    #[test]
    fn log_filter_none_for_unknown_level_or_missing() {
        let unknown = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("verbose".to_string()),
        });
        assert_eq!(log_filter(&unknown), None);
        assert_eq!(log_filter(&ReadResult::Missing), None);
    }

    #[test]
    fn apply_sets_token_when_present() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: Some("secret".to_string()),
            log_level: None,
        });
        let mut config = embedded_config();
        config.admin.token = None;
        apply_to_config(&r, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("secret"));
    }

    #[test]
    fn apply_ignores_blank_token_and_missing() {
        let mut config = embedded_config();
        config.admin.token = Some("keep".to_string());

        // Blank/whitespace token must not clobber an existing value.
        let blank = ReadResult::Parsed(AddonOptions {
            admin_token: Some("   ".to_string()),
            log_level: None,
        });
        apply_to_config(&blank, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("keep"));

        // Missing options file leaves config untouched.
        apply_to_config(&ReadResult::Missing, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("keep"));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib addon_options`
Expected: FAIL to compile — `cannot find type/function ReadResult / AddonOptions / read_options / log_filter / apply_to_config in this scope`.

- [ ] **Step 4: Write the minimal implementation**

Prepend the implementation above the `#[cfg(test)]` module in `src/addon_options.rs` (keep the module doc-comment at the very top):

```rust
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::models::AppConfig;

/// Subset of the add-on options byonk consumes. Unknown keys are ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddonOptions {
    #[serde(default)]
    pub admin_token: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
}

/// Outcome of attempting to read the options file.
#[derive(Debug)]
pub enum ReadResult {
    /// No options file present (normal for non-add-on runs).
    Missing,
    /// File present and parsed successfully.
    Parsed(AddonOptions),
    /// File present but unreadable or not valid JSON. Carries a human message.
    Malformed(String),
}

/// Default in-container path Supervisor writes options to.
const DEFAULT_OPTIONS_PATH: &str = "/data/options.json";

/// Resolve the options file path. `BYONK_OPTIONS_FILE` overrides the default
/// (used by tests and as an escape hatch).
pub fn options_path() -> PathBuf {
    std::env::var("BYONK_OPTIONS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_OPTIONS_PATH))
}

/// Read and parse the options file at `path`. Never panics.
pub fn read_options(path: &Path) -> ReadResult {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ReadResult::Missing,
        Err(e) => ReadResult::Malformed(format!("cannot read {}: {e}", path.display())),
        Ok(text) => match serde_json::from_str::<AddonOptions>(&text) {
            Ok(opts) => ReadResult::Parsed(opts),
            Err(e) => ReadResult::Malformed(format!("invalid JSON in {}: {e}", path.display())),
        },
    }
}

/// Map a log-level word to byonk's tracing filter string, or `None` if unknown.
fn level_to_filter(level: &str) -> Option<String> {
    let l = level.trim().to_ascii_lowercase();
    match l.as_str() {
        "trace" | "debug" | "info" | "warn" | "error" => {
            Some(format!("byonk={l},tower_http={l}"))
        }
        _ => None,
    }
}

/// Tracing filter implied by the options, if a valid `log_level` is present.
/// `None` when there is no options file, no `log_level`, or an unknown level.
pub fn log_filter(result: &ReadResult) -> Option<String> {
    match result {
        ReadResult::Parsed(opts) => opts.log_level.as_deref().and_then(level_to_filter),
        _ => None,
    }
}

/// Apply add-on options to the in-memory config. Only a non-empty `admin_token`
/// has an effect — it sets `config.admin.token`. byonk never persists this.
pub fn apply_to_config(result: &ReadResult, config: &mut AppConfig) {
    if let ReadResult::Parsed(opts) = result {
        if let Some(token) = opts.admin_token.as_deref() {
            let token = token.trim();
            if !token.is_empty() {
                config.admin.token = Some(token.to_string());
            }
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib addon_options`
Expected: PASS — all 8 tests green.

- [ ] **Step 6: Lint + format**

Run: `make check`
Expected: fmt clean, clippy clean, full test suite passes.

- [ ] **Step 7: Commit**

```bash
git add src/addon_options.rs src/lib.rs
git commit -m "feat: add HA add-on options reader (parse /data/options.json)"
```

---

### Task 2: Wire the reader into `run_server` + integration test

**Files:**
- Modify: `src/main.rs` (the `run_server()` function — the tracing init block and the `create_app_state` call)
- Modify: `tests/common/app.rs` (add `TestApp::from_config`)
- Create: `tests/addon_options_test.rs`

**Interfaces:**
- Consumes: `byonk::addon_options::{options_path, read_options, log_filter, apply_to_config, ReadResult}` (Task 1); `byonk::models::AppConfig`; `byonk::server::{create_app_state_with_config, build_router}`.
- Produces: `TestApp::from_config(config: AppConfig) -> TestApp` (an admin-capable test app built from an arbitrary config).

- [ ] **Step 1: Add the `TestApp::from_config` helper**

In `tests/common/app.rs`, add this method to the `impl TestApp` block (right after `new_admin`, mirroring its structure). It builds a test app from a caller-supplied config:

```rust
    /// Build a test app from an arbitrary config (embedded assets, in-memory).
    pub fn from_config(config: AppConfig) -> Self {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state_with_config(asset_loader, Arc::new(config))
            .expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        Self {
            router,
            registry,
            content_cache,
        }
    }
```

(`AppConfig`, `AssetLoader`, `Arc`, `create_app_state_with_config`, `build_router` are already imported at the top of `app.rs`.)

- [ ] **Step 2: Write the failing integration test**

Create `tests/addon_options_test.rs`:

```rust
//! Integration: HA add-on options.json feeds the admin token into the running server.

mod common;

use axum::http::StatusCode;
use byonk::addon_options::{apply_to_config, read_options};
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use common::TestApp;
use std::sync::Arc;

/// Build a TestApp as `run_server` would: read an options.json, apply it to the
/// freshly loaded config, then build the app.
fn app_with_options(json: &str) -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("options.json");
    std::fs::write(&path, json).expect("write options");
    let result = read_options(&path);

    let loader = Arc::new(AssetLoader::new(None, None, None));
    let mut config = AppConfig::load_from_assets(&loader).expect("load config");
    apply_to_config(&result, &mut config);
    TestApp::from_config(config)
}

#[tokio::test]
async fn options_token_activates_admin_api() {
    let app = app_with_options(r#"{"admin_token":"secret","log_level":"info"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
}

#[tokio::test]
async fn options_token_rejects_wrong_bearer() {
    let app = app_with_options(r#"{"admin_token":"secret"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer nope")])
        .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn blank_options_token_keeps_admin_dormant() {
    let app = app_with_options(r#"{"admin_token":"","log_level":"info"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --test addon_options_test`
Expected: FAIL to compile — `no function or associated item named from_config` until Step 1 is saved; if Step 1 is already saved, the tests compile and PASS at the library level (they exercise Task 1 + the new helper, not `main.rs`). That is fine — they lock the behavior. Proceed to wire `run_server` so production matches.

- [ ] **Step 4: Wire the reader into `run_server()`**

In `src/main.rs`, inside `run_server()`, replace the tracing-init block:

```rust
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "byonk=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
```

with:

```rust
    // Read HA add-on options (no-op outside the add-on). Read before tracing init
    // so `log_level` can shape the default filter; defer the warning until logging is up.
    let addon = byonk::addon_options::read_options(&byonk::addon_options::options_path());
    let default_filter = byonk::addon_options::log_filter(&addon)
        .unwrap_or_else(|| "byonk=debug,tower_http=debug".to_string());

    // Initialize tracing (RUST_LOG env wins; else add-on log_level; else default)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if let byonk::addon_options::ReadResult::Malformed(msg) = &addon {
        tracing::warn!("Ignoring add-on options: {msg}");
    }
```

Then, further down in the same function, replace:

```rust
    // Create application state using shared server module
    let state = server::create_app_state(asset_loader)?;
```

with:

```rust
    // Create application state, injecting the add-on admin token (if any) into config.
    // Explicit BYONK_ADMIN_TOKEN env still wins (server resolves env before config.admin.token).
    let mut config = byonk::models::AppConfig::load_from_assets(&asset_loader)?;
    byonk::addon_options::apply_to_config(&addon, &mut config);
    let state = server::create_app_state_with_config(asset_loader, std::sync::Arc::new(config))?;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test addon_options_test`
Expected: PASS — all three tests green.

- [ ] **Step 6: Lint + format + full suite**

Run: `make check`
Expected: fmt clean, clippy clean, all tests pass (the existing `tests/admin_*` suites still green — `run_server` is unchanged in behavior when no options file exists).

- [ ] **Step 7: Commit**

```bash
git add src/main.rs tests/common/app.rs tests/addon_options_test.rs
git commit -m "feat: apply HA add-on options at serve startup"
```

---

### Task 3: Add-on package + manifest validation test

**Files:**
- Create: `repository.yaml` (repo root)
- Create: `homeassistant/byonk/config.yaml`
- Create: `homeassistant/byonk/DOCS.md`
- Create: `homeassistant/byonk/CHANGELOG.md`
- Create: `homeassistant/byonk/translations/en.yaml`
- Create: `tests/addon_manifest_test.rs`

**Interfaces:**
- Consumes: nothing from byonk runtime. The manifest test uses `serde_yaml` (already a dependency) and `CARGO_MANIFEST_DIR` to locate files.
- Produces: the installable add-on package; a test guarding its invariants.

**Note on branding assets:** `icon.png`/`logo.png` are optional for a working add-on (the store shows a default). They are out of scope for this task; the add-on installs and runs without them. Mention them as a follow-up in DOCS if desired.

- [ ] **Step 1: Create `repository.yaml` at the repo root**

```yaml
name: Byonk Add-ons
url: https://github.com/oetiker/byonk
maintainer: Tobias Oetiker <tobi@oetiker.ch>
```

- [ ] **Step 2: Create `homeassistant/byonk/config.yaml`**

```yaml
name: Byonk
version: "0.15.0"
slug: byonk
description: Self-hosted content server for TRMNL e-ink devices
url: https://github.com/oetiker/byonk/tree/main/homeassistant/byonk
image: ghcr.io/oetiker/byonk
arch:
  - amd64
  - aarch64
init: true
ports:
  3000/tcp: 3000
ports_description:
  3000/tcp: TRMNL device + admin API access (point your device here)
map:
  - addon_config:rw
environment:
  CONFIG_FILE: /config/config.yaml
  SCREENS_DIR: /config/screens
  FONTS_DIR: /config/fonts
  BIND_ADDR: 0.0.0.0:3000
options:
  admin_token: ""
  log_level: info
schema:
  admin_token: "password?"
  log_level: "list(trace|debug|info|warn|error)"
```

- [ ] **Step 3: Create `homeassistant/byonk/translations/en.yaml`**

```yaml
configuration:
  admin_token:
    name: Admin token
    description: >-
      Managed automatically by the Byonk Home Assistant integration. Leave this
      blank — you do not need to set it by hand. While empty, the management API
      stays disabled.
  log_level:
    name: Log level
    description: Verbosity of the Byonk server log.
```

- [ ] **Step 4: Create `homeassistant/byonk/DOCS.md`**

```markdown
# Byonk

Self-hosted content server for TRMNL e-ink devices. This add-on runs the prebuilt
`ghcr.io/oetiker/byonk` image under Home Assistant Supervisor.

## Installation

1. In Home Assistant, go to **Settings → Add-ons → Add-on Store**.
2. Open the **⋮** menu (top right) → **Repositories**, add
   `https://github.com/oetiker/byonk`, and close.
3. Find **Byonk** in the store and click **Install**, then **Start**.

## Pointing your TRMNL device at Byonk

The add-on publishes Byonk on host port **3000**. Configure your TRMNL device to
use `http://<your-home-assistant-host>:3000` as its server.

## Configuration

- **Admin token** — leave blank. It is managed automatically by the Byonk Home
  Assistant integration (a later release). While blank, the management API is
  disabled (this does not affect serving screens to devices).
- **Log level** — server log verbosity (default `info`).

## Editing screens and config

Your configuration, screens, and fonts live in the add-on's config folder
(mapped to `/config` inside the add-on). Edit them with the **File editor** or
**Studio Code Server** add-on. Empty folders are seeded with sensible defaults on
first start.

Changes to device→screen mappings are best made through the Byonk integration
once it is available; manual edits to `config.yaml` are also picked up without a
restart.
```

- [ ] **Step 5: Create `homeassistant/byonk/CHANGELOG.md`**

```markdown
# Changelog

## 0.15.0

- Initial Home Assistant add-on for Byonk.
- Runs the prebuilt `ghcr.io/oetiker/byonk` image.
- Persistent, editable config/screens/fonts under the add-on config folder.
- Publishes host port 3000 for TRMNL devices.
- Reads `admin_token` and `log_level` from the add-on options.
```

- [ ] **Step 6: Write the failing manifest test**

Create `tests/addon_manifest_test.rs`:

```rust
//! Validates the HA add-on package manifest stays consistent with the design.

use serde_yaml::Value;
use std::path::Path;

fn load_yaml(rel: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn repository_yaml_has_required_keys() {
    let repo = load_yaml("repository.yaml");
    assert!(repo.get("name").and_then(Value::as_str).is_some(), "name");
    assert!(repo.get("url").and_then(Value::as_str).is_some(), "url");
}

#[test]
fn addon_config_matches_design() {
    let cfg = load_yaml("homeassistant/byonk/config.yaml");

    assert_eq!(cfg["slug"].as_str(), Some("byonk"));
    assert_eq!(cfg["image"].as_str(), Some("ghcr.io/oetiker/byonk"));

    // version must be a concrete, non-empty image tag
    assert!(
        cfg["version"].as_str().map(|v| !v.is_empty()).unwrap_or(false),
        "version must be a non-empty string"
    );

    // arch includes amd64 + aarch64
    let arch: Vec<&str> = cfg["arch"]
        .as_sequence()
        .expect("arch seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        arch.contains(&"amd64") && arch.contains(&"aarch64"),
        "arch={arch:?}"
    );

    // ports maps 3000/tcp -> 3000
    assert_eq!(cfg["ports"]["3000/tcp"].as_u64(), Some(3000));

    // editable persistent config
    let map: Vec<&str> = cfg["map"]
        .as_sequence()
        .expect("map seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(map.contains(&"addon_config:rw"), "map={map:?}");

    // environment wires byonk's paths + bind
    assert_eq!(cfg["environment"]["CONFIG_FILE"].as_str(), Some("/config/config.yaml"));
    assert_eq!(cfg["environment"]["SCREENS_DIR"].as_str(), Some("/config/screens"));
    assert_eq!(cfg["environment"]["FONTS_DIR"].as_str(), Some("/config/fonts"));
    assert_eq!(cfg["environment"]["BIND_ADDR"].as_str(), Some("0.0.0.0:3000"));

    // exactly the two intended options exist in the schema
    assert!(cfg["schema"].get("admin_token").is_some(), "schema.admin_token");
    assert!(cfg["schema"].get("log_level").is_some(), "schema.log_level");
    assert!(
        cfg["schema"].get("registration").is_none()
            && cfg["schema"].get("default_screen").is_none(),
        "schema must not duplicate integration-owned settings"
    );
}
```

- [ ] **Step 7: Run the test to verify it fails, then passes**

Run: `cargo test --test addon_manifest_test`
Expected (before Steps 1–2 files exist): FAIL with a `read ...` panic. After Steps 1–5 are saved: PASS.

If you are doing Steps 1–5 before Step 6 (files already exist), run the test and confirm it PASSES; to see the red state, temporarily rename `homeassistant/byonk/config.yaml` and re-run (it should panic on read), then restore it.

- [ ] **Step 8: Lint + format + full suite**

Run: `make check`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add repository.yaml homeassistant/ tests/addon_manifest_test.rs
git commit -m "feat: add Home Assistant add-on package"
```

---

### Task 4: User documentation + changelog

**Files:**
- Create: `docs/src/guide/ha-addon.md`
- Modify: `docs/src/SUMMARY.md`
- Modify: `CHANGES.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Create the mdBook page `docs/src/guide/ha-addon.md`**

```markdown
# Home Assistant Add-on

Byonk can run as a Home Assistant Supervisor add-on. The add-on runs the same
prebuilt `ghcr.io/oetiker/byonk` image, stores its configuration in a persistent,
editable folder, and exposes Byonk on a host port so your TRMNL devices can reach
it directly on your LAN.

> Requires a Supervisor-managed install (Home Assistant OS or Supervised).

## Install

1. **Settings → Add-ons → Add-on Store**.
2. **⋮ → Repositories**, add `https://github.com/oetiker/byonk`, then close.
3. Open **Byonk** in the store, **Install**, then **Start**.

## Point your TRMNL device at Byonk

The add-on publishes Byonk on host port **3000**. Set your TRMNL device's server
to `http://<your-home-assistant-host>:3000`.

## Options

| Option | Default | Notes |
|--------|---------|-------|
| `admin_token` | *(blank)* | **Leave blank.** Managed automatically by the Byonk integration (a later release). While blank, the management API is disabled — serving screens is unaffected. |
| `log_level` | `info` | Server log verbosity (`trace`/`debug`/`info`/`warn`/`error`). |

## Configuration, screens, and fonts

The add-on maps an editable, persistent folder to `/config` inside the container,
holding `config.yaml`, `screens/`, and `fonts/`. Edit these with the **File
editor** or **Studio Code Server** add-on. Empty folders are seeded with the
embedded defaults on first start. Edits to `config.yaml` are applied without a
restart.

## How it relates to the Byonk integration

A companion Home Assistant **integration** (shipping in a later release) manages
device→screen mappings and global settings from the Home Assistant UI and
establishes trust with Byonk automatically — you will not need to copy or set any
token by hand.
```

- [ ] **Step 2: Wire the page into `docs/src/SUMMARY.md`**

Change the Getting Started list so the new page follows Installation:

```markdown
# Getting Started

- [Installation](guide/installation.md)
- [Home Assistant Add-on](guide/ha-addon.md)
- [Configuration](guide/configuration.md)
- [Dev Mode](guide/dev-mode.md)
```

- [ ] **Step 3: Add the changelog entry**

In `CHANGES.md`, under `## Unreleased` → `### New`, add this bullet immediately after the `### New` line:

```markdown
- **Home Assistant add-on**: run Byonk as a Supervisor add-on (references the
  prebuilt `ghcr.io/oetiker/byonk` image) with persistent, editable
  config/screens/fonts and a host port for TRMNL devices. Byonk reads
  `admin_token` and `log_level` from the add-on options (`/data/options.json`);
  the admin token stays the single source of truth, dormant until provisioned by
  the forthcoming Byonk integration. See *Getting Started → Home Assistant Add-on*.
```

- [ ] **Step 4: Build the docs**

Run: `make docs`
Expected: mdBook build succeeds with no broken-link or missing-file errors.

- [ ] **Step 5: Commit**

```bash
git add docs/src/guide/ha-addon.md docs/src/SUMMARY.md CHANGES.md
git commit -m "docs: Home Assistant add-on guide + changelog"
```

---

## Final verification

- [ ] Run `make check` — fmt, clippy, and the full test suite (including `addon_options_test` and `addon_manifest_test`) pass.
- [ ] Run `make docs` — clean build.
- [ ] Confirm `git status` is clean and the four task commits are present.
- [ ] Spot-check the acceptance criteria in the spec §12 against the implementation.
```
