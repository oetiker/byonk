# The Byonk app installs its own integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Home Assistant user installs the Byonk app and nothing else; the app writes its own integration into the Home Assistant config directory, asks for one restart, and shows up as a Discovered card. HACS is removed from the project.

**Architecture:** The release image already contains `custom_components/byonk` (added here). At startup in add-on mode, byonk copies that directory into `<ha_config>/custom_components/byonk`, posts a persistent notification through the Supervisor's Core API proxy, and posts a Supervisor discovery message. Home Assistant loads the integration on the next restart and offers a `hassio` config flow. The integration raises a repair issue whenever the running app is a different version from the loaded integration.

**Tech Stack:** Rust (axum, reqwest, serde_json, tempfile for tests), Python (Home Assistant custom integration, pytest-homeassistant-custom-component), mdBook docs, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-19-addon-installs-integration-design.md`

## Global Constraints

- Branch: `feat/addon-installs-integration`, cut from `main` at `958b14f`.
- **Never `git add -A` or `git add .`** in this repository — untracked local files get swept in. Add by explicit path and check `git diff --cached` before committing.
- Rust: `make check` (fmt + clippy + tests) must pass. Rust edition and toolchain come from `rust-toolchain.toml`; do not add cargo to mise.
- Python: `make ha-check` (ruff + `pytest tests_ha`) must pass. Run `make ha-setup` once first if `.venv` is missing.
- `CHANGES.md` entries describe **user-visible** changes only. No CI, tooling or process notes.
- The version in `Cargo.toml`, `custom_components/byonk/manifest.json` and `homeassistant/byonk/config.yaml` must stay identical — `.github/workflows/release-publisher.yml:64-66` fails the release otherwise. Do not bump any of them in this branch.
- Mount paths, fixed by Supervisor: `addon_config:rw` → `/config` (already used by byonk), `homeassistant_config:rw` → `/homeassistant`.
- Comments, identifiers and documentation in English.
- Commit at the end of every task, with the message given in the task.

---

### Task 1: The app manifest and image carry the integration

Declares the three Supervisor capabilities the later tasks need, and puts the
integration files into the release image.

**Files:**
- Modify: `homeassistant/byonk/config.yaml`
- Modify: `Dockerfile.release`
- Test: `tests/addon_manifest_test.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: the image path `/app/custom_components/byonk`, which Task 2 reads; the `homeassistant_config:rw` mount at `/homeassistant`, which Task 2 writes to; `homeassistant_api: true` and `discovery: [byonk]`, which Task 3 uses.

- [ ] **Step 1: Write the failing test**

Append to `tests/addon_manifest_test.rs`:

```rust
#[test]
fn addon_config_declares_integration_install_capabilities() {
    let cfg = load_yaml("homeassistant/byonk/config.yaml");

    // The app writes custom_components/byonk into the HA config dir, which
    // Supervisor mounts at /homeassistant for this mapping.
    let map: Vec<&str> = cfg["map"]
        .as_sequence()
        .expect("map seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        map.contains(&"homeassistant_config:rw"),
        "map must grant write access to the HA config dir, got {map:?}"
    );

    // Needed to POST the "restart Home Assistant" notification through
    // /core/api on the Supervisor proxy.
    assert_eq!(cfg["homeassistant_api"].as_bool(), Some(true));

    // Supervisor rejects a discovery message for a service the app does not
    // list here (supervisor/api/discovery.py::set_discovery).
    let discovery: Vec<&str> = cfg["discovery"]
        .as_sequence()
        .expect("discovery seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(discovery, vec!["byonk"]);
}
```

- [ ] **Step 2: Run the test and watch it fail**

Run: `cargo test --test addon_manifest_test addon_config_declares_integration_install_capabilities`
Expected: FAIL — `map must grant write access to the HA config dir`.

- [ ] **Step 3: Add the three keys to the app manifest**

In `homeassistant/byonk/config.yaml`, replace the `map:` block and add two keys
after it:

```yaml
map:
  - addon_config:rw
  - homeassistant_config:rw
homeassistant_api: true
discovery:
  - byonk
```

Leave every other key untouched. In particular do **not** add `hassio_api` or
`hassio_role` — nothing here needs them, and `hassio_role` is the only one of
these that would lower the security rating.

- [ ] **Step 4: Put the integration into the release image**

`Dockerfile.release` has two near-identical arch stages. Add the same line to
**both**, directly after the `COPY ${BINARY_DIR}/<arch>/byonk /app/byonk` line:

```dockerfile
# The app installs this into the Home Assistant config dir at startup, so the
# integration always matches the server that wrote it.
COPY custom_components/byonk ./custom_components/byonk
```

The build context is the repository root, so no other change is needed.

- [ ] **Step 5: Run the test and watch it pass**

Run: `cargo test --test addon_manifest_test`
Expected: PASS, all tests in the file.

- [ ] **Step 6: Commit**

```bash
git add homeassistant/byonk/config.yaml Dockerfile.release tests/addon_manifest_test.rs
git commit -m "feat(addon): let the app write the HA config dir and ship the integration"
```

---

### Task 2: `ha_integration` — decide and write

The filesystem half: work out whether the integration on disk is current, and
replace it safely if not. No HTTP yet.

**Files:**
- Create: `src/ha_integration.rs`
- Modify: `src/lib.rs`
- Test: `tests/ha_integration_test.rs`

**Interfaces:**
- Consumes: `/app/custom_components/byonk` from Task 1.
- Produces, all `pub`:
  - `fn ha_config_dir() -> PathBuf`
  - `fn integration_src() -> PathBuf`
  - `enum InstallOutcome { NotNeeded, Installed { from: Option<String>, to: String }, Refused(String), Failed(String) }`
  - `fn install(src: &Path, ha_config: &Path) -> InstallOutcome`
  Task 3 adds HTTP functions to the same module; Task 4 calls `install`.

- [ ] **Step 1: Write the failing tests**

Create `tests/ha_integration_test.rs`:

