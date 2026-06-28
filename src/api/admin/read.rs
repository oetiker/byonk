//! Admin read endpoints.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use crate::error::ApiError;
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
        });
    }

    Ok(Json(out))
}
