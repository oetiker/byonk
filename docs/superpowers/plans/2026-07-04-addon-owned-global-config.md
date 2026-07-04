# Add-on-owned Global Config Implementation Plan (Plan A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move byonk's server-global config (settings + package registry) into the HA add-on Options form (`options.json` → byonk), make those writes read-only over the admin API when byonk runs as an add-on, and revert the HA integration's Plan-3 config-write UI to read-only monitoring + operational controls.

**Architecture:** byonk gains an `addon_mode` flag (true iff `/data/options.json` was present at startup). In add-on mode, `addon_options` parses global settings + the package list from `options.json` and applies them to the in-memory `AppConfig`; global-config admin writes (`PATCH /settings`, package add/patch/delete) return HTTP 409, while per-device writes and package **content refresh** (`POST /packages/update`) stay live. The add-on manifest exposes the new options. The HA integration reverts its package subentry flows, subentry reconcile, delete-propagation listener, and global Options Flow — keeping the package status sensors, the Update-packages button, the registration switch, and per-device entities.

**Tech Stack:** Rust (axum, serde, serde_yaml, arc-swap), Python (Home Assistant custom component, pytest, ruff), HAOS add-on YAML manifest.

## Global Constraints

- **Standalone byonk is unchanged.** With no `/data/options.json` present (`ReadResult::Missing`/`Malformed`), byonk behaves exactly as today: `config.yaml` is the full read/write source and the admin API is fully writable. Add-on mode is purely additive and gated solely on the options file's presence.
- **`addon_options` guarantees are preserved:** the module never writes the options file, never generates or logs a token, and is a no-op when the file is absent.
- **Token redaction preserved:** `GET /packages` (`PackageInfo`) must never serialize a package token — only `token_set: bool`. Package tokens live in `options.json` (a `password?` field) and are read by byonk only for git auth.
- **Per-device writes stay live in add-on mode.** Only *global* config (settings + package **registry** mutations) becomes read-only; `PATCH/POST/DELETE /devices*` and `POST /packages[/:handle]/update` (content refresh) remain allowed.
- **Verify Rust with** `make check` (fmt + clippy `-D warnings` + tests). **Verify Python with** `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`.
- **No `git add -A`/`.`** — stage explicit paths only; verify `git diff --cached` before each commit.

---

## File Structure

**byonk server (Rust):**
- `src/server.rs` — add `addon_mode: bool` to `AppState`; default it `false` in the constructor.
- `src/main.rs` — set `state.addon_mode` from the startup `ReadResult`.
- `src/addon_options.rs` — parse `auth_mode`, `package_refresh_interval`, `packages[]`; apply them to `AppConfig` in add-on mode.
- `src/api/admin/write.rs` — add a `require_writable_global` guard; call it in the four global-config write handlers.

**Add-on manifest:**
- `homeassistant/byonk/config.yaml` — extend `options:`/`schema:` with the new keys.

**HA integration (Python):**
- `custom_components/byonk/config_flow.py` — remove the package subentry flow + the global Options Flow + their registration hooks.
- `custom_components/byonk/coordinator.py` — remove `_reconcile_packages` + `_pkg_handles`; keep the package fetch.
- `custom_components/byonk/__init__.py` — remove the delete-propagation update listener + its seeding.
- `custom_components/byonk/strings.json` + `translations/en.json` — remove the reverted UI strings.
- `tests_ha/` — delete the four revert-target test files; trim `test_api.py`.

**Docs:**
- `CHANGES.md`, `docs/src/` — record the redirection.

---

## Task 1: Thread `addon_mode` into `AppState`

**Files:**
- Modify: `src/server.rs:67-80` (the `AppState` struct) and `src/server.rs:179-190` (the struct literal in `create_app_state_with_overrides`)
- Modify: `src/main.rs:684-686` (startup wiring)

**Interfaces:**
- Produces: `AppState.addon_mode: bool` — read by later tasks' admin-write guard. `false` by default (standalone); set `true` in `main.rs` when `options.json` was parsed.

- [ ] **Step 1: Add the field to `AppState`**

In `src/server.rs`, inside `pub struct AppState { … }` (lines 67-80), add a field after `package_manager`:

