//! Admin write endpoints: device mappings + global settings.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::ApiError;
use crate::models::config::RESERVED_DEFAULT_KEY;
use crate::models::param_schema::validate_params;
use crate::server::{reload_config, AppState};
use crate::services::config_writer;
use crate::services::screen_repo_loader::BUILTIN_HANDLE;

use super::read::{build_screen_repo_info, ScreenRepoInfo};
use super::require_admin;

#[derive(Deserialize)]
pub struct DeviceWrite {
    pub key: Option<String>, // required for POST, ignored for PATCH (taken from URL)
    pub screen: Option<String>,
    pub panel: Option<String>,
    pub dither: Option<String>,
    pub colors: Option<String>,
    pub params: Option<HashMap<String, serde_yaml::Value>>,
    pub refresh: Option<u32>,
    pub name: Option<String>,
}

/// Guard: writes require a file-backed config.
fn require_file_config(state: &AppState) -> Result<std::path::PathBuf, ApiError> {
    state
        .asset_loader
        .config_path()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| ApiError::Conflict("config is embedded/read-only; set CONFIG_FILE".into()))
}

/// Guard: global-config registry/settings writes are read-only when byonk runs
/// as an HA add-on. The add-on Options form (`/data/options.json`) is the sole
/// editor for global config; per-device writes and screen repo content-refresh are
/// unaffected.
fn require_writable_global(state: &AppState) -> Result<(), ApiError> {
    if state.addon_mode {
        return Err(ApiError::Conflict(
            "global config is read-only in add-on mode; edit it in the byonk add-on Configuration tab".into(),
        ));
    }
    Ok(())
}

/// In add-on mode, the genuinely-global settings are read-only (edited via the
/// add-on Options form). The operational `registration_enabled` toggle stays live.
fn require_writable_settings(state: &AppState, body: &SettingsWrite) -> Result<(), ApiError> {
    let touches_global = body.auth_mode.is_some() || body.screen_repo_refresh_interval.is_some();
    if state.addon_mode && touches_global {
        return Err(ApiError::Conflict(
            "global config is read-only in add-on mode; edit it in the byonk add-on Configuration tab".into(),
        ));
    }
    Ok(())
}

/// Validate the screen ref resolves to a screen repo screen and that the provided
/// params pass its `meta.yaml` schema. A device `screen` is always a qualified
/// `handle/path` screen repo ref — there is no bare-name / flat-file resolution.
fn validate_screen_and_params(
    state: &AppState,
    screen: &str,
    params: &HashMap<String, serde_yaml::Value>,
) -> Result<(), ApiError> {
    let resolved = state
        .screen_repo_manager
        .loader()
        .resolve(screen)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown screen `{screen}`")))?;
    if let Err(errs) = validate_params(&resolved.meta.params, params) {
        return Err(ApiError::BadRequest(errs.join("; ")));
    }
    Ok(())
}

/// Build the YAML mapping for a device block from the provided fields.
fn device_block(w: &DeviceWrite, screen: &str) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    m.insert("screen".into(), screen.into());
    if let Some(p) = &w.panel {
        m.insert("panel".into(), p.as_str().into());
    }
    if let Some(d) = &w.dither {
        m.insert("dither".into(), d.as_str().into());
    }
    if let Some(c) = &w.colors {
        m.insert("colors".into(), c.as_str().into());
    }
    if let Some(r) = w.refresh {
        if r > 0 {
            m.insert("refresh".into(), serde_yaml::Value::from(r));
        }
    }
    if let Some(n) = &w.name {
        if !n.is_empty() {
            m.insert("name".into(), n.as_str().into());
        }
    }
    if let Some(params) = &w.params {
        let mut pm = serde_yaml::Mapping::new();
        for (k, v) in params {
            pm.insert(k.as_str().into(), v.clone());
        }
        m.insert("params".into(), serde_yaml::Value::Mapping(pm));
    }
    m
}

