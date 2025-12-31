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
use crate::services::{
    CachedContent, ContentCache, ContentPipeline, DeviceContext, DeviceRegistry, RenderService,
};

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
    State(content_pipeline): State<Arc<ContentPipeline>>,
    State(content_cache): State<Arc<ContentCache>>,
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

    // Build device context for script
    let device_ctx = DeviceContext {
        mac: device.device_id.to_string(),
        battery_voltage: device.battery_voltage,
        rssi: device.rssi,
    };

    // Run script, render SVG, and cache the result (PNG rendering happens in /api/image)
    let device_mac = device.device_id.to_string();
    let pipeline = content_pipeline.clone();
    let mac = device_mac.clone();
    let cache = content_cache.clone();
    let ctx = device_ctx.clone();

    // Run in spawn_blocking because Lua scripts use blocking HTTP requests
    let (refresh_rate, skip_update, content_hash, error_msg) =
        tokio::task::spawn_blocking(move || {
            match pipeline.run_script_for_device(&mac, Some(ctx.clone())) {
                Ok(result) => {
                    if result.skip_update {
                        (result.refresh_rate, true, None, None)
                    } else {
                        // Render SVG from script result (template processing happens here)
                        match pipeline.render_svg_from_script(&result, Some(&ctx)) {
                            Ok(svg) => {
                                // Cache the pre-rendered SVG (keyed by content hash)
                                let cached = CachedContent::new(svg, result.screen_name.clone());
                                let hash = cached.content_hash.clone();
                                let rt = tokio::runtime::Handle::current();
                                rt.block_on(cache.store(cached));
                                (result.refresh_rate, false, Some(hash), None)
                            }
                            Err(e) => {
                                // Template rendering failed - cache error SVG
                                let error_msg = e.to_string();
                                let error_svg = pipeline.render_error_svg(&error_msg);
                                let cached = CachedContent::new(error_svg, "_error".to_string());
                                let hash = cached.content_hash.clone();
                                let rt = tokio::runtime::Handle::current();
                                rt.block_on(cache.store(cached));
                                (60, false, Some(hash), Some(error_msg))
                            }
                        }
                    }
                }
                Err(e) => {
                    // Script error - cache error SVG
                    let error_msg = e.to_string();
                    let error_svg = pipeline.render_error_svg(&error_msg);
                    let cached = CachedContent::new(error_svg, "_error".to_string());
                    let hash = cached.content_hash.clone();
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(cache.store(cached));
                    (60, false, Some(hash), Some(error_msg))
                }
            }
        })
        .await
        .map_err(|e| ApiError::Internal(format!("Task error: {e}")))?;

    if let Some(ref error) = error_msg {
        tracing::error!(error = %error, "Script error, cached for error rendering");
    } else if skip_update {
        tracing::info!(refresh_rate = refresh_rate, "Script returned skip_update");
    } else {
        tracing::info!(refresh_rate = refresh_rate, "Script output cached");
    }

    // Build image URL only if we have new content
    let image_url = if skip_update {
        tracing::info!(
            refresh_rate = refresh_rate,
            "Returning display response without image (skip_update)"
        );
        None
    } else if let Some(ref hash) = content_hash {
        // Build full image URL using content hash (no signature needed - hash is unpredictable)
        let host = headers
            .get("Host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost:3000");

        let url = format!("http://{host}/api/image/{hash}.png?w={width}&h={height}");

        tracing::info!(
            image_url = %url,
            refresh_rate = refresh_rate,
            "Returning display response with image URL"
        );
        Some(url)
    } else {
        None
    };

    // Return JSON response
    // Note: firmware expects status=0 for success (not 200!)
    // The filename is a hash of the SVG content, so TRMNL can detect changes
    Ok(Json(DisplayJsonResponse {
        status: 0,
        image_url,
        filename: content_hash.unwrap_or_else(|| "unchanged".to_string()),
        update_firmware: false,
        firmware_url: None,
        refresh_rate,
        reset_firmware: false,
        temperature_profile: Some("default".to_string()),
        special_function: None,
    })
    .into_response())
}

/// Get rendered PNG image by content hash
///
/// Returns the actual PNG image data rendered from SVG with dithering applied.
/// The content hash in the filename ensures uniqueness and enables client-side caching.
#[utoipa::path(
    get,
    path = "/api/image/{hash}.png",
    responses(
        (status = 200, description = "PNG image", content_type = "image/png"),
        (status = 404, description = "Content not found"),
        (status = 500, description = "Rendering error"),
    ),
    params(
        ("hash" = String, Path, description = "Content hash (from /api/display response)"),
        ("w" = Option<u32>, Query, description = "Display width in pixels (default: 800)"),
        ("h" = Option<u32>, Query, description = "Display height in pixels (default: 480)"),
    ),
    tag = "Display"
)]
pub async fn handle_image<R: DeviceRegistry>(
    State(_registry): State<Arc<R>>,
    State(_renderer): State<Arc<RenderService>>,
    State(content_cache): State<Arc<ContentCache>>,
    State(content_pipeline): State<Arc<ContentPipeline>>,
    Path(hash_with_ext): Path<String>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, ApiError> {
    // Strip .png extension from hash
    let content_hash = hash_with_ext
        .strip_suffix(".png")
        .unwrap_or(&hash_with_ext);

    // Parse dimensions from query parameters
    let width: u32 = query
        .w
        .filter(|&w| w > 0 && w <= MAX_DISPLAY_WIDTH)
        .unwrap_or(800);
    let height: u32 = query
        .h
        .filter(|&h| h > 0 && h <= MAX_DISPLAY_HEIGHT)
        .unwrap_or(480);

    let spec = DisplaySpec::from_dimensions(width, height)?;

    tracing::info!(
        content_hash = %content_hash,
        width = width,
        height = height,
        "Image request received"
    );

    // Get cached SVG by content hash and render to PNG
    let png_bytes = if let Some(cached) = content_cache.get(content_hash).await {
        tracing::info!(
            screen = %cached.screen_name,
            content_hash = %cached.content_hash,
            age_secs = (chrono::Utc::now() - cached.generated_at).num_seconds(),
            "Rendering PNG from cached SVG"
        );
        content_pipeline.render_png_from_svg(&cached.rendered_svg, spec)?
    } else {
        tracing::warn!(
            content_hash = %content_hash,
            "Content not found in cache"
        );
        return Err(ApiError::NotFound);
    };

    tracing::info!(size_bytes = png_bytes.len(), "Image rendered and served");

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