```rust
//! Tests for writing byonk's HA integration into the Home Assistant config dir.
//!
//! This code deletes a directory inside the user's Home Assistant configuration,
//! so the refusal cases matter as much as the happy path.

use std::path::Path;

use byonk::ha_integration::{install, InstallOutcome};
use tempfile::TempDir;

/// Build a fake image-side integration source: manifest plus one extra file,
/// so a test can tell a real copy from a manifest-only one.
fn make_src(dir: &Path, version: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(r#"{{"domain": "byonk", "name": "Byonk", "version": "{version}"}}"#),
    )
    .unwrap();
    std::fs::write(dir.join("__init__.py"), "# byonk\n").unwrap();
    std::fs::create_dir_all(dir.join("brand")).unwrap();
    std::fs::write(dir.join("brand/icon.png"), b"not-really-a-png").unwrap();
}

fn target_of(ha_config: &Path) -> std::path::PathBuf {
    ha_config.join("custom_components").join("byonk")
}

#[test]
fn writes_the_integration_when_none_is_installed() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(&outcome, InstallOutcome::Installed { from: None, to } if to == "0.18.0"),
        "got {outcome:?}"
    );
    let target = target_of(ha.path());
    assert!(target.join("__init__.py").is_file());
    assert!(target.join("brand/icon.png").is_file(), "subdirectories are copied");
}

#[test]
fn does_nothing_when_the_installed_version_matches() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");
    install(src_dir.path(), ha.path());

    // A file the copy would not have produced; it must survive a no-op.
    let marker = target_of(ha.path()).join("__pycache__marker");
    std::fs::write(&marker, "x").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(matches!(outcome, InstallOutcome::NotNeeded), "got {outcome:?}");
    assert!(marker.is_file(), "a no-op must not touch the directory");
}

#[test]
fn replaces_the_integration_when_the_version_differs() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");
    install(src_dir.path(), ha.path());
    let stale = target_of(ha.path()).join("gone.py");
    std::fs::write(&stale, "old").unwrap();

    make_src(src_dir.path(), "0.19.0");
    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(&outcome, InstallOutcome::Installed { from: Some(f), to } if f == "0.18.0" && to == "0.19.0"),
        "got {outcome:?}"
    );
    assert!(!stale.exists(), "a replaced install must not keep stale files");
    let manifest =
        std::fs::read_to_string(target_of(ha.path()).join("manifest.json")).unwrap();
    assert!(manifest.contains("0.19.0"));
}

#[test]
fn refuses_a_directory_that_is_not_byonks() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let target = target_of(ha.path());
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("manifest.json"),
        r#"{"domain": "something_else", "version": "1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(target.join("precious.py"), "keep me").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(matches!(outcome, InstallOutcome::Refused(_)), "got {outcome:?}");
    assert_eq!(
        std::fs::read_to_string(target.join("precious.py")).unwrap(),
        "keep me",
        "a refusal must change nothing"
    );
}

#[test]
fn refuses_a_directory_with_no_manifest_at_all() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let target = target_of(ha.path());
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("random.txt"), "who knows").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(matches!(outcome, InstallOutcome::Refused(_)), "got {outcome:?}");
    assert!(target.join("random.txt").is_file());
}

#[test]
fn reports_failure_when_the_config_dir_is_absent() {
    let src_dir = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let outcome = install(src_dir.path(), Path::new("/no/such/ha/config"));

    assert!(matches!(outcome, InstallOutcome::Failed(_)), "got {outcome:?}");
}

#[test]
fn reports_failure_when_the_image_has_no_integration() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(matches!(outcome, InstallOutcome::Failed(_)), "got {outcome:?}");
    assert!(!target_of(ha.path()).exists());
}

#[test]
fn clears_a_leftover_staging_dir_from_a_crashed_run() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let staging = ha.path().join("custom_components").join(".byonk-new");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("junk.py"), "half-written").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(matches!(outcome, InstallOutcome::Installed { .. }), "got {outcome:?}");
    assert!(!staging.exists(), "staging is consumed by the swap");
    assert!(!target_of(ha.path()).join("junk.py").exists());
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test --test ha_integration_test`
Expected: FAIL to compile — `unresolved import byonk::ha_integration`.

- [ ] **Step 3: Write the module**

Create `src/ha_integration.rs`:

```rust
//! Installs byonk's Home Assistant integration into the Home Assistant config
//! directory.
//!
//! Byonk ships as a Supervisor app, and the app is the only thing a user
//! installs. The integration half therefore travels inside the app image
//! (`Dockerfile.release` copies `custom_components/byonk` to
//! `/app/custom_components/byonk`) and is written out here, into the config dir
//! Supervisor mounts at `/homeassistant`. Home Assistant reads
//! `custom_components/` only when it starts, so a restart is still needed; the
//! caller asks for one (see `install_and_announce`).
//!
//! Everything here is best effort. A failure logs and lets the server run.

use std::path::{Path, PathBuf};

/// Where Supervisor mounts the Home Assistant config dir for the
/// `homeassistant_config` mapping (`supervisor/docker/const.py`).
const DEFAULT_HA_CONFIG_DIR: &str = "/homeassistant";

/// Where `Dockerfile.release` puts the integration inside the image.
const DEFAULT_INTEGRATION_SRC: &str = "/app/custom_components/byonk";

/// Staging directory name, a sibling of the target inside `custom_components/`.
const STAGING_NAME: &str = ".byonk-new";

/// Home Assistant config dir. `BYONK_HA_CONFIG_DIR` overrides it (tests, and an
/// escape hatch), mirroring `addon_options::options_path`.
pub fn ha_config_dir() -> PathBuf {
    std::env::var("BYONK_HA_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_HA_CONFIG_DIR))
}

/// The integration source inside the image. `BYONK_INTEGRATION_SRC` overrides it.
pub fn integration_src() -> PathBuf {
    std::env::var("BYONK_INTEGRATION_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_INTEGRATION_SRC))
}

/// What `install` did, so the caller can decide whether to tell the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// The installed integration is already this version.
    NotNeeded,
    /// Written. `from` is the version that was there before, if any.
    Installed { from: Option<String>, to: String },
    /// The target exists and is not byonk's. Nothing was touched.
    Refused(String),
    /// Something went wrong. Nothing usable was written.
    Failed(String),
}

/// Read one top-level string field out of a `manifest.json` in `dir`.
fn manifest_field(dir: &Path, field: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get(field)?.as_str().map(str::to_string)
}

/// Copy `src` into `dst` recursively, creating `dst`.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Write the integration from `src` into `<ha_config>/custom_components/byonk`.
///
/// The swap deletes a directory inside the user's Home Assistant config, so it
/// happens only when the target does not exist, or its `manifest.json` names
/// byonk's own domain. Nothing else under `ha_config` is read, walked or
/// removed.
pub fn install(src: &Path, ha_config: &Path) -> InstallOutcome {
    let Some(ours) = manifest_field(src, "version") else {
        return InstallOutcome::Failed(format!(
            "no readable manifest.json in {}",
            src.display()
        ));
    };

    let custom_components = ha_config.join("custom_components");
    let target = custom_components.join("byonk");
    let installed = manifest_field(&target, "version");

    if installed.as_deref() == Some(ours.as_str()) {
        return InstallOutcome::NotNeeded;
    }

    // Refusal guard. An existing target must identify itself as byonk's.
    if target.exists() && manifest_field(&target, "domain").as_deref() != Some("byonk") {
        return InstallOutcome::Refused(format!(
            "{} exists but is not byonk's integration; leaving it alone",
            target.display()
        ));
    }

    if !ha_config.is_dir() {
        return InstallOutcome::Failed(format!(
            "Home Assistant config dir {} is not available",
            ha_config.display()
        ));
    }

    // Stage the new copy beside the target, then swap. A crashed earlier run can
    // leave staging behind; the name is ours, so clearing it is safe.
    let staging = custom_components.join(STAGING_NAME);
    if let Err(e) = std::fs::create_dir_all(&custom_components) {
        return InstallOutcome::Failed(format!("could not create {}: {e}", custom_components.display()));
    }
    if staging.exists() {
        if let Err(e) = std::fs::remove_dir_all(&staging) {
            return InstallOutcome::Failed(format!("could not clear {}: {e}", staging.display()));
        }
    }
    if let Err(e) = copy_dir(src, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return InstallOutcome::Failed(format!("could not stage the integration: {e}"));
    }
    if target.exists() {
        if let Err(e) = std::fs::remove_dir_all(&target) {
            let _ = std::fs::remove_dir_all(&staging);
            return InstallOutcome::Failed(format!("could not replace {}: {e}", target.display()));
        }
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        let _ = std::fs::remove_dir_all(&staging);
        return InstallOutcome::Failed(format!("could not move the integration into place: {e}"));
    }

    InstallOutcome::Installed {
        from: installed,
        to: ours,
    }
}
```