fn persist(state: &AppState, path: &std::path::Path, new_yaml: String) -> Result<(), ApiError> {
    // Snapshot current contents so we can roll back if the new text fails to reparse.
    let previous = std::fs::read_to_string(path).ok();

    // Atomic write: temp file in same dir, then rename.
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, &new_yaml).map_err(|e| ApiError::Internal(format!("write temp: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| ApiError::Internal(format!("rename: {e}")))?;

    // Reload into the live config; on failure, roll the file back.
    if let Err(e) = reload_config(state) {
        if let Some(prev) = previous {
            let _ = std::fs::write(path, prev);
        }
        return Err(ApiError::Internal(format!(
            "reload failed, rolled back: {e}"
        )));
    }
    Ok(())
}

pub async fn add_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;
    let key = body
        .key
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`key` is required".into()))?;
    let screen = body
        .screen
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`screen` is required".into()))?;

    // Reject duplicate.
    if state.config.load().devices.contains_key(&key) {
        return Err(ApiError::Conflict(format!("device `{key}` already exists")));
    }

    let empty = HashMap::new();
    validate_screen_and_params(&state, &screen, body.params.as_ref().unwrap_or(&empty))?;

    let block = device_block(&body, &screen);
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_device(&yaml, &key, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "key": key, "screen": screen })))
}

pub async fn patch_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    // Must already exist.
    let existing = {
        let config = state.config.load();
        config
            .devices
            .get(&key)
            .cloned()
            .ok_or(ApiError::NotFound)?
    };

    // Merge: start from existing, override provided fields.
    let screen = body.screen.clone().unwrap_or(existing.screen.clone());

    // Params: a screen change replaces params wholesale (the new screen's
    // defaults). Without a screen change, provided params are merged key-by-key
    // into the existing set, so editing one parameter never drops the others.
    let params = if body.screen.is_none() {
        match &body.params {
            Some(p) => {
                let mut merged = existing.params.clone();
                for (k, v) in p {
                    merged.insert(k.clone(), v.clone());
                }
                merged
            }
            None => existing.params.clone(),
        }
    } else {
        body.params
            .clone()
            .unwrap_or_else(|| existing.params.clone())
    };

    let merged = DeviceWrite {
        key: Some(key.clone()),
        screen: Some(screen.clone()),
        panel: body.panel.clone().or(existing.panel.clone()),
        dither: body.dither.clone().or(existing.dither.clone()),
        colors: body.colors.clone().or(existing.colors.clone()),
        params: Some(params),
        refresh: body.refresh.or(existing.refresh),
        name: body.name.clone().or(existing.name.clone()),
    };

    let empty = HashMap::new();
    validate_screen_and_params(&state, &screen, merged.params.as_ref().unwrap_or(&empty))?;

    let block = device_block(&merged, &screen);
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_device(&yaml, &key, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "key": key, "screen": screen })))
}

pub async fn delete_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    if key == RESERVED_DEFAULT_KEY {
        return Err(ApiError::Conflict(
            "the reserved DEFAULT device cannot be deleted".into(),
        ));
    }
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = match config_writer::remove_device(&yaml, &key) {
        Ok(y) => y,
        Err(config_writer::ConfigWriteError::NotFound(_)) => return Err(ApiError::NotFound),
        Err(e) => return Err(ApiError::Internal(e.to_string())),
    };
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "deleted": key })))
}

#[derive(Deserialize)]
pub struct SettingsWrite {
    pub(crate) registration_enabled: Option<bool>,
    pub(crate) auth_mode: Option<String>,
    pub(crate) screen_repo_refresh_interval: Option<u64>,
}

