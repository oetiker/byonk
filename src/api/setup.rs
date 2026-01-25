use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use super::headers::HeaderMapExt;
use crate::error::ApiError;
use crate::models::{ApiKey, AppConfig, Device, DeviceId, DeviceModel};
use crate::services::DeviceRegistry;

/// Response from the /api/setup endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct SetupResponse {
    /// Status code (200 = success, 404 = device not found)
    pub status: u16,
    /// API key for authenticating future requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Human-readable device identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friendly_id: Option<String>,
    /// Optional URL to an initial image to display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Optional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Register a new device or retrieve existing registration
///
/// The device sends its MAC address and receives an API key for future requests.
/// When device registration is enabled, Byonk generates keys with a special prefix
/// that can be validated. Non-Byonk keys (from other servers) will be replaced.
#[utoipa::path(
    get,
    path = "/api/setup",
    responses(
        (status = 200, description = "Device registered successfully", body = SetupResponse),
        (status = 400, description = "Missing required header"),
    ),
    params(
        ("ID" = String, Header, description = "Device MAC address (e.g., 'AA:BB:CC:DD:EE:FF')"),
        ("FW-Version" = String, Header, description = "Firmware version (e.g., '1.7.1')"),
        ("Model" = String, Header, description = "Device model ('og' or 'x')"),
    ),
    tag = "Device"
)]
pub async fn handle_setup<R: DeviceRegistry>(
    State(config): State<Arc<AppConfig>>,
    State(registry): State<Arc<R>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    // Extract headers
    let device_id_str = headers.require_str("ID")?;
    let fw_version = headers.get_str("FW-Version").unwrap_or("unknown");
    let model_str = headers.get_str("Model").unwrap_or("og");

    let model = DeviceModel::parse(model_str);
    let device_id = DeviceId::new(device_id_str);

    tracing::info!(
        device_id = %device_id,
        fw_version = fw_version,
        model = ?model,
        registration_enabled = config.registration.enabled,
        "Setup request received"
    );

    // Check if already registered in our registry
    if let Some(mut existing) = registry.find_by_id(&device_id).await? {
        // If registration is enabled and device has a non-Byonk key, replace it
        if config.registration.enabled && !existing.api_key.is_byonk_key() {
            let old_key = existing.api_key.as_str().to_string();
            let new_key = ApiKey::generate();

            tracing::info!(
                device_id = %device_id,
                old_key_prefix = &old_key[..old_key.len().min(8)],
                new_registration_code = new_key.registration_code().unwrap_or(""),
                "Replacing non-Byonk API key with Byonk key"
            );

            existing.api_key = new_key;
            registry.update(existing.clone()).await?;

            return Ok(Json(SetupResponse {
                status: 200,
                api_key: Some(existing.api_key.as_str().to_string()),
                friendly_id: Some(existing.friendly_id.clone()),
                image_url: None,
                message: Some("Device key upgraded to Byonk format".to_string()),
            }));
        }

        tracing::info!(
            device_id = %device_id,
            friendly_id = %existing.friendly_id,
            is_byonk_key = existing.api_key.is_byonk_key(),
            "Device already registered"
        );

        return Ok(Json(SetupResponse {
            status: 200,
            api_key: Some(existing.api_key.as_str().to_string()),
            friendly_id: Some(existing.friendly_id.clone()),
            image_url: None,
            message: Some("Device already registered".to_string()),
        }));
    }

    // Register new device (always uses Byonk key format now)
    let device = Device::new(device_id.clone(), model, fw_version.to_string());

    if config.registration.enabled {
        if let Some(code) = device.api_key.registration_code() {
            tracing::info!(
                device_id = %device_id,
                registration_code = code,
                "New device registered with registration code"
            );
        }
    }

    let registered = registry.register(device_id, device).await?;

    tracing::info!(
        friendly_id = %registered.friendly_id,
        "New device registered"
    );

    Ok(Json(SetupResponse {
        status: 200,
        api_key: Some(registered.api_key.as_str().to_string()),
        friendly_id: Some(registered.friendly_id),
        image_url: None,
        message: Some("Device registered successfully".to_string()),
    }))
}
