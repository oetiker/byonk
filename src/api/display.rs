use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use utoipa::ToSchema;

use super::headers::HeaderMapExt;
use crate::error::ApiError;
use crate::models::{
    verify_ed25519_signature, ApiKey, AppConfig, Device, DeviceId, DeviceModel, DisplaySpec,
};
use crate::services::{
    CachedContent, ContentCache, ContentPipeline, DeviceContext, DeviceRegistry, RenderService,
};

// Maximum allowed display dimensions to prevent DoS
const MAX_DISPLAY_WIDTH: u32 = 2000;
const MAX_DISPLAY_HEIGHT: u32 = 2000;

/// Default 4-grey palette for devices that don't send a Colors header
const DEFAULT_COLORS: &str = "#000000,#555555,#AAAAAA,#FFFFFF";

/// Parse a comma-separated list of hex RGB color strings into RGB tuples
pub fn parse_colors_header(s: &str) -> Vec<(u8, u8, u8)> {
    s.split(',')
        .filter_map(|c| {
            let c = c.trim().trim_start_matches('#');
            if c.len() == 6 {
                let r = u8::from_str_radix(&c[0..2], 16).ok()?;
                let g = u8::from_str_radix(&c[2..4], 16).ok()?;
                let b = u8::from_str_radix(&c[4..6], 16).ok()?;
                Some((r, g, b))
            } else {
                None
            }
        })
        .collect()
}

/// Convert RGB tuples back to hex strings for Lua/template exposure
pub fn colors_to_hex_strings(colors: &[(u8, u8, u8)]) -> Vec<String> {
    colors
        .iter()
        .map(|(r, g, b)| format!("#{:02X}{:02X}{:02X}", r, g, b))
        .collect()
}

/// Resolved rendering parameters for the dithering pipeline.
pub struct RenderParams {
    pub palette: Vec<(u8, u8, u8)>,
    pub measured_colors: Option<Vec<(u8, u8, u8)>>,
    pub dither: Option<String>,
    pub preserve_exact: bool,
}

/// Resolve the pre-script palette for device context.
/// Chain: device_config_colors > panel_colors > fallback_palette
pub fn resolve_ctx_palette(
    device_config_colors: Option<&str>,
    panel_colors: Option<&str>,
    fallback_palette: &[(u8, u8, u8)],
) -> Vec<(u8, u8, u8)> {
    if let Some(cc) = device_config_colors {
        parse_colors_header(cc)
    } else if let Some(pc) = panel_colors {
        parse_colors_header(pc)
    } else {
        fallback_palette.to_vec()
    }
}