```rust
    pub package_manager: Arc<PackageManager>,
    /// True when byonk started as an HA Supervisor add-on (i.e. `/data/options.json`
    /// was present). In add-on mode, global-config admin writes are read-only.
    pub addon_mode: bool,
```

- [ ] **Step 2: Default it `false` in the constructor**

In `src/server.rs`, in the `Ok(AppState { … })` literal in `create_app_state_with_overrides` (lines 179-190), add:

```rust
        dev_overrides,
        package_manager,
        addon_mode: false,
    })
```

- [ ] **Step 3: Set it from the startup `ReadResult` in `main.rs`**

In `src/main.rs`, change the state construction (lines 685-686) from:

```rust
    byonk::addon_options::apply_to_config(&addon, &mut config);
    let state = server::create_app_state_with_config(asset_loader, config)?;
```

to:

```rust
    byonk::addon_options::apply_to_config(&addon, &mut config);
    let mut state = server::create_app_state_with_config(asset_loader, config)?;
    // Add-on mode = the options file was present and parsed. Gates global-config
    // admin writes to read-only (the add-on Options form is the sole editor).
    state.addon_mode = matches!(addon, byonk::addon_options::ReadResult::Parsed(_));
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build`
Expected: builds clean (no other callers of `create_app_state_with_*` change, since the field defaults inside the constructor).

- [ ] **Step 5: Commit**

```bash
git add src/server.rs src/main.rs
git commit -m "feat(addon): thread addon_mode flag into AppState"
```

---

## Task 2: Parse global settings + packages from `options.json`

**Files:**
- Modify: `src/addon_options.rs` (the `AddonOptions` struct, `apply_to_config`, and the `#[cfg(test)]` module)

**Interfaces:**
- Consumes: `AppConfig` fields `auth_mode: String`, `package_refresh_interval: u64`, `packages: HashMap<String, PackageRef>`, where `PackageRef { repo: Option<String>, pin: Option<String>, token: Option<String> }` (from `src/models/config.rs`).
- Produces: extended `apply_to_config` that, when `Parsed`, overrides those three `AppConfig` fields from the options file (in addition to the existing `admin.token` behavior).

- [ ] **Step 1: Write the failing tests**

In `src/addon_options.rs`, inside `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn parses_settings_and_packages_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(
            &path,
            r#"{
                "admin_token":"secret",
                "auth_mode":"ed25519",
                "package_refresh_interval":900,
                "packages":[
                    {"handle":"disttest","repo":"https://example.com/x.git","pin":"main","token":"gh_x"},
                    {"handle":"nopin","repo":"https://example.com/y.git"}
                ]
            }"#,
        )
        .unwrap();
        match read_options(&path) {
            ReadResult::Parsed(opts) => {
                assert_eq!(opts.auth_mode.as_deref(), Some("ed25519"));
                assert_eq!(opts.package_refresh_interval, Some(900));
                assert_eq!(opts.packages.len(), 2);
                assert_eq!(opts.packages[0].handle, "disttest");
                assert_eq!(opts.packages[0].pin.as_deref(), Some("main"));
                assert_eq!(opts.packages[1].pin, None);
                assert_eq!(opts.packages[1].token, None);
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn apply_overrides_settings_and_builds_package_map() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: Some("t".to_string()),
            log_level: None,
            auth_mode: Some("ed25519".to_string()),
            package_refresh_interval: Some(600),
            packages: vec![AddonPackage {
                handle: "disttest".to_string(),
                repo: Some("https://example.com/x.git".to_string()),
                pin: Some("main".to_string()),
                token: Some("gh_x".to_string()),
            }],
        });
        let mut config = embedded_config();
        apply_to_config(&r, &mut config);
        assert_eq!(config.auth_mode, "ed25519");
        assert_eq!(config.package_refresh_interval, 600);
        let pkg = config.packages.get("disttest").expect("package present");
        assert_eq!(pkg.repo.as_deref(), Some("https://example.com/x.git"));
        assert_eq!(pkg.pin.as_deref(), Some("main"));
        assert_eq!(pkg.token.as_deref(), Some("gh_x"));
    }

    #[test]
    fn apply_absent_settings_leave_config_defaults() {
        // A parsed options file that omits the new keys must not clobber config
        // defaults (only admin_token is authoritative-on-absence).
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: None,
            auth_mode: None,
            package_refresh_interval: None,
            packages: vec![],
        });
        let mut config = embedded_config();
        config.auth_mode = "api_key".to_string();
        config.package_refresh_interval = 42;
        apply_to_config(&r, &mut config);
        assert_eq!(config.auth_mode, "api_key", "absent auth_mode keeps config value");
        assert_eq!(
            config.package_refresh_interval, 42,
            "absent interval keeps config value"
        );
        assert!(config.packages.is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib addon_options::tests -- --nocapture`