pub async fn patch_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SettingsWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    require_writable_settings(&state, &body)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    // 1. Validate all provided fields before touching yaml.
    if let Some(mode) = &body.auth_mode {
        if mode != "api_key" && mode != "ed25519" {
            return Err(ApiError::BadRequest(
                "auth_mode must be api_key or ed25519".into(),
            ));
        }
    }
    // 2. Apply all mutations.
    let mut yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if let Some(enabled) = body.registration_enabled {
        yaml = config_writer::set_scalar(&yaml, &["registration", "enabled"], enabled.into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(mode) = &body.auth_mode {
        yaml = config_writer::set_scalar(&yaml, &["auth_mode"], mode.as_str().into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(secs) = body.screen_repo_refresh_interval {
        yaml = config_writer::set_scalar(&yaml, &["screen_repo_refresh_interval"], secs.into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // 3. Persist.
    persist(&state, &path, yaml)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ScreenRepoWrite {
    pub handle: Option<String>,
    pub repo: Option<String>,
    pub pin: Option<String>,
    pub token: Option<String>,
}

/// Number of screens currently resolved under `handle` (0 before the first
/// successful fetch, since nothing is registered in the loader yet).
fn screen_repo_screen_count(state: &AppState, handle: &str) -> usize {
    state
        .screen_repo_manager
        .loader()
        .list_all()
        .into_iter()
        .filter(|s| s.handle == handle)
        .count()
}

/// Build a `serde_yaml::Mapping` screen repo block from the given fields, omitting
/// any that are `None` (so e.g. an absent `token` is never written as `null`).
fn screen_repo_block(
    repo: Option<&str>,
    pin: Option<&str>,
    token: Option<&str>,
) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    if let Some(r) = repo {
        m.insert("repo".into(), r.into());
    }
    if let Some(p) = pin {
        m.insert("pin".into(), p.into());
    }
    if let Some(t) = token {
        m.insert("token".into(), t.into());
    }
    m
}

pub async fn add_screen_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ScreenRepoWrite>,
) -> Result<Json<ScreenRepoInfo>, ApiError> {
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let handle = body
        .handle
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`handle` is required".into()))?;

    if handle == BUILTIN_HANDLE {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is the reserved builtin screen repo handle"
        )));
    }
    if state.config.load().screen_repos.contains_key(&handle) {
        return Err(ApiError::Conflict(format!(
            "screen repo `{handle}` already exists"
        )));
    }

    let block = screen_repo_block(
        body.repo.as_deref(),
        body.pin.as_deref(),
        body.token.as_deref(),
    );
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_screen_repo(&yaml, &handle, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    // Fetch asynchronously; the response reflects whatever status exists at
    // this instant (likely no entry yet). The client polls GET /screen-repos for
    // the settled result rather than this handler awaiting the fetch.
    let mgr = state.screen_repo_manager.clone();
    let h = handle.clone();
    tokio::task::spawn_blocking(move || mgr.refresh_one(&h));

    let count = screen_repo_screen_count(&state, &handle);
    let statuses = state.screen_repo_manager.status_snapshot();
    let info = build_screen_repo_info(&state.config.load(), statuses.get(&handle), count, handle);
    Ok(Json(info))
}

pub async fn patch_screen_repo(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ScreenRepoWrite>,
) -> Result<Json<ScreenRepoInfo>, ApiError> {
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    if handle == BUILTIN_HANDLE {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is the reserved builtin screen repo handle"
        )));
    }

    let existing = state
        .config
        .load()
        .screen_repos
        .get(&handle)
        .cloned()
        .ok_or(ApiError::NotFound)?;

    // Overlay only the provided fields; an omitted field (incl. `token`)
    // keeps its existing value.
    let repo = body.repo.clone().or_else(|| existing.repo.clone());
    let pin = body.pin.clone().or_else(|| existing.pin.clone());
    let token = body.token.clone().or_else(|| existing.token.clone());
    let repo_or_pin_changed = (body.repo.is_some() && body.repo != existing.repo)
        || (body.pin.is_some() && body.pin != existing.pin);

    let block = screen_repo_block(repo.as_deref(), pin.as_deref(), token.as_deref());
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_screen_repo(&yaml, &handle, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    if repo_or_pin_changed {
        let mgr = state.screen_repo_manager.clone();
        let h = handle.clone();
        tokio::task::spawn_blocking(move || mgr.refresh_one(&h));
    }

    let count = screen_repo_screen_count(&state, &handle);
    let statuses = state.screen_repo_manager.status_snapshot();
    let info = build_screen_repo_info(&state.config.load(), statuses.get(&handle), count, handle);
    Ok(Json(info))
}

