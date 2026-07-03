//! Admin read endpoints.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use crate::error::ApiError;
use crate::models::compat::{compat_warning, engine_version};
use crate::models::config::AppConfig;
use crate::models::param_schema::ParamField;
use crate::server::AppState;
use crate::services::DeviceRegistry;

use super::require_admin;

#[derive(Serialize)]
pub struct PendingDevice {
    pub mac: String,
    pub registration_code: String,
    pub model: String,
    pub firmware_version: String,
    pub last_seen: String,
}

pub async fn pending(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PendingDevice>>, ApiError> {
    require_admin(&state, &headers)?;

    let config = state.config.load();
    let mut out = Vec::new();
    for d in state.registry.list_all().await? {
        let mac = d.device_id.to_string();
        let code = d.api_key.registration_code();
        if config.is_device_registered(&mac, Some(&code)) {
            continue;
        }
        out.push(PendingDevice {
            mac,
            registration_code: code,
            model: d.model.to_string(),
            firmware_version: d.firmware_version.clone(),
            last_seen: d.last_seen.to_rfc3339(),
        });
    }
    Ok(Json(out))
}

pub async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;

    let text = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;
    let mut value: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|e| ApiError::Internal(format!("parse config: {e}")))?;

    // Strip admin.token from the response.
    if let Some(map) = value.as_mapping_mut() {
        if let Some(admin) = map
            .get_mut(serde_yaml::Value::from("admin"))
            .and_then(|a| a.as_mapping_mut())
        {
            admin.remove(serde_yaml::Value::from("token"));
        }
        // Strip package tokens — `PackageRef.token` is documented as
        // "Secret token; redacted in read APIs".
        if let Some(packages) = map
            .get_mut(serde_yaml::Value::from("packages"))
            .and_then(|p| p.as_mapping_mut())
        {
            for (_, pkg) in packages.iter_mut() {
                if let Some(pkg_map) = pkg.as_mapping_mut() {
                    pkg_map.remove(serde_yaml::Value::from("token"));
                }
            }
        }
    }

    let json =
        serde_json::to_value(&value).map_err(|e| ApiError::Internal(format!("to json: {e}")))?;
    Ok(Json(json))
}

#[derive(Serialize)]
pub struct AdminDevice {
    /// Config key (MAC or registration code) if configured, else the MAC.
    pub key: String,
    pub mac: String,
    pub registration_code: String,
    pub registered: bool,
    // telemetry (None when never seen)
    pub model: Option<String>,
    pub firmware_version: Option<String>,
    pub last_seen: Option<String>,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
    // resolved active config (None when no mapping)
    pub screen: Option<String>,
    pub dither: Option<String>,
    pub panel: Option<String>,
    pub colors: Option<String>,
    pub params: serde_json::Value,
    pub refresh: Option<u32>,
    pub name: Option<String>,
}

pub async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminDevice>>, ApiError> {
    require_admin(&state, &headers)?;

    let config = state.config.load();
    let seen = state.registry.list_all().await?;

    let mut out: Vec<AdminDevice> = Vec::new();

    // 1) Every device the registry has seen, merged with its config mapping.
    for d in &seen {
        let mac = d.device_id.to_string();
        let code = d.api_key.registration_code();
        let dc = config
            .get_device_config(&mac)
            .or_else(|| config.get_device_config_for_code(&code));
        let registered = config.is_device_registered(&mac, Some(&code));
        out.push(AdminDevice {
            key: mac.clone(),
            mac,
            registration_code: code,
            registered,
            model: Some(d.model.to_string()),
            firmware_version: Some(d.firmware_version.clone()),
            last_seen: Some(d.last_seen.to_rfc3339()),
            battery_voltage: d.battery_voltage,
            rssi: d.rssi,
            screen: dc.map(|c| c.screen.clone()),
            dither: dc.and_then(|c| c.dither.clone()),
            panel: dc.and_then(|c| c.panel.clone()),
            colors: dc.and_then(|c| c.colors.clone()),
            params: dc
                .map(|c| serde_json::to_value(&c.params).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null),
            refresh: dc.and_then(|c| c.refresh),
            name: dc.and_then(|c| c.name.clone()),
        });
    }

    // 2) Configured devices that have NOT been seen yet (telemetry = None).
    let seen_macs: std::collections::HashSet<String> =
        seen.iter().map(|d| d.device_id.to_string()).collect();
    for (key, dc) in &config.devices {
        // Skip if this config entry corresponds to a seen device (by MAC key).
        if seen_macs.contains(key) {
            continue;
        }
        out.push(AdminDevice {
            key: key.clone(),
            mac: key.clone(),
            registration_code: String::new(),
            registered: true,
            model: None,
            firmware_version: None,
            last_seen: None,
            battery_voltage: None,
            rssi: None,
            screen: Some(dc.screen.clone()),
            dither: dc.dither.clone(),
            panel: dc.panel.clone(),
            colors: dc.colors.clone(),
            params: serde_json::to_value(&dc.params).unwrap_or(serde_json::Value::Null),
            refresh: dc.refresh,
            name: dc.name.clone(),
        });
    }

    Ok(Json(out))
}

#[derive(Serialize)]
pub struct ScreenInfo {
    pub r#ref: String,
    pub title: String,
    pub description: String,
    pub params: Vec<ParamField>,
    pub byonk: String,
    pub compat_warning: Option<String>,
}

