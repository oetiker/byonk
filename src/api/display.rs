use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::models::{ApiKey, Device, DeviceId, DeviceModel, DisplaySpec};
use crate::services::{ContentPipeline, DeviceRegistry, RenderService, UrlSigner};

/// Error response for display endpoints
#[derive(Debug, Serialize, ToSchema)]
pub struct DisplayErrorResponse {
    /// Status code
    pub status: u16,
    /// Error message
    pub error: String,
}

// Maximum allowed display dimensions to prevent DoS
const MAX_DISPLAY_WIDTH: u32 = 2000;
const MAX_DISPLAY_HEIGHT: u32 = 2000;

/// Query parameters for image endpoint
#[derive(Debug, Deserialize)]
pub struct ImageQuery {
    #[serde(default)]
    pub w: Option<u32>,
    #[serde(default)]
    pub h: Option<u32>,
    /// Expiration timestamp for signed URLs
    #[serde(default)]
    pub exp: Option<i64>,
    /// HMAC signature for URL verification
    #[serde(default)]
    pub sig: Option<String>,
}

/// Get display content for a device
///
/// Returns JSON with an image_url that the device should fetch separately.
/// The firmware expects status=0 for success (not HTTP 200).
#[utoipa::path(
    get,
    path = "/api/display",
    responses(
        (status = 200, description = "Display content available", body = DisplayJsonResponse),
        (status = 400, description = "Missing required header"),
        (status = 404, description = "Device not found"),
    ),
    params(
        ("ID" = String, Header, description = "Device MAC address"),
        ("Access-Token" = String, Header, description = "API key from /api/setup"),
        ("Width" = Option<u32>, Header, description = "Display width in pixels (default: 800)"),
        ("Height" = Option<u32>, Header, description = "Display height in pixels (default: 480)"),
        ("Refresh-Rate" = Option<u32>, Header, description = "Current refresh rate in seconds"),
        ("Battery-Voltage" = Option<f32>, Header, description = "Battery voltage"),
        ("RSSI" = Option<i32>, Header, description = "WiFi signal strength"),
        ("FW-Version" = Option<String>, Header, description = "Firmware version"),
        ("Model" = Option<String>, Header, description = "Device model ('og' or 'x')"),
    ),
    tag = "Display"
)]
pub async fn handle_display<R: DeviceRegistry>(
    State(registry): State<Arc<R>>,
    State(_renderer): State<Arc<RenderService>>,
    State(url_signer): State<Arc<UrlSigner>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Extract required headers
    let api_key_str = headers
        .get("Access-Token")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::MissingHeader("Access-Token"))?;

    // Extract device ID (MAC address) for auto-registration
    let device_id_str = headers
        .get("ID")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::MissingHeader("ID"))?;

    // Parse and validate dimensions with bounds checking
    let width: u32 = headers
        .get("Width")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .filter(|&w| w > 0 && w <= MAX_DISPLAY_WIDTH)
        .unwrap_or(800);

    let height: u32 = headers
        .get("Height")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .filter(|&h| h > 0 && h <= MAX_DISPLAY_HEIGHT)
        .unwrap_or(480);

    let api_key = ApiKey::from_str(api_key_str);

    // Find device by API key, or auto-register if not found
    // This handles devices that were previously registered with trmnl.app
    let mut device = match registry.find_by_api_key(&api_key).await? {
        Some(d) => d,
        None => {
            // Auto-register the device using the MAC address from ID header
            let device_id = DeviceId::new(device_id_str);
            let model_str = headers
                .get("Model")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("og");
            let fw_version = headers
                .get("FW-Version")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown")
                .to_string();

            let model = DeviceModel::from_str(model_str);
            let new_device = Device::new(device_id.clone(), model, fw_version);

            tracing::info!(
                device_id = %device_id,
                model = ?model,
                "Auto-registering new device from /api/display"
            );

            registry.register(device_id, new_device).await?
        }
    };

    tracing::info!(
        device_id = %device.device_id,
        width = width,
        height = height,
        "Display request received"
    );

    // Update device metadata
    if let Some(battery) = headers
        .get("Battery-Voltage")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
    {
        device.battery_voltage = Some(battery);
    }

    if let Some(rssi) = headers
        .get("RSSI")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
    {
        device.rssi = Some(rssi);
    }

    device.last_seen = chrono::Utc::now();
    registry.update(device.clone()).await?;

    // Build full image URL - device needs absolute URL for separate HTTP request
    // Get the Host header to construct the base URL
    let host = headers
        .get("Host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:3000");

    // URL-encode the device ID (MAC address contains colons)
    let encoded_device_id = device.device_id.to_string().replace(':', "-");
    let path = format!("/api/image/{encoded_device_id}");

    // Sign the URL to prevent unauthorized access
    let (signature, expires) = url_signer.sign(&path);
    let image_url = format!(
        "http://{host}/api/image/{encoded_device_id}?w={width}&h={height}&exp={expires}&sig={signature}"
    );

    tracing::info!(
        image_url = %image_url,
        "Returning display response with image URL"
    );

    // Return JSON response with image_url
    // Note: firmware expects status=0 for success (not 200!)
    Ok(Json(DisplayJsonResponse {
        status: 0,
        image_url: Some(image_url),
        filename: "default".to_string(),
        update_firmware: false,
        firmware_url: None,
        refresh_rate: 900, // 15 minutes
        reset_firmware: false,
        temperature_profile: Some("default".to_string()),
        special_function: None,
    })
    .into_response())
}

