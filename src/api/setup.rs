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
use crate::models::{AppConfig, Device, DeviceId, DeviceModel};
use crate::services::DeviceRegistry;

/// Response from the /api/setup endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct SetupResponse {
    /// Status code (200 = success, 404 = device not found)
    pub status: u16,
    /// API key for authenticating future requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Optional URL to an initial image to display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Optional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Authentication mode the device should use ("api_key" or "ed25519")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
}

/// Register a new device or retrieve existing registration
///
/// The device sends its MAC address and receives an API key for future requests.
/// If the device already has a stored key, it is returned. Otherwise, a new
/// random key is generated.
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
        "Setup request received"
    );

    // Check if already registered in our registry
    if let Some(existing) = registry.find_by_id(&device_id).await? {
        tracing::info!(
            device_id = %device_id,
            registration_code = %existing.api_key.registration_code(),
            "Device already registered"
        );

        return Ok(Json(SetupResponse {
            status: 200,
            api_key: Some(existing.api_key.as_str().to_string()),
            image_url: None,
            message: Some("Device already registered".to_string()),
            auth_mode: Some(config.auth_mode.clone()),
        }));
    }

    // Register new device with a random API key
    let device = Device::new(device_id.clone(), model, fw_version.to_string());

    tracing::info!(
        device_id = %device_id,
        registration_code = %device.api_key.registration_code(),
        "New device registered"
    );

    registry.upsert(device.clone()).await?;

    Ok(Json(SetupResponse {
        status: 200,
        api_key: Some(device.api_key.as_str().to_string()),
        image_url: None,
        message: Some("Device registered successfully".to_string()),
        auth_mode: Some(config.auth_mode.clone()),
    }))
}
