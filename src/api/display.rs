use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use super::headers::HeaderMapExt;
use crate::error::ApiError;
use crate::models::{ApiKey, AppConfig, Device, DeviceId, DeviceModel, DisplaySpec};
use crate::services::{
    CachedContent, ContentCache, ContentPipeline, DeviceContext, DeviceRegistry, RenderService,
};

// Maximum allowed display dimensions to prevent DoS
const MAX_DISPLAY_WIDTH: u32 = 2000;
const MAX_DISPLAY_HEIGHT: u32 = 2000;

/// Get display content for a device
///
/// Returns JSON with an image_url that the device should fetch separately.
/// The firmware expects status=0 for success (not HTTP 200).
///
/// If device registration is enabled and the device is not registered,
/// returns a registration screen showing the device's registration code.
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
    State(config): State<Arc<AppConfig>>,
    State(registry): State<Arc<R>>,
    State(_renderer): State<Arc<RenderService>>,
    State(content_pipeline): State<Arc<ContentPipeline>>,
    State(content_cache): State<Arc<ContentCache>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Extract required headers
    let api_key_str = headers.require_str("Access-Token")?;
    let device_id_str = headers.require_str("ID")?;

    // Parse and validate dimensions with bounds checking
    let width: u32 = headers
        .get_parsed_filtered("Width", |&w| w > 0 && w <= MAX_DISPLAY_WIDTH)
        .unwrap_or(800);
    let height: u32 = headers
        .get_parsed_filtered("Height", |&h| h > 0 && h <= MAX_DISPLAY_HEIGHT)
        .unwrap_or(480);

    let api_key = ApiKey::new(api_key_str);

    // Check device registration when enabled
    // Registration uses the API key's embedded code OR MAC address to identify devices
    if config.registration.enabled {
        let registration_code = api_key.registration_code();

        if !api_key.is_byonk_key() {
            // Non-Byonk key - tell device to reset and call /api/setup
            tracing::info!(
                api_key_prefix = &api_key_str[..api_key_str.len().min(8)],
                "Non-Byonk API key detected, requesting device reset"
            );

            return Ok(Json(DisplayJsonResponse {
                status: 0,
                image_url: None,
                filename: "reset".to_string(),
                update_firmware: false,
                firmware_url: None,
                refresh_rate: 60,
                reset_firmware: true, // Force device to call /api/setup again
                temperature_profile: Some("default".to_string()),
                special_function: None,
            })
            .into_response());
        }

        // Check if device is registered (by MAC address OR by registration code)
        if !config.is_device_registered(device_id_str, registration_code) {
            let code = registration_code.unwrap_or("UNKNOWN");
            tracing::info!(
                device_id = device_id_str,
                registration_code = code,
                custom_screen = config.registration.screen.as_deref(),
                "Device not registered, showing registration screen"
            );

            // Build device context with registration code
            let model_str = headers.get_str("Model").unwrap_or("og");
            let device_ctx = DeviceContext {
                mac: device_id_str.to_string(),
                battery_voltage: headers.get_parsed::<f32>("Battery-Voltage"),
                rssi: headers.get_parsed::<i32>("RSSI"),
                model: Some(model_str.to_string()),
                firmware_version: headers.get_str("FW-Version").map(|s| s.to_string()),
                width: Some(width),
                height: Some(height),
                grey_levels: None,
                registration_code: Some(code.to_string()),
            };

            // Use registration screen if configured, otherwise use default screen
            // Both have device.registration_code available for display
            let screen_to_use = config
                .registration
                .screen
                .as_deref()
                .or(config.default_screen.as_deref());

            let (registration_svg, screen_name, refresh_rate) = if let Some(screen_name) =
                screen_to_use
            {
                // Run the registration/default screen (code available via device.registration_code)
                match content_pipeline.run_screen_by_name(
                    screen_name,
                    std::collections::HashMap::new(),
                    Some(device_ctx.clone()),
                ) {
                    Ok(result) => {
                        match content_pipeline.render_svg_from_script(&result, Some(&device_ctx)) {
                            Ok(svg) => (svg, screen_name.to_string(), result.refresh_rate),
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    screen = screen_name,
                                    "Registration screen template failed, using built-in"
                                );
                                (
                                    content_pipeline
                                        .render_registration_screen(code, width, height),
                                    "_registration".to_string(),
                                    300,
                                )
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            screen = screen_name,
                            "Registration screen failed, using built-in"
                        );
                        (
                            content_pipeline.render_registration_screen(code, width, height),
                            "_registration".to_string(),
                            300,
                        )
                    }
                }
            } else {
                // No default screen configured, use built-in registration screen
                (
                    content_pipeline.render_registration_screen(code, width, height),
                    "_registration".to_string(),
                    300,
                )
            };

            let cached = CachedContent::new(registration_svg, screen_name, width, height);
            let hash = cached.content_hash.clone();
            content_cache.store(cached);

            // Build image URL
            let host = headers.get_str("Host").unwrap_or("localhost:3000");
            let image_url = format!("http://{host}/api/image/{hash}.png");

            return Ok(Json(DisplayJsonResponse {
                status: 0,
                image_url: Some(image_url),
                filename: hash,
                update_firmware: false,
                firmware_url: None,
                refresh_rate,
                reset_firmware: false,
                temperature_profile: Some("default".to_string()),
                special_function: None,
            })
            .into_response());
        }
        // Device is registered - continue with normal flow
    }

    // Find device by API key, or auto-register if not found
    let mut device = match registry.find_by_api_key(&api_key).await? {
        Some(d) => d,
        None => {
            // Auto-register the device using the MAC address from ID header
            let device_id = DeviceId::new(device_id_str);
            let model_str = headers.get_str("Model").unwrap_or("og");
            let fw_version = headers
                .get_str("FW-Version")
                .unwrap_or("unknown")
                .to_string();

            let model = DeviceModel::parse(model_str);
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
    if let Some(battery) = headers.get_parsed::<f32>("Battery-Voltage") {
        device.battery_voltage = Some(battery);
    }
    if let Some(rssi) = headers.get_parsed::<i32>("RSSI") {
        device.rssi = Some(rssi);
    }

    device.last_seen = chrono::Utc::now();
    registry.update(device.clone()).await?;

    // Build device context for script
    let device_ctx = DeviceContext {
        mac: device.device_id.to_string(),
        battery_voltage: device.battery_voltage,
        rssi: device.rssi,
        model: Some(device.model.to_string()),
        firmware_version: Some(device.firmware_version.clone()),
        width: Some(width),
        height: Some(height),
        grey_levels: None, // Determined by device model in render phase
        registration_code: device.api_key.registration_code().map(|s| s.to_string()),
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
                                let cached = CachedContent::new(
                                    svg,
                                    result.screen_name.clone(),
                                    width,
                                    height,
                                );
                                let hash = cached.content_hash.clone();
                                cache.store(cached);
                                (result.refresh_rate, false, Some(hash), None)
                            }
                            Err(e) => {
                                // Template rendering failed - cache error SVG
                                let error_msg = e.to_string();
                                let error_svg = pipeline.render_error_svg(&error_msg);
                                let cached = CachedContent::new(
                                    error_svg,
                                    "_error".to_string(),
                                    width,
                                    height,
                                );
                                let hash = cached.content_hash.clone();
                                cache.store(cached);
                                (60, false, Some(hash), Some(error_msg))
                            }
                        }
                    }
                }
                Err(e) => {
                    // Script error - cache error SVG
                    let error_msg = e.to_string();
                    let error_svg = pipeline.render_error_svg(&error_msg);
                    let cached = CachedContent::new(error_svg, "_error".to_string(), width, height);
                    let hash = cached.content_hash.clone();
                    cache.store(cached);
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
        let host = headers.get_str("Host").unwrap_or("localhost:3000");

        let url = format!("http://{host}/api/image/{hash}.png");

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
    ),
    tag = "Display"
)]
pub async fn handle_image<R: DeviceRegistry>(
    State(_registry): State<Arc<R>>,
    State(_renderer): State<Arc<RenderService>>,
    State(content_cache): State<Arc<ContentCache>>,
    State(content_pipeline): State<Arc<ContentPipeline>>,
    Path(hash_with_ext): Path<String>,
) -> Result<Response, ApiError> {
    // Strip .png extension from hash
    let content_hash = hash_with_ext.strip_suffix(".png").unwrap_or(&hash_with_ext);

    // Get cached SVG by content hash and render to PNG
    let cached = content_cache.get(content_hash).ok_or_else(|| {
        tracing::warn!(content_hash = %content_hash, "Content not found in cache");
        ApiError::NotFound
    })?;

    let spec = DisplaySpec::from_dimensions(cached.width, cached.height)?;

    tracing::info!(
        content_hash = %content_hash,
        screen = %cached.screen_name,
        width = cached.width,
        height = cached.height,
        age_secs = (chrono::Utc::now() - cached.generated_at).num_seconds(),
        "Rendering PNG from cached SVG"
    );

    let png_bytes = content_pipeline.render_png_from_svg(&cached.rendered_svg, spec)?;

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
