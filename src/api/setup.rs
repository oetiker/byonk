use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::models::{Device, DeviceId, DeviceModel};
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
    State(registry): State<Arc<R>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    // Extract headers
    let device_id_str = headers
        .get("ID")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::MissingHeader("ID"))?;

    let fw_version = headers
        .get("FW-Version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let model_str = headers
        .get("Model")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("og");

    let model = DeviceModel::parse(model_str);
    let device_id = DeviceId::new(device_id_str);

    tracing::info!(
        device_id = %device_id,
        fw_version = fw_version,
        model = ?model,
        "Setup request received"
    );

    // Check if already registered
    if let Some(existing) = registry.find_by_id(&device_id).await? {
        tracing::info!(
            device_id = %device_id,
            friendly_id = %existing.friendly_id,
            "Device already registered"
        );

        return Ok(Json(SetupResponse {
            status: 200,
            api_key: Some(existing.api_key.as_str().to_string()),
            friendly_id: Some(existing.friendly_id.clone()),
            image_url: None, // Device will call /api/display
            message: Some("Device already registered".to_string()),
        }));
    }

    // Register new device
    let device = Device::new(device_id.clone(), model, fw_version.to_string());
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