Expected: FAIL — `AddonOptions` has no `auth_mode`/`package_refresh_interval`/`packages` fields and `AddonPackage` is undefined.

- [ ] **Step 3: Extend the `AddonOptions` DTO + add `AddonPackage`**

In `src/addon_options.rs`, replace the `AddonOptions` struct (lines 16-22) with:

```rust
/// One package entry as it appears in the add-on options `packages:` list.
/// The handle is a field here (HAOS list rows are flat objects); byonk stores
/// packages keyed by handle, so `apply_to_config` folds these into a map.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddonPackage {
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub pin: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

/// Subset of the add-on options byonk consumes. Unknown keys are ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddonOptions {
    #[serde(default)]
    pub admin_token: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub package_refresh_interval: Option<u64>,
    #[serde(default)]
    pub packages: Vec<AddonPackage>,
}
```

- [ ] **Step 4: Import `PackageRef` and extend `apply_to_config`**

In `src/addon_options.rs`, change the import at line 13 from:

```rust
use crate::models::AppConfig;
```

to:

```rust
use crate::models::{AppConfig, PackageRef};
```

(If `PackageRef` is not re-exported from `crate::models`, use `use crate::models::config::PackageRef;` instead — verify against `src/models/mod.rs`.)

Then replace the body of `apply_to_config` (lines 101-110) with:

```rust
pub fn apply_to_config(result: &ReadResult, config: &mut AppConfig) {
    if let ReadResult::Parsed(opts) = result {
        // admin_token stays authoritative (non-empty sets, blank/absent clears).
        config.admin.token = opts
            .admin_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);

        // The remaining global settings override config only when present in
        // options.json; absent keys leave the config value untouched.
        if let Some(mode) = opts.auth_mode.as_deref() {
            let mode = mode.trim();
            if !mode.is_empty() {
                config.auth_mode = mode.to_string();
            }
        }
        if let Some(interval) = opts.package_refresh_interval {
            config.package_refresh_interval = interval;
        }

        // In add-on mode the package registry comes from options.json. An empty
        // list means "no packages"; only replace the map when a list is present.
        if !opts.packages.is_empty() {
            config.packages = opts
                .packages
                .iter()
                .filter(|p| !p.handle.trim().is_empty())
                .map(|p| {
                    (
                        p.handle.trim().to_string(),
                        PackageRef {
                            repo: p.repo.clone(),
                            pin: p.pin.clone(),
                            token: p.token.clone(),
                        },
                    )
                })
                .collect();
        }
    }
}
```

- [ ] **Step 5: Update the existing test literals for the new fields**

The existing tests build `AddonOptions { admin_token, log_level }` literals (e.g. lines 166-175, 179-185, 188-197, 204-227, 245-267). Each now needs the three new fields. Add `auth_mode: None, package_refresh_interval: None, packages: vec![]` to every `AddonOptions { … }` literal in the test module.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --lib addon_options::tests -- --nocapture`
Expected: PASS (all, including the three new tests).

- [ ] **Step 7: Commit**

```bash
git add src/addon_options.rs
git commit -m "feat(addon): parse global settings + package list from options.json"
```

---

## Task 3: Read-only gate for global-config writes in add-on mode

**Files:**
- Modify: `src/api/admin/write.rs` — add `require_writable_global`; call it in `patch_settings`, `add_package`, `patch_package`, `delete_package`; add a `#[cfg(test)]` guard test.

**Interfaces:**
- Consumes: `AppState.addon_mode` (Task 1).
- Produces: global-config write handlers return `ApiError::Conflict` (HTTP 409) when `addon_mode` is true. Per-device handlers and `update_package`/`update_all_packages` are **not** gated.