Register the module in `src/lib.rs`, keeping the list alphabetical — insert
between `pub mod error;` and `pub mod mcp;`:

```rust
pub mod ha_integration;
```

- [ ] **Step 4: Run the tests and watch them pass**

Run: `cargo test --test ha_integration_test`
Expected: PASS, 8 tests.

- [ ] **Step 5: Check formatting and lints**

Run: `make check`
Expected: clean fmt, no clippy warnings, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/ha_integration.rs src/lib.rs tests/ha_integration_test.rs
git commit -m "feat(addon): write the HA integration into the Home Assistant config dir"
```

---

### Task 3: Tell Supervisor and the user

The two HTTP calls: a persistent notification asking for the restart, and the
discovery message that produces the Discovered card.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/ha_integration.rs`
- Test: `tests/ha_integration_http_test.rs`

**Interfaces:**
- Consumes: `homeassistant_api: true` and `discovery: [byonk]` from Task 1.
- Produces, all `pub`:
  - `fn supervisor_url() -> String`
  - `async fn notify_restart(client: &reqwest::Client, token: &str, from: Option<&str>, to: &str) -> anyhow::Result<()>`
  - `async fn announce_discovery(client: &reqwest::Client, token: &str) -> anyhow::Result<()>`
  Task 4 calls both.

- [ ] **Step 1: Write the failing tests**

Create `tests/ha_integration_http_test.rs`:

```rust
//! Tests for the two Supervisor calls the app makes after installing the
//! integration. A tiny local axum server stands in for Supervisor and records
//! what arrived.

use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::post, Json, Router};
use byonk::ha_integration::{announce_discovery, notify_restart, supervisor_url};
use serde_json::Value;

type Captured = Arc<Mutex<Vec<(String, Option<String>, Value)>>>;

async fn record(
    State(seen): State<Captured>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<Value>,
) -> &'static str {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    seen.lock().unwrap().push((uri.path().to_string(), auth, body));
    "{}"
}

/// Start a fake Supervisor on a free port. Returns its base URL and the log.
async fn fake_supervisor() -> (String, Captured) {
    let seen: Captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/discovery", post(record))
        .route(
            "/core/api/services/persistent_notification/create",
            post(record),
        )
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), seen)
}

#[tokio::test]
async fn announces_discovery_for_the_byonk_service() {
    let (url, seen) = fake_supervisor().await;
    // SAFETY: single-threaded test body; the var is read on the next line.
    unsafe { std::env::set_var("BYONK_SUPERVISOR_URL", &url) };
    assert_eq!(supervisor_url(), url);

    let client = reqwest::Client::new();
    announce_discovery(&client, "super-secret").await.unwrap();

    let seen = seen.lock().unwrap();
    let (path, auth, body) = seen.first().expect("one request");
    assert_eq!(path, "/discovery");
    assert_eq!(auth.as_deref(), Some("Bearer super-secret"));
    assert_eq!(body["service"], "byonk");
    assert!(body["config"].is_object(), "config must be present, may be empty");
}

#[tokio::test]
async fn first_install_notification_says_finish_setup() {
    let (url, seen) = fake_supervisor().await;
    unsafe { std::env::set_var("BYONK_SUPERVISOR_URL", &url) };

    let client = reqwest::Client::new();
    notify_restart(&client, "tok", None, "0.18.0").await.unwrap();

    let seen = seen.lock().unwrap();
    let (path, _, body) = seen.first().expect("one request");
    assert_eq!(path, "/core/api/services/persistent_notification/create");
    assert_eq!(body["notification_id"], "byonk_integration");
    let message = body["message"].as_str().unwrap();
    assert!(message.contains("0.18.0"), "message names the version: {message}");
    assert!(
        message.to_lowercase().contains("restart"),
        "message asks for a restart: {message}"
    );
}

#[tokio::test]
async fn update_notification_names_both_versions() {
    let (url, seen) = fake_supervisor().await;
    unsafe { std::env::set_var("BYONK_SUPERVISOR_URL", &url) };

    let client = reqwest::Client::new();
    notify_restart(&client, "tok", Some("0.18.0"), "0.19.0")
        .await
        .unwrap();

    let seen = seen.lock().unwrap();
    let message = seen.first().unwrap().2["message"].as_str().unwrap().to_string();
    assert!(message.contains("0.18.0") && message.contains("0.19.0"), "{message}");
}
```