/// Get rendered PNG image for a device
///
/// Returns the actual PNG image data rendered from SVG with dithering applied.
#[utoipa::path(
    get,
    path = "/api/image/{device_id}",
    responses(
        (status = 200, description = "PNG image", content_type = "image/png"),
        (status = 500, description = "Rendering error"),
    ),
    params(
        ("device_id" = String, Path, description = "Device ID (MAC with colons replaced by dashes)"),
        ("Width" = Option<u32>, Header, description = "Display width in pixels (default: 800)"),
        ("Height" = Option<u32>, Header, description = "Display height in pixels (default: 480)"),
    ),
    tag = "Display"
)]
pub async fn handle_image(
    State(renderer): State<Arc<RenderService>>,
    State(url_signer): State<Arc<UrlSigner>>,
    State(content_pipeline): State<Arc<ContentPipeline>>,
    Path(device_id): Path<String>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, ApiError> {
    // Verify URL signature to prevent unauthorized access
    let path = format!("/api/image/{device_id}");
    let signature = query.sig.as_deref().ok_or(ApiError::InvalidSignature)?;
    let expires = query.exp.ok_or(ApiError::InvalidSignature)?;

    if !url_signer.verify(&path, signature, expires) {
        tracing::warn!(device_id = %device_id, "Invalid or expired image URL signature");
        return Err(ApiError::InvalidSignature);
    }

    // Parse dimensions from query parameters (w, h)
    let width: u32 = query
        .w
        .filter(|&w| w > 0 && w <= MAX_DISPLAY_WIDTH)
        .unwrap_or(800);
    let height: u32 = query
        .h
        .filter(|&h| h > 0 && h <= MAX_DISPLAY_HEIGHT)
        .unwrap_or(480);

    // Convert device_id back to MAC format (dashes to colons)
    let device_mac = device_id.replace('-', ":");

    tracing::info!(
        device_id = %device_id,
        device_mac = %device_mac,
        width = width,
        height = height,
        "Image request received"
    );

    // Determine display spec from dimensions
    let spec = DisplaySpec::from_dimensions(width, height)?;

    // Try content pipeline first (for scripted content)
    let png_bytes = if content_pipeline
        .config()
        .get_screen_for_device(&device_mac)
        .is_some()
    {
        // Device has a configured screen - use the content pipeline
        // Run in spawn_blocking because Lua scripts use blocking HTTP requests
        // Convert Result to (Option<bytes>, Option<error_string>) to avoid Send issues
        let pipeline = content_pipeline.clone();
        let mac = device_mac.clone();
        let (bytes_opt, refresh_opt, error_opt) = tokio::task::spawn_blocking(move || {
            match pipeline.generate_for_device(&mac, spec) {
                Ok(result) => (Some(result.png_bytes), Some(result.refresh_rate), None),
                Err(e) => {
                    // Try to generate error screen
                    let error_msg = e.to_string();
                    match pipeline.generate_error(&error_msg, spec) {
                        Ok(bytes) => (Some(bytes), None, Some(error_msg)),
                        Err(render_err) => {
                            (None, None, Some(format!("Render error: {render_err}")))
                        }
                    }
                }
            }
        })
        .await
        .map_err(|e| ApiError::Internal(format!("Task error: {e}")))?;

        if let Some(bytes) = bytes_opt {
            if let Some(refresh_rate) = refresh_opt {
                tracing::info!(
                    refresh_rate = refresh_rate,
                    size_bytes = bytes.len(),
                    "Scripted content generated successfully"
                );
            } else if let Some(ref error_msg) = error_opt {
                tracing::error!(error = %error_msg, "Content pipeline error, showing error screen");
            }
            bytes
        } else {
            let error_msg = error_opt.unwrap_or_else(|| "Unknown error".to_string());
            tracing::error!(error = %error_msg, "Failed to render content");
            return Err(ApiError::Internal(error_msg));
        }
    } else {
        // No configured screen - use default static SVG
        renderer.render_default(spec).await?
    };

    tracing::info!(size_bytes = png_bytes.len(), "Image rendered successfully");

    // Return PNG as binary response
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CONTENT_LENGTH, &png_bytes.len().to_string()),
        ],
        Bytes::from(png_bytes),
    )
        .into_response())
}

/// Response from the /api/display endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct DisplayJsonResponse {
    /// Status code (0 = success, 202 = not registered, 500 = error)
    pub status: u16,
    /// Full URL to fetch the rendered image from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Image filename/identifier for caching
    pub filename: String,
    /// Whether device should update its firmware
    pub update_firmware: bool,
    /// URL to download firmware from (if update_firmware is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_url: Option<String>,
    /// How long device should sleep before next request (seconds)
    pub refresh_rate: u32,
    /// Whether device should reset its credentials
    pub reset_firmware: bool,
    /// Display temperature profile ('default', 'a', 'b', 'c')
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_profile: Option<String>,
    /// Special function to execute ('identify', 'sleep', etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_function: Option<String>,
}

#[allow(dead_code)]
pub async fn handle_display_json<R: DeviceRegistry>(
    State(registry): State<Arc<R>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    // Extract required headers
    let api_key_str = headers
        .get("Access-Token")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::MissingHeader("Access-Token"))?;

    let api_key = ApiKey::from_str(api_key_str);

    // Find device by API key
    let device = registry
        .find_by_api_key(&api_key)
        .await?
        .ok_or(ApiError::DeviceNotFound)?;

    tracing::info!(
        device_id = %device.device_id,
        "Display JSON request received"
    );

    // Return JSON with image_url pointing to static endpoint
    Ok(Json(DisplayJsonResponse {
        status: 200,
        image_url: Some("/static/images/default.png".to_string()),
        filename: "default".to_string(),
        update_firmware: false,
        firmware_url: None,
        refresh_rate: 900, // 15 minutes
        reset_firmware: false,
        temperature_profile: Some("default".to_string()),
        special_function: None,
    }))
}