pub async fn delete_screen_repo(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    if handle == BUILTIN_HANDLE {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is the reserved builtin screen repo handle"
        )));
    }

    // Reject if any device's screen dangles into this screen repo's namespace.
    let prefix = format!("{handle}/");
    let config = state.config.load();
    if let Some((device_key, _)) = config
        .devices
        .iter()
        .find(|(_, d)| d.screen.starts_with(&prefix))
        .map(|(k, d)| (k.clone(), d.screen.clone()))
    {
        return Err(ApiError::Conflict(format!(
            "screen repo `{handle}` is referenced by device `{device_key}`"
        )));
    }
    drop(config);

    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = match config_writer::remove_screen_repo(&yaml, &handle) {
        Ok(y) => y,
        Err(config_writer::ConfigWriteError::NotFound(_)) => return Err(ApiError::NotFound),
        Err(e) => return Err(ApiError::Internal(e.to_string())),
    };
    persist(&state, &path, new_yaml)?;

    // Forget the deleted handle's fetch status so a later re-registration of
    // the same handle doesn't briefly surface the stale resolved_sha/Ready
    // state left over from the deleted screen repo.
    state.screen_repo_manager.forget_status(&handle);
    // Rebuild the hot-swapped loader so the deleted handle's screens stop
    // resolving immediately (in-memory only; not blocking).
    state.screen_repo_manager.rebuild_loader();

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_screen_repo(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ScreenRepoInfo>, ApiError> {
    require_admin(&state, &headers)?;

    if handle != BUILTIN_HANDLE && !state.config.load().screen_repos.contains_key(&handle) {
        return Err(ApiError::NotFound);
    }

    let mgr = state.screen_repo_manager.clone();
    let h = handle.clone();
    tokio::task::spawn_blocking(move || mgr.refresh_one(&h));

    let count = screen_repo_screen_count(&state, &handle);
    let statuses = state.screen_repo_manager.status_snapshot();
    let info = build_screen_repo_info(&state.config.load(), statuses.get(&handle), count, handle);
    Ok(Json(info))
}

pub async fn update_all_screen_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;

    let mgr = state.screen_repo_manager.clone();
    tokio::task::spawn_blocking(move || mgr.refresh_all(true));

    Ok(Json(serde_json::json!({ "ok": true })))
}

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

    fn registration_only_body() -> SettingsWrite {
        SettingsWrite {
            registration_enabled: Some(true),
            auth_mode: None,
            screen_repo_refresh_interval: None,
        }
    }

    #[test]
    fn addon_mode_allows_registration_enabled_only() {
        let state = state_with_addon_mode(true);
        let body = registration_only_body();
        assert!(
            require_writable_settings(&state, &body).is_ok(),
            "registration_enabled-only body must stay live in add-on mode"
        );
    }

    #[test]
    fn addon_mode_rejects_global_field() {
        let state = state_with_addon_mode(true);
        let mut body = registration_only_body();
        body.auth_mode = Some("api_key".to_string());
        match require_writable_settings(&state, &body) {
            Err(ApiError::Conflict(_)) => {}
            other => panic!("expected Conflict for global field in add-on mode, got {other:?}"),
        }
    }

    #[test]
    fn standalone_allows_any_settings_body() {
        let state = state_with_addon_mode(false);
        let body = SettingsWrite {
            registration_enabled: Some(false),
            auth_mode: Some("ed25519".to_string()),
            screen_repo_refresh_interval: Some(300),
        };
        assert!(require_writable_settings(&state, &body).is_ok());
    }
}