> Note on `unsafe { std::env::set_var }`: Rust 2024 makes `set_var` unsafe. Each
> of these tests is its own `#[tokio::test]`, but cargo runs them on threads in
> one process, so they all write the same variable. They set it to a per-test
> URL and only read it through their own call, which is why they cannot observe
> each other's value in a way that changes an assertion. If this proves flaky,
> pass the base URL as a parameter instead of reading the env var — do not add a
> sleep.

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test --test ha_integration_http_test`
Expected: FAIL to compile — `announce_discovery` and `notify_restart` do not exist.

- [ ] **Step 3: Enable reqwest's JSON bodies**

`RequestBuilder::json` is behind reqwest's `json` feature, which byonk does not
currently enable. In `Cargo.toml`, add it to the existing reqwest line:

```toml
reqwest = { version = "0.13", default-features = false, features = ["blocking", "rustls", "query", "json"] }
```

`serde_json` is already a dependency, so this pulls in nothing new of substance.

- [ ] **Step 4: Add the HTTP half to the module**

Append to `src/ha_integration.rs`:

```rust
/// Supervisor's in-container base URL. `BYONK_SUPERVISOR_URL` overrides it.
const DEFAULT_SUPERVISOR_URL: &str = "http://supervisor";

/// Notification id, so a repeated notification replaces the previous one
/// instead of stacking up.
const NOTIFICATION_ID: &str = "byonk_integration";

/// Base URL for Supervisor API calls.
pub fn supervisor_url() -> String {
    std::env::var("BYONK_SUPERVISOR_URL").unwrap_or_else(|_| DEFAULT_SUPERVISOR_URL.to_string())
}