#[derive(Serialize)]
pub struct PackageScreens {
    pub handle: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub screens: Vec<ScreenInfo>,
}

#[derive(Serialize)]
pub struct PanelInfo {
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub colors: String,
}

#[derive(Serialize)]
pub struct ScreensResponse {
    pub packages: Vec<PackageScreens>,
    pub panels: Vec<PanelInfo>,
    pub dither_algorithms: Vec<String>,
}

/// One entry in the package registry listing (`GET /packages`).
#[derive(Serialize)]
pub struct PackageInfo {
    pub handle: String,
    pub repo: Option<String>,
    pub pin: Option<String>,
    /// `true` for the embedded builtin or any package without a remote repo.
    pub builtin: bool,
    /// Whether an auth token is configured — the token itself is never serialized.
    pub token_set: bool,
    pub screen_count: usize,
    /// Static in Plan 1 (fetch status/sha/last_fetched land in Plan 2).
    pub status: String,
}

/// Canonical dither algorithm names byonk understands.
const DITHER_ALGORITHMS: &[&str] = &[
    "floyd-steinberg",
    "atkinson",
    "atkinson-hybrid",
    "jarvis-judice-ninke",
    "sierra",
    "sierra-two-row",
    "sierra-lite",
    "sierra-light",
    "stucki",
    "burkes",
];

pub async fn screens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ScreensResponse>, ApiError> {
    require_admin(&state, &headers)?;
    let config = state.config.load();

    // Group every resolved screen by its package handle. The package-level
    // metadata (name/description/author/license) comes from that package's
    // manifest, read off the first screen encountered for the handle.
    let engine = engine_version();
    let mut by_handle: std::collections::BTreeMap<String, PackageScreens> =
        std::collections::BTreeMap::new();

    for screen in state.package_manager.loader().list_all() {
        let entry = by_handle.entry(screen.handle.clone()).or_insert_with(|| {
            let m = screen.source.manifest();
            PackageScreens {
                handle: screen.handle.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                author: m.author.clone(),
                license: m.license.clone(),
                screens: Vec::new(),
            }
        });
        entry.screens.push(ScreenInfo {
            r#ref: format!("{}/{}", screen.handle, screen.path),
            title: screen.meta.title.clone(),
            description: screen.meta.description.clone(),
            params: screen.meta.params.fields.clone(),
            byonk: screen.meta.byonk.clone(),
            compat_warning: compat_warning(engine, &screen.meta.byonk),
        });
    }

    // Deterministic order: screens within each package sorted by ref.
    let packages: Vec<PackageScreens> = by_handle
        .into_values()
        .map(|mut p| {
            p.screens.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
            p
        })
        .collect();

    let panels = config
        .panels
        .iter()
        .map(|(name, p)| PanelInfo {
            name: name.clone(),
            width: p.width,
            height: p.height,
            colors: p.colors.clone(),
        })
        .collect();

    Ok(Json(ScreensResponse {
        packages,
        panels,
        dither_algorithms: DITHER_ALGORITHMS.iter().map(|s| s.to_string()).collect(),
    }))
}

/// Build the `PackageInfo` for a single handle from the live config.
///
/// Shared by `packages()` (the read listing) and the package write handlers
/// (`add`/`patch`/`update`), so both surfaces stay in sync. `status` is a
/// placeholder (`"ready"`) in Plan 2 — real fetch-status enrichment
/// (pin_kind/resolved_sha/last_fetched/error) is Task 9's job and should edit
/// this one builder.
pub(crate) fn build_package_info(
    config: &AppConfig,
    screen_count: usize,
    handle: String,
) -> PackageInfo {
    let pkg = config.packages.get(&handle);
    let repo = pkg.and_then(|p| p.repo.clone());
    let builtin = handle == crate::services::package_loader::BUILTIN_HANDLE || repo.is_none();
    PackageInfo {
        repo,
        pin: pkg.and_then(|p| p.pin.clone()),
        builtin,
        token_set: pkg.map(|p| p.token.is_some()).unwrap_or(false),
        screen_count,
        status: "ready".to_string(),
        handle,
    }
}

/// List the registered screen packages. `byonk-builtin` is always present (it is
/// registered by the package loader even without a `packages:` config entry); any
/// additional entries come from `config.packages`. The package `token` is never
/// serialized — only whether one is set (`token_set`).
pub async fn packages(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PackageInfo>>, ApiError> {
    require_admin(&state, &headers)?;
    let config = state.config.load();

    // Screen counts per handle, from the source of truth (the package loader).
    let loader = state.package_manager.loader();
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for screen in loader.list_all() {
        *counts.entry(screen.handle).or_insert(0) += 1;
    }

    // Union of loader-registered handles and configured package handles, so the
    // always-registered builtin appears and config-only packages are not dropped.
    let mut handles: std::collections::BTreeSet<String> = loader.handles().into_iter().collect();
    handles.extend(config.packages.keys().cloned());

    let out = handles
        .into_iter()
        .map(|handle| {
            let count = counts.get(&handle).copied().unwrap_or(0);
            build_package_info(&config, count, handle)
        })
        .collect();

    Ok(Json(out))
}