- [ ] **Step 1: Write the failing test**

In `src/api/admin/write.rs`, add at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetLoader;
    use crate::models::AppConfig;
    use crate::server::create_app_state_with_config;

    fn state_with_addon_mode(addon_mode: bool) -> AppState {
        let loader = AssetLoader::new(None, None, None);
        let config = AppConfig::load_from_assets(&loader).expect("load embedded config");
        let mut state = create_app_state_with_config(std::sync::Arc::new(loader), config)
            .expect("create app state");
        state.addon_mode = addon_mode;
        state
    }

    #[test]
    fn global_writes_rejected_in_addon_mode() {
        let state = state_with_addon_mode(true);
        match require_writable_global(&state) {
            Err(ApiError::Conflict(_)) => {}
            other => panic!("expected Conflict in add-on mode, got {other:?}"),
        }
    }

    #[test]
    fn global_writes_allowed_standalone() {
        let state = state_with_addon_mode(false);
        assert!(require_writable_global(&state).is_ok());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib api::admin::write::tests -- --nocapture`
Expected: FAIL — `require_writable_global` is not defined.

- [ ] **Step 3: Add the guard**

In `src/api/admin/write.rs`, immediately after `require_file_config` (ends line 39), add:

```rust
/// Guard: global-config registry/settings writes are read-only when byonk runs
/// as an HA add-on. The add-on Options form (`/data/options.json`) is the sole
/// editor for global config; per-device writes and package content-refresh are
/// unaffected.
fn require_writable_global(state: &AppState) -> Result<(), ApiError> {
    if state.addon_mode {
        return Err(ApiError::Conflict(
            "global config is read-only in add-on mode; edit it in the byonk add-on Configuration tab".into(),
        ));
    }
    Ok(())
}
```

- [ ] **Step 4: Call the guard in the four global-config handlers**

In each of `patch_settings` (line 252), `add_package` (line 353), `patch_package` (line 404), and `delete_package` (line 457), add the guard as the **first** line of the function body, immediately before the existing `require_admin(&state, &headers)?;`:

```rust
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
```

Do **not** add the guard to `add_device`, `patch_device`, `delete_device`, `update_package`, or `update_all_packages`.

- [ ] **Step 5: Run the guard test to verify it passes**

Run: `cargo test --lib api::admin::write::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Run the full check**

Run: `make check`
Expected: fmt clean, clippy clean (`-D warnings`), all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/api/admin/write.rs
git commit -m "feat(addon): make global-config admin writes read-only in add-on mode"
```

---

## Task 4: Extend the add-on Options manifest schema

**Files:**
- Modify: `homeassistant/byonk/config.yaml` (the `options:` and `schema:` blocks, lines 23-28)

**Interfaces:**
- Produces: the HAOS Configuration tab renders `auth_mode`, `package_refresh_interval`, and a repeatable `packages` list — Supervisor writes them into `/data/options.json`, which Task 2's reader consumes.

- [ ] **Step 1: Extend `options:` and `schema:`**

In `homeassistant/byonk/config.yaml`, replace the `options:`/`schema:` blocks (lines 23-28) with:

```yaml
options:
  admin_token: ""
  log_level: info
  auth_mode: api_key
  package_refresh_interval: 0
  packages: []
schema:
  admin_token: "password?"
  log_level: "list(trace|debug|info|warn|error)"
  auth_mode: "list(api_key|ed25519)"
  package_refresh_interval: "int(0,)"
  packages:
    - handle: "str"
      repo: "str"
      pin: "str?"
      token: "password?"
```

- [ ] **Step 2: Verify the manifest is valid YAML and carries the new keys**

Run:
```bash
python3 -c "import yaml,sys; d=yaml.safe_load(open('homeassistant/byonk/config.yaml')); \
assert d['schema']['auth_mode']=='list(api_key|ed25519)'; \
assert d['schema']['package_refresh_interval']=='int(0,)'; \
assert d['schema']['packages'][0]['handle']=='str'; \
assert d['options']['packages']==[]; print('manifest schema OK')"
```
Expected: prints `manifest schema OK`.

- [ ] **Step 3: Commit**

```bash
git add homeassistant/byonk/config.yaml
git commit -m "feat(addon): expose auth_mode, refresh interval, packages in add-on options"
```

---

## Task 5: Revert the package subentry flow (Plan-3 Tasks 3 & 4)

**Files:**
- Modify: `custom_components/byonk/config_flow.py` (remove the subentry-type hook + the `ByonkPackageSubentryFlow` class + `_package_schema` + now-unused imports)
- Modify: `custom_components/byonk/strings.json` and `custom_components/byonk/translations/en.json` (remove the `config_subentries.package` block)
- Delete: `tests_ha/test_package_subentry_flow.py`

**Interfaces:**
- Produces: the hub config entry advertises **no** subentry types; there is no package add/reconfigure UI. The API client's `async_add_package`/`async_update_package`/`async_delete_package` remain defined (Task 8 trims their tests) but are no longer called by the integration.

- [ ] **Step 1: Remove the subentry-type hook**

In `custom_components/byonk/config_flow.py`, delete the `async_get_supported_subentry_types` classmethod (lines 81-88 per the current tree). No replacement — with the method gone, HA advertises no subentry types.

- [ ] **Step 2: Remove the subentry flow class + its schema helper**

Delete `_package_schema` (lines 285-296) and the entire `ByonkPackageSubentryFlow` class (lines 299-367).

- [ ] **Step 3: Remove now-unused imports**

Remove `ConfigSubentryFlow` and `SubentryFlowResult` from the imports (lines 13, 15). Leave `OptionsFlow`/`BUILTIN_SCREEN_LABEL` for now — Task 7 removes those.

- [ ] **Step 4: Remove the UI strings**

In `custom_components/byonk/strings.json`, delete the `config_subentries.package` block (lines 32-50). Mirror the deletion in `custom_components/byonk/translations/en.json`.

- [ ] **Step 5: Delete the revert-target test**

```bash
git rm tests_ha/test_package_subentry_flow.py
```

- [ ] **Step 6: Verify ruff + the suite**

Run: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`
Expected: ruff clean (no unused-import errors); pytest passes (the subentry-flow tests are gone; no other test references the removed flow).

- [ ] **Step 7: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json
git commit -m "revert(ha): remove package subentry add/reconfigure flow"
```

---

## Task 6: Revert the subentry reconcile + delete-propagation (Plan-3 Tasks 5 & 6)

**Files:**
- Modify: `custom_components/byonk/coordinator.py` (remove `_reconcile_packages`, its invocation, `_pkg_handles`, and now-unused subentry imports; keep the package **fetch**)
- Modify: `custom_components/byonk/__init__.py` (remove the `_async_hub_updated` listener + its registration + seeding + now-unused import)
- Delete: `tests_ha/test_package_reconcile.py`, `tests_ha/test_package_delete_propagation.py`

**Interfaces:**
- Consumes: nothing new.
- Produces: the coordinator still fetches `packages` (feeding the Task-9 status sensors) but no longer creates/updates/removes config subentries; the hub config entry has no update listener that writes to byonk.

- [ ] **Step 1: Remove the reconcile from the coordinator**

In `custom_components/byonk/coordinator.py`:
- Delete the `_reconcile_packages` method (lines 167-212).
- Delete its call site inside `_async_update_data` (line 128: `self._reconcile_packages(data)`).
- Delete the `self._pkg_handles: set[str] = set()` init (line 87).
- Trim the first-refresh comment (lines 118-125) so it no longer describes package-subentry reconcile.
- Remove the `ConfigSubentry` and `ConfigSubentryData` imports (lines 12-13). **Keep** `ConfigEntry` and `SOURCE_INTEGRATION_DISCOVERY` (used elsewhere).

**Keep** the package fetch: the `self.client.async_get_packages()` call in the `asyncio.gather` (line 96), its unpack into `packages` (line 91), and `ByonkData(..., packages=packages)` (line 116) all stay — the status sensors depend on them.

- [ ] **Step 2: Remove the delete-propagation listener**

In `custom_components/byonk/__init__.py`:
- Delete the `_async_hub_updated` function (lines 59-80).
- Delete its registration (line 45: `entry.async_on_unload(entry.add_update_listener(_async_hub_updated))`).
- Delete the `_pkg_handles` seeding block (lines 42-44).
- Remove the `ByonkReadOnlyError` import (line 12) — it was used only by the listener. Verify no other reference remains: `grep -n ByonkReadOnlyError custom_components/byonk/__init__.py` returns nothing.

- [ ] **Step 3: Delete the revert-target tests**

```bash
git rm tests_ha/test_package_reconcile.py tests_ha/test_package_delete_propagation.py
```

- [ ] **Step 4: Verify ruff + the suite**

Run: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`
Expected: ruff clean (no unused imports/vars — confirms `_pkg_handles`, `ConfigSubentry*`, `ByonkReadOnlyError` are fully removed); pytest passes.

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/coordinator.py custom_components/byonk/__init__.py
git commit -m "revert(ha): remove package subentry reconcile + delete propagation"
```

---

## Task 7: Revert the global Options Flow (Plan-3 Task 7)

**Files:**
- Modify: `custom_components/byonk/config_flow.py` (remove `async_get_options_flow` + `ByonkOptionsFlow` + now-unused imports)
- Modify: `custom_components/byonk/strings.json` and `translations/en.json` (remove the `options.step.init` block)
- Delete: `tests_ha/test_options_flow.py`

**Interfaces:**
- Produces: the integration exposes no global settings Options Flow. `registration_screen`/`auth_mode` intentionally lose their integration UI — `auth_mode` now lives in the app Options (Task 4) and the default/onboarding screen moves to the reserved DEFAULT device in **Plan B**. The API client's `async_update_settings` method stays (the registration switch uses it).

- [ ] **Step 1: Remove the options-flow hook + class**

In `custom_components/byonk/config_flow.py`:
- Delete the `async_get_options_flow` staticmethod (lines 90-93).
- Delete the entire `ByonkOptionsFlow` class (lines 238-282).

- [ ] **Step 2: Remove now-unused imports**

Remove `OptionsFlow` (line 13) and `BUILTIN_SCREEN_LABEL` (line 30) if no other code in the file references them (`grep -n 'OptionsFlow\|BUILTIN_SCREEN_LABEL' custom_components/byonk/config_flow.py` returns nothing after the class is gone).

- [ ] **Step 3: Remove the UI strings**

In `custom_components/byonk/strings.json`, delete the `options.step.init` block (lines 51-62). Mirror in `translations/en.json`.

- [ ] **Step 4: Delete the revert-target test**

```bash
git rm tests_ha/test_options_flow.py
```

- [ ] **Step 5: Verify ruff + the suite**

Run: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`
Expected: ruff clean; pytest passes. Confirm `async_update_settings` is still exercised by the registration-switch test (`tests_ha/test_settings_entities.py`).

- [ ] **Step 6: Commit**

```bash
git add custom_components/byonk/config_flow.py custom_components/byonk/strings.json custom_components/byonk/translations/en.json
git commit -m "revert(ha): remove global settings options flow (now add-on owned)"
```

---

## Task 8: Trim the API-client package-write tests

**Files:**
- Modify: `tests_ha/test_api.py` (remove assertions for `async_add_package`/`async_update_package`/`async_delete_package`; keep `async_get_packages` + `async_update_packages` + `async_update_settings`)

**Interfaces:**
- Consumes: `ByonkClient` methods — the write methods remain defined (still used by standalone-facing code paths and kept for API completeness) but the integration no longer drives them, so their integration-side tests are dropped.

- [ ] **Step 1: Identify the package-write assertions**

Run: `grep -n 'async_add_package\|async_update_package\b\|async_delete_package' tests_ha/test_api.py`
Expected: lists the test functions/lines asserting the three registry-write methods.

- [ ] **Step 2: Remove those test cases**

Delete the test functions (or the specific assertions) covering `async_add_package`, `async_update_package`, and `async_delete_package`. **Keep** any tests for `async_get_packages`, `async_update_packages` (the button backend), and `async_update_settings` (the switch backend).

- [ ] **Step 3: Verify ruff + the suite**

Run: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`
Expected: ruff clean; pytest passes with the reduced count.

- [ ] **Step 4: Commit**

```bash
git add tests_ha/test_api.py
git commit -m "test(ha): drop integration-side package registry-write tests"
```

---

## Task 9: Documentation + changelog

**Files:**
- Modify: `CHANGES.md` (Unreleased section)
- Modify: `docs/src/` (the HA add-on + integration pages describing config ownership)

**Interfaces:** none (docs only).

- [ ] **Step 1: Update `CHANGES.md`**

Under the `Unreleased` heading, add entries:

```markdown
### Changed
- **Home Assistant add-on now owns byonk's global configuration.** Server settings
  (`auth_mode`, `package_refresh_interval`) and the screen-package registry are now
  edited in the byonk add-on's Configuration tab (`options.json`); changes apply on
  add-on restart. In add-on mode these become read-only over the admin API.
- **HA integration is now read-only monitoring for global config.** The package
  add/reconfigure/delete flows and the global settings Options Flow were removed;
  package status sensors, the Update-packages button, the registration switch, and
  per-device entities remain.

### Unchanged
- Standalone byonk (no add-on `options.json`) keeps full read/write config via
  `config.yaml` and the admin API.
```

- [ ] **Step 2: Update the docs pages**

In the relevant `docs/src/` page(s) for the HA add-on and integration, describe the config-ownership split (add-on Options = global config source of truth; integration = monitoring + per-device + operational controls; apply-on-restart trade-off). Reuse the wording from the spec §2 and §7 table.

- [ ] **Step 3: Build the docs**

Run: `make docs`
Expected: mdBook builds clean.

- [ ] **Step 4: Commit**

```bash
git add CHANGES.md docs/src
git commit -m "docs(addon): document add-on-owned global config"
```

---

## Self-Review

**Spec coverage (against `2026-07-04-addon-owned-global-config-design.md`):**
- §4 app Options schema → Task 4. ✅
- §5.1 extend `AddonOptions` (auth_mode, package_refresh_interval, packages) → Task 2. ✅
- §5.2 feed options into byonk on startup (settings + package registry; config.yaml still provides devices) → Task 2 (apply) + existing merge (`PackageManager` reads `config.packages`; `config.yaml` still supplies `devices`). ✅
- §5.3 global-config admin writes read-only in add-on mode; per-device writes stay; content refresh allowed → Task 3 (gate on the four registry/settings handlers, **not** on device handlers or `update_package`/`update_all_packages`). ✅
- §5.4 standalone unchanged → Global Constraints + Task 1 (`addon_mode` false by default) + Task 3 (`global_writes_allowed_standalone` test). ✅
- §5.5 token handling / redaction → Global Constraints (preserved; `PackageInfo` untouched). ✅
- §6 integration reverts (Tasks 3–7) + keeps (sensors, button, switch, per-device) → Tasks 5, 6, 7, 8; keeps are explicitly *not touched*. ✅
- **§4a / §5.6 reserved DEFAULT device → Plan B** (deliberately out of scope here; see note below). ⚠️ tracked, not a gap.
- §8 testing → Rust unit tests (Tasks 2, 3), manifest assertion (Task 4), pytest suite kept green (Tasks 5–8). Live-VM verification is an execution-time step after merge. ✅

**Deliberate scope note:** This plan implements the add-on-mode config redirection but **not** the `default_screen`/`registration.screen` → reserved-DEFAULT-device unification (spec §4a, §5.6, §6 DEFAULT-device surface). That is a core model change applying to standalone byonk too, and is planned separately as **Plan B** (`2026-07-04-reserved-default-device.md`), executed after this plan. Until Plan B lands, byonk's shipped `default_screen: byonk-builtin/default` remains the default and per-device screens are set via the integration (allowed in add-on mode) — a working intermediate state.

**Placeholder scan:** No TBD/TODO; every code step shows the exact code or exact deletion target with line anchors. ✅

**Type consistency:** `AddonPackage { handle, repo, pin, token }` → folded into `PackageRef { repo, pin, token }` keyed by handle (matches `src/models/config.rs`). `AppState.addon_mode: bool` defined in Task 1, consumed in Task 3. `require_writable_global(&AppState) -> Result<(), ApiError>` consistent across definition and call sites. ✅