/// Resolve all rendering parameters after script execution.
///
/// Palette: script_colors > device_config_colors > panel_colors > fallback
/// Dither: script_dither > device_config_dither > None
/// Preserve: script_preserve_exact > override > true
#[allow(clippy::too_many_arguments)]
pub fn resolve_render_params(
    script_colors: Option<&[String]>,
    script_dither: Option<&str>,
    script_preserve_exact: Option<bool>,
    device_config_colors: Option<&str>,
    device_config_dither: Option<&str>,
    panel_colors: Option<&str>,
    fallback_palette: &[(u8, u8, u8)],
    measured_colors: Option<Vec<(u8, u8, u8)>>,
    preserve_exact_override: Option<bool>,
) -> RenderParams {
    let palette = if let Some(sc) = script_colors {
        parse_colors_header(&sc.join(","))
    } else if let Some(cc) = device_config_colors {
        parse_colors_header(cc)
    } else if let Some(pc) = panel_colors {
        parse_colors_header(pc)
    } else {
        fallback_palette.to_vec()
    };

    let dither = script_dither
        .map(|s| s.to_string())
        .or_else(|| device_config_dither.map(|s| s.to_string()));

    let preserve_exact = script_preserve_exact
        .or(preserve_exact_override)
        .unwrap_or(true);

    RenderParams {
        palette,
        measured_colors,
        dither,
        preserve_exact,
    }
}

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
    State(panel_color_overrides): State<Arc<RwLock<std::collections::HashMap<String, String>>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Extract required headers
    let device_id_str = headers.require_str("ID")?;

    // Check for Ed25519 authentication headers
    let has_ed25519 = headers.get_str("X-Public-Key").is_some()
        && headers.get_str("X-Signature").is_some()
        && headers.get_str("X-Timestamp").is_some();

    // Ed25519 public key for registration code derivation (if present)
    let ed25519_public_key: Option<String> = if has_ed25519 {
        headers.get_str("X-Public-Key").map(|s| s.to_string())
    } else {
        None
    };

    if has_ed25519 {
        let public_key_hex = headers.require_str("X-Public-Key")?;
        let signature_hex = headers.require_str("X-Signature")?;
        let timestamp_str = headers.require_str("X-Timestamp")?;

        tracing::debug!(
            device_id = device_id_str,
            public_key = public_key_hex,
            signature = signature_hex,
            timestamp = timestamp_str,
            public_key_len = public_key_hex.len(),
            signature_len = signature_hex.len(),
            "Ed25519 auth attempt"
        );

        let timestamp_ms: u64 = timestamp_str
            .parse()
            .map_err(|_| ApiError::MissingHeader("X-Timestamp"))?;

        verify_ed25519_signature(public_key_hex, signature_hex, timestamp_ms).map_err(|e| {
            tracing::warn!(
                device_id = device_id_str,
                error = %e,
                "Ed25519 authentication failed"
            );
            ApiError::Internal(format!("Authentication failed: {e}"))
        })?;

        tracing::info!(
            device_id = device_id_str,
            "Ed25519 authentication successful"
        );
    }

    // Access-Token is still required (used for registration code derivation)
    let api_key_str = headers.require_str("Access-Token")?;

    // Parse and validate dimensions with bounds checking
    let width: u32 = headers
        .get_parsed_filtered("Width", |&w| w > 0 && w <= MAX_DISPLAY_WIDTH)
        .unwrap_or(800);
    let height: u32 = headers
        .get_parsed_filtered("Height", |&h| h > 0 && h <= MAX_DISPLAY_HEIGHT)
        .unwrap_or(480);

    let api_key = ApiKey::new(api_key_str);

    // Derive registration code: Ed25519 public key takes priority over API key.
    // This ensures the registration code is stable even if the API key changes.
    let identity_key = if let Some(ref pk) = ed25519_public_key {
        ApiKey::new(pk)
    } else {
        api_key.clone()
    };

    // Check device registration when enabled
    // Registration uses the identity key's derived code OR MAC address to identify devices
    if config.registration.enabled {
        let registration_code = identity_key.registration_code();

        // Check if device is registered (by MAC address OR by registration code)
        if !config.is_device_registered(device_id_str, Some(&registration_code)) {
            tracing::info!(
                device_id = device_id_str,
                registration_code = %registration_code,
                custom_screen = config.registration.screen.as_deref(),
                "Device not registered, showing registration screen"
            );
            let code = registration_code.as_str();

            // Build device context with registration code
            let model_str = headers.get_str("Model").unwrap_or("og");
            let colors_str = headers.get_str("Colors").unwrap_or(DEFAULT_COLORS);
            let palette = parse_colors_header(colors_str);
            let color_hex = colors_to_hex_strings(&palette);
            let device_ctx = DeviceContext {
                mac: device_id_str.to_string(),
                battery_voltage: headers.get_parsed::<f32>("Battery-Voltage"),
                rssi: headers.get_parsed::<i32>("RSSI"),
                model: Some(model_str.to_string()),
                firmware_version: headers.get_str("FW-Version").map(|s| s.to_string()),
                width: Some(width),
                height: Some(height),

                registration_code: Some(code.to_string()),
                board: headers.get_str("Board").map(|s| s.to_string()),
                colors: Some(color_hex.clone()),
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

            let cached = CachedContent::new(registration_svg, screen_name, width, height)
                .with_colors(Some(palette));
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

    // Get or create device metadata
    let device_id = DeviceId::new(device_id_str);
    let model_str = headers.get_str("Model").unwrap_or("og");
    let model = DeviceModel::parse(model_str);
    let fw_version = headers
        .get_str("FW-Version")
        .unwrap_or("unknown")
        .to_string();

    let mut device = registry
        .find_by_id(&device_id)
        .await?
        .unwrap_or_else(|| Device::new(device_id.clone(), model, fw_version.clone()));

    tracing::info!(
        device_id = %device_id,
        registration_code = %identity_key.registration_code(),
        board = ?headers.get_str("Board"),
        width = width,
        height = height,
        "Display request received"
    );

    // Update device metadata
    device.model = model;
    device.firmware_version = fw_version;
    if let Some(battery) = headers.get_parsed::<f32>("Battery-Voltage") {
        device.battery_voltage = Some(battery);
    }
    if let Some(rssi) = headers.get_parsed::<i32>("RSSI") {
        device.rssi = Some(rssi);
    }
    device.last_seen = chrono::Utc::now();
    registry.upsert(device.clone()).await?;

    // Parse color palette from headers (firmware Colors header)
    let header_colors_str = headers.get_str("Colors");
    let initial_colors_str = header_colors_str.unwrap_or(DEFAULT_COLORS);
    let initial_palette = parse_colors_header(initial_colors_str);

    // Resolve panel and device config for this device
    let board_header = headers.get_str("Board").map(|s| s.to_string());
    let registration_code = identity_key.registration_code();

    let (device_config, device_config_key) =
        if let Some(dc) = config.get_device_config(device_id_str) {
            (Some(dc), Some(device_id_str.to_string()))
        } else if let Some(dc) = config.get_device_config_for_code(&registration_code) {
            (Some(dc), Some(registration_code.clone()))
        } else {
            (None, None)
        };
    let device_config_panel = device_config.and_then(|dc| dc.panel.clone());
    let device_config_colors = device_config.and_then(|dc| dc.colors.clone());

    let (panel, panel_source) = if let Some(ref panel_name) = device_config_panel {
        if let Some(p) = config.get_panel(panel_name) {
            (Some(p), Some(format!("device_config:{}", panel_name)))
        } else {
            (None, None)
        }
    } else if let Some((name, p)) = board_header
        .as_deref()
        .and_then(|b| config.find_panel_for_board(b))
    {
        (Some(p), Some(format!("board_header:{}", name)))
    } else {
        (None, None)
    };

    // Resolve measured colors: dev override > panel.colors_actual > Measured-Colors header > None
    // Dev overrides (from color adjustment popup) take highest priority when present.
    // Config always trumps firmware-reported values.
    let measured_colors_header = headers.get_str("Measured-Colors").map(|s| s.to_string());
    let panel_name_for_override = device_config_panel.as_deref();
    let override_colors = if let Some(pname) = panel_name_for_override {
        panel_color_overrides.read().await.get(pname).cloned()
    } else {
        None
    };
    let measured_source;
    let measured_colors: Option<Vec<(u8, u8, u8)>> = if let Some(ref oc) = override_colors {
        measured_source = "dev_override";
        Some(parse_colors_header(oc))
    } else if let Some(actual) = panel.and_then(|p| p.colors_actual.as_deref()) {
        measured_source = "panel.colors_actual";
        Some(parse_colors_header(actual))
    } else if let Some(ref hdr) = measured_colors_header {
        measured_source = "Measured-Colors header";
        Some(parse_colors_header(hdr))
    } else {
        measured_source = "none";
        None
    };

    // Panel official colors for palette chain
    let panel_colors_for_chain: Option<String> = panel.map(|p| p.colors.clone());

    tracing::debug!(
        device_id = device_id_str,
        registration_code = %registration_code,
        device_config_key = ?device_config_key,
        device_config_screen = ?device_config.map(|dc| &dc.screen),
        panel_source = ?panel_source,
        panel_colors = ?panel_colors_for_chain,
        panel_colors_actual = ?panel.and_then(|p| p.colors_actual.as_deref()),
        board_header = ?board_header,
        measured_colors_header = ?measured_colors_header,
        measured_source = measured_source,
        "Device config and panel resolution"
    );

    // Pre-script resolved palette for device context:
    // device_config_colors > panel_colors > firmware header > default
    // Matches the resolve_palette chain (minus script_colors, unknown until script runs)
    let device_config_dither = device_config.and_then(|dc| dc.dither.clone());
    let ctx_palette = resolve_ctx_palette(
        device_config_colors.as_deref(),
        panel_colors_for_chain.as_deref(),
        &initial_palette,
    );
    let ctx_color_hex = colors_to_hex_strings(&ctx_palette);

    // Build device context for script (palette matches dithering chain)
    let device_ctx = DeviceContext {
        mac: device.device_id.to_string(),
        battery_voltage: device.battery_voltage,
        rssi: device.rssi,
        model: Some(device.model.to_string()),
        firmware_version: Some(device.firmware_version.clone()),
        width: Some(width),
        height: Some(height),
        registration_code: Some(registration_code),
        board: board_header.clone(),
        colors: Some(ctx_color_hex),
    };

    // Run script, render SVG, and cache the result (PNG rendering happens in /api/image)
    let device_mac = device.device_id.to_string();
    let pipeline = content_pipeline.clone();
    let mac = device_mac.clone();
    let cache = content_cache.clone();
    let ctx = device_ctx.clone();
    let header_colors_for_chain = header_colors_str.map(|s| s.to_string());
    let dc_colors = device_config_colors;
    let dc_dither = device_config_dither;

    // Run in spawn_blocking because Lua scripts use blocking HTTP requests
    let (refresh_rate, skip_update, content_hash, error_msg) =
        tokio::task::spawn_blocking(move || {
            match pipeline.run_script_for_device(&mac, Some(ctx.clone())) {
                Ok(result) => {
                    let fallback = header_colors_for_chain
                        .as_deref()
                        .map(parse_colors_header)
                        .unwrap_or_else(|| parse_colors_header(DEFAULT_COLORS));

                    tracing::debug!(
                        device = %mac,
                        script_colors = ?result.script_colors,
                        script_dither = ?result.script_dither,
                        script_preserve_exact = ?result.script_preserve_exact,
                        dc_colors = ?dc_colors,
                        dc_dither = ?dc_dither,
                        panel_colors = ?panel_colors_for_chain,
                        header_colors = ?header_colors_for_chain,
                        measured_colors = ?measured_colors.as_ref().map(|mc| colors_to_hex_strings(mc)),
                        "Resolving render params for device"
                    );

                    let params = resolve_render_params(
                        result.script_colors.as_deref(),
                        result.script_dither.as_deref(),
                        result.script_preserve_exact,
                        dc_colors.as_deref(),
                        dc_dither.as_deref(),
                        panel_colors_for_chain.as_deref(),
                        &fallback,
                        measured_colors.clone(),
                        None,
                    );

                    tracing::debug!(
                        device = %mac,
                        resolved_palette = ?colors_to_hex_strings(&params.palette),
                        resolved_dither = ?params.dither,
                        resolved_preserve_exact = params.preserve_exact,
                        has_measured = params.measured_colors.is_some(),
                        "Resolved render params"
                    );

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
                                )
                                .with_colors(Some(params.palette))
                                .with_colors_actual(params.measured_colors)
                                .with_dither(params.dither)
                                .with_preserve_exact(Some(params.preserve_exact));
                                let hash = cached.content_hash.clone();
                                cache.store(cached);
                                (result.refresh_rate, false, Some(hash), None)
                            }
                            Err(e) => {
                                // Template rendering failed - cache error SVG
                                let error_msg = e.to_string();
                                let error_svg = pipeline.render_error_svg(&error_msg);
                                let fallback_palette = resolve_ctx_palette(
                                    dc_colors.as_deref(),
                                    panel_colors_for_chain.as_deref(),
                                    &fallback,
                                );
                                let cached = CachedContent::new(
                                    error_svg,
                                    "_error".to_string(),
                                    width,
                                    height,
                                )
                                .with_colors(Some(fallback_palette));
                                let hash = cached.content_hash.clone();
                                cache.store(cached);
                                (60, false, Some(hash), Some(error_msg))
                            }
                        }
                    }
                }
                Err(e) => {
                    // Script error - cache error SVG
                    let fallback = header_colors_for_chain
                        .as_deref()
                        .map(parse_colors_header)
                        .unwrap_or_else(|| parse_colors_header(DEFAULT_COLORS));
                    let fallback_palette = resolve_ctx_palette(
                        dc_colors.as_deref(),
                        panel_colors_for_chain.as_deref(),
                        &fallback,
                    );
                    let error_msg = e.to_string();
                    let error_svg = pipeline.render_error_svg(&error_msg);
                    let cached = CachedContent::new(error_svg, "_error".to_string(), width, height)
                        .with_colors(Some(fallback_palette));
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

    // Colors are always stored in cache by the display handler
    let fallback_palette = parse_colors_header(DEFAULT_COLORS);
    let palette = cached.colors.as_deref().unwrap_or(&fallback_palette);
    let dither = cached.dither.as_deref();
    let colors_actual = cached.colors_actual.as_deref();
    let preserve_exact = cached.preserve_exact.unwrap_or(true);

    tracing::debug!(
        content_hash = %content_hash,
        palette = ?colors_to_hex_strings(palette),
        colors_actual = ?colors_actual.map(colors_to_hex_strings),
        dither = ?dither,
        preserve_exact = preserve_exact,
        "Dither parameters for PNG render"
    );

    let png_bytes = content_pipeline.render_png_from_svg(
        &cached.rendered_svg,
        spec,
        palette,
        colors_actual,
        false, // production always uses official colors
        dither,
        preserve_exact,
    )?;

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