/// Ask the user to restart Home Assistant, through Supervisor's Core API proxy.
///
/// Reachable because the app declares `homeassistant_api: true`; the proxy
/// blocks only `hassio*` paths (`supervisor/api/proxy.py`).
pub async fn notify_restart(
    client: &reqwest::Client,
    token: &str,
    from: Option<&str>,
    to: &str,
) -> anyhow::Result<()> {
    let message = match from {
        Some(previous) => format!(
            "Byonk updated its Home Assistant integration from {previous} to {to}. \
             Restart Home Assistant to load it."
        ),
        None => format!(
            "Byonk installed its Home Assistant integration ({to}). \
             Restart Home Assistant to finish setting up Byonk."
        ),
    };
    client
        .post(format!(
            "{}/core/api/services/persistent_notification/create",
            supervisor_url()
        ))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "notification_id": NOTIFICATION_ID,
            "title": "Byonk",
            "message": message,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Tell Supervisor the byonk service is here, which makes Home Assistant offer
/// a Discovered card for the integration.
///
/// Supervisor stores the message and dedupes on (app, service), so calling this
/// on every start is harmless; if Home Assistant is down the message is kept and
/// replayed when it starts (`supervisor/discovery/__init__.py`).
pub async fn announce_discovery(client: &reqwest::Client, token: &str) -> anyhow::Result<()> {
    client
        .post(format!("{}/discovery", supervisor_url()))
        .bearer_auth(token)
        .json(&serde_json::json!({ "service": "byonk", "config": {} }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test --test ha_integration_http_test`
Expected: PASS, 3 tests.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/ha_integration.rs tests/ha_integration_http_test.rs
git commit -m "feat(addon): ask for the restart and announce byonk to Supervisor"
```

---

### Task 4: Run it at startup

Joins Tasks 2 and 3 into one entry point and calls it from the server.

**Files:**
- Modify: `src/ha_integration.rs`
- Modify: `src/main.rs:895-898`
- Test: `tests/ha_integration_http_test.rs`

**Interfaces:**
- Consumes: `install`, `notify_restart`, `announce_discovery`.
- Produces: `pub async fn install_and_announce()`, called from `run_server`.

- [ ] **Step 1: Write the failing test**

Append to `tests/ha_integration_http_test.rs`:

```rust
#[tokio::test]
async fn install_and_announce_notifies_once_then_only_announces() {
    use byonk::ha_integration::install_and_announce;

    let (url, seen) = fake_supervisor().await;
    let src = tempfile::TempDir::new().unwrap();
    let ha = tempfile::TempDir::new().unwrap();
    std::fs::write(
        src.path().join("manifest.json"),
        r#"{"domain": "byonk", "version": "0.18.0"}"#,
    )
    .unwrap();

    unsafe {
        std::env::set_var("BYONK_SUPERVISOR_URL", &url);
        std::env::set_var("BYONK_INTEGRATION_SRC", src.path());
        std::env::set_var("BYONK_HA_CONFIG_DIR", ha.path());
        std::env::set_var("SUPERVISOR_TOKEN", "tok");
    }

    install_and_announce().await;
    install_and_announce().await;

    let paths: Vec<String> = seen.lock().unwrap().iter().map(|r| r.0.clone()).collect();
    assert_eq!(
        paths.iter().filter(|p| p.contains("persistent_notification")).count(),
        1,
        "the restart notification is posted only when something changed: {paths:?}"
    );
    assert_eq!(
        paths.iter().filter(|p| *p == "/discovery").count(),
        2,
        "discovery is announced on every start: {paths:?}"
    );
}
```

- [ ] **Step 2: Run the test and watch it fail**

Run: `cargo test --test ha_integration_http_test install_and_announce`
Expected: FAIL to compile — `install_and_announce` does not exist.

- [ ] **Step 3: Write the entry point**

Append to `src/ha_integration.rs`:

```rust
/// Install the integration if needed, then announce byonk to Supervisor.
///
/// Called once per start in add-on mode. Discovery is announced every time, not
/// only after a write: the Discovered card must also appear on the restart that
/// follows a first install, when there is nothing left to write.
pub async fn install_and_announce() {
    let src = integration_src();
    let ha_config = ha_config_dir();
    let outcome = install(&src, &ha_config);

    match &outcome {
        InstallOutcome::NotNeeded => {
            tracing::debug!("Home Assistant integration is already up to date");
        }
        InstallOutcome::Installed { from: Some(previous), to } => {
            tracing::info!("Updated the Home Assistant integration {previous} -> {to}");
        }
        InstallOutcome::Installed { from: None, to } => {
            tracing::info!("Installed the Home Assistant integration {to}");
        }
        InstallOutcome::Refused(why) => tracing::warn!("{why}"),
        InstallOutcome::Failed(why) => {
            tracing::warn!("Could not install the Home Assistant integration: {why}")
        }
    }

    let Ok(token) = std::env::var("SUPERVISOR_TOKEN") else {
        tracing::warn!("No SUPERVISOR_TOKEN; skipping the Home Assistant notification");
        return;
    };
    let client = reqwest::Client::new();

    if let InstallOutcome::Installed { from, to } = &outcome {
        if let Err(e) = notify_restart(&client, &token, from.as_deref(), to).await {
            tracing::warn!("Could not post the restart notification: {e}");
        }
    }

    if let Err(e) = announce_discovery(&client, &token).await {
        tracing::warn!("Could not announce byonk to Supervisor: {e}");
    }
}
```

- [ ] **Step 4: Call it from the server**

In `src/main.rs`, immediately after the `state.addon_mode = …` line in
`run_server` (around line 897), add:

```rust
    // In add-on mode byonk owns its Home Assistant integration: write it into
    // the HA config dir and announce ourselves. Spawned so a slow or wedged
    // Supervisor can never delay the listener.
    if state.addon_mode {
        tokio::spawn(byonk::ha_integration::install_and_announce());
    }
```

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test --test ha_integration_http_test`
Expected: PASS, 4 tests.

- [ ] **Step 6: Full check**

Run: `make check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/ha_integration.rs src/main.rs tests/ha_integration_http_test.rs
git commit -m "feat(addon): install and announce the integration on startup"
```

---

### Task 5: The Discovered card

The integration accepts the Supervisor discovery message, so the user clicks
**Configure** on a card instead of hunting for **Add Integration**.

**Files:**
- Modify: `custom_components/byonk/config_flow.py:85-112`
- Modify: `custom_components/byonk/strings.json`
- Modify: `custom_components/byonk/translations/en.json`
- Test: `tests_ha/test_config_flow.py`

**Interfaces:**
- Consumes: the discovery message from Task 3.
- Produces: config-flow steps `hassio` and `hassio_confirm`; a private helper `_async_create_hub_entry` shared with `async_step_user`.

- [ ] **Step 1: Write the failing tests**

Append to `tests_ha/test_config_flow.py`:

```python
async def _start_hassio(hass):
    from homeassistant.helpers.service_info.hassio import HassioServiceInfo

    return await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": config_entries.SOURCE_HASSIO},
        data=HassioServiceInfo(
            config={}, name="Byonk", slug="abcd1234_byonk", uuid="u-1"
        ),
    )


async def test_supervisor_discovery_offers_a_confirm_form(hass):
    result = await _start_hassio(hass)
    assert result["type"] == FlowResultType.FORM
    assert result["step_id"] == "hassio_confirm"


async def test_supervisor_discovery_creates_the_hub_entry(hass):
    with (
        patch(
            "custom_components.byonk.config_flow.async_ensure_addon_installed",
            new=AsyncMock(return_value="abcd1234_byonk"),
        ),
        patch(
            "custom_components.byonk.config_flow.async_read_token",
            new=AsyncMock(return_value="tok"),
        ),
        patch(
            "custom_components.byonk.config_flow.async_get_base_url",
            new=AsyncMock(return_value="http://addon:3000"),
        ),
        patch(
            "custom_components.byonk.config_flow.ByonkClient.async_get_config",
            new=AsyncMock(return_value={}),
        ),
    ):
        result = await _start_hassio(hass)
        result = await hass.config_entries.flow.async_configure(result["flow_id"], {})

    assert result["type"] == FlowResultType.CREATE_ENTRY
    assert result["data"] == {
        "addon_slug": "abcd1234_byonk",
        CONF_BASE_URL: "http://addon:3000",
    }


async def test_supervisor_discovery_aborts_when_already_configured(hass):
    from tests_ha.conftest import make_hub_entry

    make_hub_entry(hass)
    result = await _start_hassio(hass)
    assert result["type"] == FlowResultType.ABORT
    assert result["reason"] == "single_instance_allowed"
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `make ha-check`
Expected: FAIL — the flow has no `hassio` step (`UnknownStep`).

- [ ] **Step 3: Factor the shared tail out of `async_step_user`**

In `custom_components/byonk/config_flow.py`, replace the body of
`async_step_user` (lines 85-112) with a guard plus a call, and add the helper
above it:

```python
    async def _async_create_hub_entry(self) -> ConfigFlowResult:
        """Install and start the app, provision the token, create the hub entry.

        Shared by the manual route (`user`) and the Supervisor discovery route
        (`hassio`), which differ only in how they are triggered.
        """
        try:
            slug = await async_ensure_addon_installed(self.hass)
            token = await async_read_token(self.hass, slug)
            if not token:
                token = await async_provision_token(self.hass, slug)
            base_url = await async_get_base_url(self.hass, slug)
        except AddonError:
            return self.async_abort(reason="addon_error")

        # Provisioning restarts the add-on; byonk's HTTP comes back up a moment
        # later. If it never answers, abort cleanly rather than raising.
        if not await _async_probe_ready(self.hass, base_url, token):
            return self.async_abort(reason="addon_unhealthy")

        await self.async_set_unique_id(DOMAIN)
        return self.async_create_entry(
            title="Byonk",
            data={CONF_ADDON_SLUG: slug, CONF_BASE_URL: base_url},
        )

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if self._hub_entry() is not None:
            return self.async_abort(reason="single_instance_allowed")
        if not is_hassio(self.hass):
            return self.async_abort(reason="not_hassio")
        return await self._async_create_hub_entry()
```

- [ ] **Step 4: Add the two discovery steps**

Directly after `async_step_user`, add:

```python
    async def async_step_hassio(
        self, discovery_info: HassioServiceInfo
    ) -> ConfigFlowResult:
        """The Byonk app told Supervisor it is running.

        The app writes this integration into the config dir and announces
        itself, so this is the normal way a user reaches setup: a Discovered
        card in Settings > Devices & Services.
        """
        if self._hub_entry() is not None:
            return self.async_abort(reason="single_instance_allowed")
        self.context["title_placeholders"] = {"name": discovery_info.name}
        return await self.async_step_hassio_confirm()

    async def async_step_hassio_confirm(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is None:
            return self.async_show_form(
                step_id="hassio_confirm", data_schema=vol.Schema({})
            )
        return await self._async_create_hub_entry()
```

Add the import beside the other `homeassistant` imports at the top of the file:

```python
from homeassistant.helpers.service_info.hassio import HassioServiceInfo
```

- [ ] **Step 5: Add the user-visible strings**

In `custom_components/byonk/strings.json`, inside `config` → `step`, add a
`hassio_confirm` entry beside `configure`:

```json
   "hassio_confirm": {
    "title": "Set up Byonk",
    "description": "The Byonk app is running. Set up Byonk to onboard your TRMNL devices and control their screens from Home Assistant."
   },
```

Copy the same block into the matching place in
`custom_components/byonk/translations/en.json`.

- [ ] **Step 6: Run the tests and watch them pass**

Run: `make ha-check`
Expected: PASS, including the three new tests and every pre-existing one — the
refactor must not change `async_step_user` behaviour.

- [ ] **Step 7: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/test_config_flow.py
git commit -m "feat(ha): offer Byonk as a Supervisor-discovered integration"
```

---

### Task 6: Warn when the app is ahead of the integration

**Files:**
- Modify: `custom_components/byonk/addon.py`
- Modify: `custom_components/byonk/coordinator.py:84-124`
- Modify: `custom_components/byonk/strings.json`
- Modify: `custom_components/byonk/translations/en.json`
- Modify: `tests_ha/conftest.py:67-106`
- Test: `tests_ha/test_version_mismatch.py`

**Interfaces:**
- Consumes: `ByonkCoordinator.slug`, already set in `__init__`.
- Produces: `async_get_addon_version(hass, slug) -> str | None` in `addon.py`; repair issue id `version_mismatch`; a new `addon_version` field on the `byonk` test fixture's state object.

> **Read this before starting.** The coordinator's `_async_update_data` runs in
> every existing HA test through the `byonk` fixture. Adding an unpatched
> Supervisor call there would break all of them, because there is no Supervisor
> in the test harness. That is why Step 1 extends the fixture *before* the
> feature exists.

- [ ] **Step 1: Give the fixture an app version**

In `tests_ha/conftest.py`, add these imports at the top, beside the existing ones:

```python
import json
from pathlib import Path
```

Add a module-level constant after `PNG_1PX`:

```python
# The version the integration really ships, so the default fixture state has the
# app and the integration in agreement and no test sees a spurious repair issue.
INTEGRATION_VERSION = json.loads(
    Path("custom_components/byonk/manifest.json").read_text()
)["version"]
```

In the `byonk` fixture, add one field to the `SimpleNamespace(...)` state, after
`get_device_preview=...`:

```python
        addon_version=INTEGRATION_VERSION,
```

and add one more `patch` to the `with (...)` block, beside the existing
`patch("custom_components.byonk.async_read_token", ...)`:

```python
        patch(
            "custom_components.byonk.coordinator.async_get_addon_version",
            new=AsyncMock(side_effect=lambda *a, **k: state.addon_version),
        ),
```

- [ ] **Step 2: Write the failing tests**

Create `tests_ha/test_version_mismatch.py`:

```python
"""The app rewrites custom_components/byonk whenever it starts, but Home
Assistant reads that directory only at boot. After an app update the running
integration is a version behind until the user restarts, so say so."""
from homeassistant.components.hassio import AddonError
from homeassistant.helpers import issue_registry as ir

from custom_components.byonk.const import DOMAIN
from tests_ha.conftest import INTEGRATION_VERSION, make_hub_entry

ISSUE_ID = "version_mismatch"


async def _setup(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_no_issue_when_versions_agree(hass, byonk):
    await _setup(hass)
    assert ir.async_get(hass).async_get_issue(DOMAIN, ISSUE_ID) is None


async def test_issue_raised_and_cleared(hass, byonk):
    byonk.addon_version = "999.0.0"
    hub = await _setup(hass)

    reg = ir.async_get(hass)
    issue = reg.async_get_issue(DOMAIN, ISSUE_ID)
    assert issue is not None
    assert issue.severity == ir.IssueSeverity.WARNING
    assert issue.translation_placeholders == {
        "addon": "999.0.0",
        "integration": INTEGRATION_VERSION,
    }

    # The user restarts Home Assistant; the app and the integration now agree.
    byonk.addon_version = INTEGRATION_VERSION
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert reg.async_get_issue(DOMAIN, ISSUE_ID) is None


async def test_supervisor_failure_is_silent(hass, byonk, monkeypatch):
    """A Supervisor hiccup must not invent a version mismatch."""
    from unittest.mock import AsyncMock, patch

    with patch(
        "custom_components.byonk.coordinator.async_get_addon_version",
        new=AsyncMock(side_effect=AddonError("supervisor unavailable")),
    ):
        await _setup(hass)
    assert ir.async_get(hass).async_get_issue(DOMAIN, ISSUE_ID) is None
```

- [ ] **Step 3: Run the tests and watch them fail**

Run: `make ha-check`
Expected: FAIL — `custom_components.byonk.coordinator` has no attribute
`async_get_addon_version`. Every other test in `tests_ha` still passes.

- [ ] **Step 4: Expose the app version**

Append to `custom_components/byonk/addon.py`:

```python
async def async_get_addon_version(hass: HomeAssistant, slug: str) -> str | None:
    """Return the installed add-on version, or None if Supervisor won't say."""
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    return info.version
```

- [ ] **Step 5: Compare and raise the issue**

In `custom_components/byonk/coordinator.py`, add the imports beside the existing
`homeassistant` and relative imports:

```python
from homeassistant.components.hassio import AddonError
from homeassistant.loader import async_get_integration

from .addon import async_get_addon_version
```

Add the method to `ByonkCoordinator`, next to `_async_reconcile_repo_issues`:

```python
    async def _async_check_version(self) -> None:
        """Warn when the running app and the loaded integration disagree.

        The app rewrites custom_components/byonk whenever it starts, so after an
        app update the files on disk are new while Home Assistant is still
        running the integration it loaded at boot. One restart fixes it.
        """
        try:
            addon_version = await async_get_addon_version(self.hass, self.slug)
        except AddonError:
            return  # supervisor hiccup: stay quiet rather than guess

        # Read the version through the loader, never from manifest.json: the
        # loader hands back the manifest as read when Home Assistant started,
        # which is the code actually running. The file on disk has already been
        # overwritten by the app, so it would always look like a match.
        integration = await async_get_integration(self.hass, DOMAIN)
        loaded_version = str(integration.version) if integration.version else None

        if addon_version and loaded_version and addon_version != loaded_version:
            ir.async_create_issue(
                self.hass,
                DOMAIN,
                "version_mismatch",
                is_fixable=False,
                severity=ir.IssueSeverity.WARNING,
                translation_key="version_mismatch",
                translation_placeholders={
                    "addon": addon_version,
                    "integration": loaded_version,
                },
            )
        else:
            ir.async_delete_issue(self.hass, DOMAIN, "version_mismatch")
```

Call it at the end of `_async_update_data`, after
`self._async_reconcile_repo_issues(data)` and before `return data`:

```python
        await self._async_check_version()
```

- [ ] **Step 6: Add the issue text**

In `custom_components/byonk/strings.json`, inside `issues`, beside
`screen_repo_error`:

```json
  "version_mismatch": {
   "title": "Restart Home Assistant to finish updating Byonk",
   "description": "The Byonk app is version {addon}, but the Byonk integration Home Assistant is running is version {integration}.\n\nThe app has already installed the matching integration; Home Assistant loads it on the next restart. Everything keeps working in the meantime."
  }
```

Copy the same block into `custom_components/byonk/translations/en.json`.

- [ ] **Step 7: Run the tests and watch them pass**

Run: `make ha-check`
Expected: PASS, the whole suite.

- [ ] **Step 8: Commit**

```bash
git add custom_components/byonk/addon.py custom_components/byonk/coordinator.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json tests_ha/conftest.py tests_ha/test_version_mismatch.py
git commit -m "feat(ha): flag an app newer than the loaded integration"
```

---

### Task 7: Remove HACS

**Files:**
- Delete: `hacs.json`
- Modify: `.github/workflows/ci.yml:101-102`
- Modify: `tests_ha/test_manifest.py:21`
- Modify: `docs/superpowers/ha-publishing.md`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing. Tasks 5, 6 and 8 do not depend on this task, so it may be done in any order relative to them.

- [ ] **Step 1: Delete the HACS test**

Remove `test_hacs_json_parses` from `tests_ha/test_manifest.py` (the function
starting at line 21, and the `hacs.json` reference inside it). Leave every other
test in the file alone.

- [ ] **Step 2: Run the tests and watch them pass**

Run: `make ha-check`
Expected: PASS. The file still tests `manifest.json`.

- [ ] **Step 3: Delete `hacs.json`**

```bash
git rm hacs.json
```

- [ ] **Step 4: Drop the HACS CI job**

In `.github/workflows/ci.yml`, delete lines 101-107 in full — the step, its
`with:` block and its `env:` block:

```yaml
      - name: HACS validation
        uses: hacs/action@main
        with:
          category: integration
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Then fix the comment at line 10, which currently reads:

```yaml
# (build/test/lint, hassfest, and hacs/action's API reads). No job writes.
```

to:

```yaml
# (build/test/lint and hassfest). No job writes.
```

Leave the `hassfest` step exactly as it is — Home Assistant's own manifest
validation still applies, and it is now the only check on the integration
manifest.

- [ ] **Step 5: Retarget the publishing runbook**

`docs/superpowers/ha-publishing.md` opens with "Getting byonk into the HACS
default store". Rewrite the title and intro so its subject is the integration
icon, delete every section about the HACS default store, the HACS PR template
and `hacs-bot`, and keep section 1 (brand images) intact — that finding is
independent of HACS and still true. Add one sentence at the top recording that
HACS was dropped on 2026-08-19 and pointing at
`docs/superpowers/specs/2026-08-19-addon-installs-integration-design.md`.

- [ ] **Step 6: Verify nothing still references HACS in live files**

Run:

```bash
grep -rn -i hacs --include='*.yml' --include='*.json' --include='*.py' --include='*.rs' . | grep -v '^./target' | grep -v docs/superpowers/specs | grep -v docs/superpowers/plans
```

Expected: no output. (Files under `docs/superpowers/specs/` and `plans/` are
records of decisions and keep their HACS references.)

- [ ] **Step 7: Commit**

```bash
git add -u hacs.json .github/workflows/ci.yml tests_ha/test_manifest.py docs/superpowers/ha-publishing.md
git commit -m "chore: drop HACS, the app is byonk's distribution channel"
```

---

### Task 8: One Home Assistant page

**Files:**
- Create: `docs/src/guide/home-assistant.md`
- Delete: `docs/src/guide/ha-addon.md`, `docs/src/guide/ha-integration.md`
- Modify: `docs/src/SUMMARY.md:9-10`
- Modify: `README.md:25,32`
- Modify: `homeassistant/byonk/DOCS.md`
- Modify: `CHANGES.md`

**Interfaces:**
- Consumes: the install flow built in Tasks 1-6.
- Produces: nothing code-facing.

- [ ] **Step 1: Find every inbound link**

Run:

```bash
grep -rn "ha-addon\|ha-integration" docs/src README.md homeassistant CHANGES.md
```

Every hit must be updated in this task. Record the list before editing.

- [ ] **Step 2: Write the merged page**

Create `docs/src/guide/home-assistant.md`. Move the prose across; do not rewrite
what already reads well. The full mapping, so nothing is lost and nothing is
invented:

| # | Section in the new page | Source |
|---|---|---|
| 1 | `# Byonk in Home Assistant` — two sentences: Byonk runs as an app, and it brings its own integration | adapted from `ha-addon.md:1-11` plus `ha-integration.md:15-21` (requirements), keeping the "apps were called add-ons before Home Assistant 2026.2" note |
| 2 | `## Install` | **new text, below** — replaces `ha-addon.md:12-30` and `ha-integration.md:22-43` |
| 3 | `## Point your TRMNL device at Byonk` | `ha-addon.md:31-35` |
| 4 | `## Onboarding a device` | `ha-integration.md:85-111` |
| 5 | `## Entities` (hub, Byonk Default, per-device) | `ha-integration.md:44-84` |
| 6 | `## Editing device settings` | `ha-integration.md:112-126` |
| 7 | `## Screen preview` | `ha-integration.md:127-175` |
| 8 | `## Settings` | `ha-addon.md:36-65` (Options table + global configuration) |
| 9 | `## Configuration, screens and fonts` | `ha-addon.md:66-84` |
| 10 | `## Screen repo cache persistence` | `ha-addon.md:85-97` |
| 11 | `## Monitoring screen repos` | `ha-integration.md:176-201` |
| 12 | `## Re-authentication` | `ha-integration.md:202-207` |
| 13 | `## Upgrading from an earlier install` | **new text, below** |

`ha-addon.md:98-107`, "How it relates to the Byonk integration", is **deleted,
not moved**. That section exists only to explain the split this task removes.

The two new sections, verbatim:

`## Install`:

   ```markdown
   1. In Home Assistant, go to **Settings → Apps → App store**.
   2. Open the **⋮** menu, choose **Repositories**, add
      `https://github.com/oetiker/byonk` and select **Add**.
   3. Find **Byonk** in the store, select **Install**, then **Start**.
   4. Byonk asks you to restart Home Assistant. Do that
      (**Settings → System → Restart**).
   5. After the restart, a **Byonk** card is waiting in
      **Settings → Devices & Services**. Select **Configure**.

   That is the whole setup. Byonk generates its own management token, and no
   token or password is ever asked of you.
   ```

`## Upgrading from an earlier install`:

   ```markdown
   If you installed Byonk through HACS before version 0.19.0, remove it from
   HACS. The Byonk app now keeps the integration up to date by itself, and HACS
   would otherwise offer you a second, competing copy. Nothing else is needed —
   your devices and settings are unaffected.
   ```

The words *app* and *integration* appear only where the reader must click on
one. Do not carry over the "A full Home Assistant setup is **two parts** …
Install both." opening from `ha-addon.md:13-15`, or any HACS install steps.

- [ ] **Step 3: Delete the old pages and fix the navigation**

```bash
git rm docs/src/guide/ha-addon.md docs/src/guide/ha-integration.md
```

In `docs/src/SUMMARY.md`, replace the two lines

```markdown
- [Home Assistant App](guide/ha-addon.md)
- [Home Assistant Integration](guide/ha-integration.md)
```

with

```markdown
- [Home Assistant](guide/home-assistant.md)
```

Then update every link found in Step 1 to point at `guide/home-assistant.md`
(plus the right anchor).

- [ ] **Step 4: Update the README and the in-app docs**

`README.md:25,32` — replace the HACS custom-repository steps with the same five
steps as the docs page, and drop the "two parts" framing at line 25.

`homeassistant/byonk/DOCS.md` — this is what a user reads inside Home Assistant,
on the app's Documentation tab, so it matters most. Replace lines 13 and 23-27
with the install steps minus step 1-3 (the reader has already installed the
app): start it, restart Home Assistant, configure the card.

- [ ] **Step 5: Add the changelog entry**

In `CHANGES.md`, under `## Unreleased`, add user-facing lines only:

```markdown
- Installing Byonk in Home Assistant now takes one step: install the Byonk app
  from the app store and it sets up its own integration. HACS is no longer
  used or needed. If you installed Byonk through HACS before, remove it there.
- Home Assistant shows a repair notice when the Byonk app has been updated but
  Home Assistant has not been restarted yet, so the integration is still the
  older version.
```

- [ ] **Step 6: Build the docs**

Run: `make docs`
Expected: builds with no broken-link warnings.

- [ ] **Step 7: Commit**

```bash
git add docs/src/guide/home-assistant.md docs/src/SUMMARY.md README.md homeassistant/byonk/DOCS.md CHANGES.md
git add -u docs/src/guide/ha-addon.md docs/src/guide/ha-integration.md
git commit -m "docs: one Home Assistant page, written for someone installing Byonk"
```

---

### Task 9: Prove it on the VM

Nothing before this proves the chain end to end. Unit tests cannot show that
Supervisor accepts the manifest, that the mount lands where we think, or that
the card appears.

**Files:**
- Modify: `docs/HANDOVER.md` (record the result)

**Interfaces:**
- Consumes: everything above.
- Produces: a pass/fail record.

- [ ] **Step 1: Read the VM workflow**

Read `.claude/skills/ha-vm-testing/SKILL.md` in full and follow it. Build the
app from source as `local_byonk` — the published image will not contain this
work until a release. Remember that `make ha-rebuild` does **not** sync the app
manifest: this branch changes `homeassistant/byonk/config.yaml`, so the VM needs
a manual version bump plus `POST /store/reload` and `ha addons update`, or the
new `map`, `homeassistant_api` and `discovery` keys will not be live.

- [ ] **Step 2: Work through the checklist on a clean VM**

Confirm each of these, and write down what you actually saw:

1. The app installs and starts with the new manifest (no Supervisor schema error).
2. `/homeassistant/custom_components/byonk/` exists after the first start and
   its `manifest.json` version matches the app version.
3. A persistent notification appears in Home Assistant asking for a restart.
4. After restarting Home Assistant, a **Byonk** Discovered card appears in
   **Settings → Devices & Services**.
5. **Configure** on that card creates the hub entry, with no token prompt.
6. The hub, **Byonk Default**, and any TRMNL devices produce their usual entities.
7. Restart the app on its own: the log says the integration is already up to
   date, no second notification appears, and the integration keeps working.
8. Put a foreign `custom_components/byonk/` in place (a `manifest.json` with a
   different `domain`, plus a marker file), restart the app, and confirm the log
   refuses and the marker file survives.
9. Simulate an update: edit the version in the installed
   `custom_components/byonk/manifest.json` to something older, restart the app,
   and confirm the update notification appears and the repair issue shows in
   **Settings → Repairs**. Restart Home Assistant and confirm the issue clears.

- [ ] **Step 3: Record the result**

Rewrite `docs/HANDOVER.md` for this branch: the initiative, branch and HEAD,
what passed, anything that did not, and what remains. Overwrite it, do not
append.

- [ ] **Step 4: Commit**

```bash
git add docs/HANDOVER.md
git commit -m "docs(handover): record the VM validation of app-installed integration"
```

---

## Notes for the reviewer

- Task 2's `install` is the only code in byonk that deletes anything in a
  user's Home Assistant configuration. The refusal tests are the point of that
  task; if a change makes them pass for the wrong reason, the safety is gone.
- Task 5 refactors `async_step_user`. Every pre-existing test in
  `tests_ha/test_config_flow.py` must still pass untouched — that is the check
  that the refactor changed nothing.
- The two Rust test files both write process-wide environment variables. If they
  interfere once the suite grows, pass the values as parameters rather than
  serialising the tests.
