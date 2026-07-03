//! Admin write endpoints: device mappings + global settings.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::ApiError;
use crate::models::param_schema::validate_params;
use crate::server::{reload_config, AppState};
use crate::services::config_writer;

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

/// Validate the screen ref resolves to a package screen and that the provided
/// params pass its `meta.yaml` schema. A device `screen` is always a qualified
/// `handle/path` package ref — there is no bare-name / flat-file resolution.
fn validate_screen_and_params(
    state: &AppState,
    screen: &str,
    params: &HashMap<String, serde_yaml::Value>,
) -> Result<(), ApiError> {
    let resolved = state
        .package_manager
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
    pub(crate) default_screen: Option<String>,
    pub(crate) registration_screen: Option<String>,
    pub(crate) package_refresh_interval: Option<u64>,
}

pub async fn patch_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SettingsWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
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
    if let Some(screen) = &body.default_screen {
        if state.package_manager.loader().resolve(screen).is_none() {
            return Err(ApiError::BadRequest(format!("unknown screen `{screen}`")));
        }
    }
    if let Some(screen) = &body.registration_screen {
        if !screen.is_empty() && state.package_manager.loader().resolve(screen).is_none() {
            return Err(ApiError::BadRequest(format!("unknown screen `{screen}`")));
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
    if let Some(screen) = &body.default_screen {
        yaml = config_writer::set_scalar(&yaml, &["default_screen"], screen.as_str().into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(screen) = &body.registration_screen {
        yaml =
            config_writer::set_scalar(&yaml, &["registration", "screen"], screen.as_str().into())
                .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(secs) = body.package_refresh_interval {
        yaml = config_writer::set_scalar(&yaml, &["package_refresh_interval"], secs.into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // 3. Persist.
    persist(&state, &path, yaml)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
