//! Dev mode API endpoints.
//!
//! These endpoints are only available when running `byonk dev`.

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Json, Response,
    },
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::assets::AssetLoader;
use crate::models::{AppConfig, DisplaySpec, ScreenConfig};
use crate::services::{ContentCache, ContentPipeline, DeviceContext, FileWatcher, RenderService};

/// Dev mode application state
#[derive(Clone)]
pub struct DevState {
    pub config: Arc<AppConfig>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
    pub renderer: Arc<RenderService>,
    pub file_watcher: Arc<FileWatcher>,
    pub asset_loader: Arc<AssetLoader>,
}

/// Query parameters for /dev/render
#[derive(Debug, Deserialize)]
pub struct RenderQuery {
    /// Screen name from config (optional if mac is provided)
    pub screen: Option<String>,
    /// MAC address to resolve screen from config
    pub mac: Option<String>,
    /// Device model: "og" or "x"
    #[serde(default = "default_model")]
    pub model: String,
    /// Display width (overrides model default)
    pub width: Option<u32>,
    /// Display height (overrides model default)
    pub height: Option<u32>,
    /// Battery voltage (default: 4.2)
    pub battery_voltage: Option<f32>,
    /// WiFi RSSI in dBm (default: -50)
    pub rssi: Option<i32>,
    /// Unix timestamp override for time_now() in Lua
    pub timestamp: Option<i64>,
    /// Custom parameters as JSON string
    pub params: Option<String>,
    /// Display colors as comma-separated hex RGB (e.g. "#000000,#FFFFFF,#FF0000,#FFFF00")
    pub colors: Option<String>,
}

/// Convert serde_yaml params to serde_json values
fn yaml_params_to_json(
    params: &HashMap<String, serde_yaml::Value>,
) -> HashMap<String, serde_json::Value> {
    params
        .iter()
        .filter_map(|(k, v)| serde_json::to_value(v).ok().map(|jv| (k.clone(), jv)))
        .collect()
}

fn default_model() -> String {
    "og".to_string()
}

/// Query parameters for /dev/resolve-mac
#[derive(Debug, Deserialize)]
pub struct ResolveMacQuery {
    pub mac: String,
}

/// Response from /dev/resolve-mac
#[derive(Debug, Serialize)]
pub struct ResolveMacResponse {
    pub screen: String,
    pub params: HashMap<String, serde_json::Value>,
}

/// Screen info for the screens list
#[derive(Debug, Serialize)]
pub struct ScreenInfo {
    pub name: String,
    pub script: String,
    pub template: String,
    pub default_refresh: u32,
}

/// Device info for the screens list
#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub screen: String,
}

/// Response from /dev/screens
#[derive(Debug, Serialize)]
pub struct ScreensResponse {
    pub screens: Vec<ScreenInfo>,
    pub devices: Vec<DeviceInfo>,
    pub default_screen: Option<String>,
}

/// Response from /dev/render when there's an error
#[derive(Debug, Serialize)]
pub struct RenderErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

/// Serve the dev mode HTML page
pub async fn handle_dev_page() -> Html<&'static str> {
    Html(include_str!("../../static/dev/index.html"))
}

/// Serve the dev mode CSS
pub async fn handle_dev_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../static/dev/dev.css"),
    )
}

/// Serve the dev mode JavaScript
pub async fn handle_dev_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("../../static/dev/dev.js"),
    )
}

/// List available screens by discovering them from the filesystem.
///
/// Screens are discovered by scanning for `.lua` files with matching `.svg` templates.
/// Config-defined screens are included first, then any filesystem screens not in config
/// are auto-discovered. This allows new screens to appear without restarting the server.
pub async fn handle_screens(State(state): State<DevState>) -> Json<ScreensResponse> {
    let mut seen = std::collections::HashSet::new();

    // Start with config-defined screens (they may have custom settings)
    let mut screens: Vec<ScreenInfo> = state
        .config
        .screens
        .iter()
        .map(|(name, config)| {
            seen.insert(name.clone());
            ScreenInfo {
                name: name.clone(),
                script: config.script.display().to_string(),
                template: config.template.display().to_string(),
                default_refresh: config.default_refresh,
            }
        })
        .collect();

    // Auto-discover screens from filesystem that aren't in config
    let all_files = state.asset_loader.list_screens();
    for file in &all_files {
        if let Some(name) = file.strip_suffix(".lua") {
            // Skip subdirectories (layouts/, components/, etc.) and already-seen screens
            if name.contains('/') || seen.contains(name) {
                continue;
            }
            // Check that a matching .svg template exists
            let svg_file = format!("{}.svg", name);
            if all_files.iter().any(|f| f == &svg_file) {
                seen.insert(name.to_string());
                screens.push(ScreenInfo {
                    name: name.to_string(),
                    script: format!("{}.lua", name),
                    template: svg_file,
                    default_refresh: 900,
                });
            }
        }
    }

    // Collect configured devices
    let devices: Vec<DeviceInfo> = state
        .config
        .devices
        .iter()
        .map(|(id, dev)| DeviceInfo {
            id: id.clone(),
            screen: dev.screen.clone(),
        })
        .collect();

    Json(ScreensResponse {
        screens,
        devices,
        default_screen: state.config.default_screen.clone(),
    })
}

/// SSE endpoint for file change events
pub async fn handle_events(
    State(state): State<DevState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.file_watcher.subscribe();

    let stream = BroadcastStream::new(rx).map(|result| {
        match result {
            Ok(event) => {
                let paths: Vec<String> = event
                    .paths
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect();
                Ok(Event::default()
                    .event("file-change")
                    .data(serde_json::to_string(&paths).unwrap_or_default()))
            }
            Err(_) => {
                // Lagged - just send a generic refresh event
                Ok(Event::default().event("refresh").data("lagged"))
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Resolve MAC address to screen and params
pub async fn handle_resolve_mac(
    State(state): State<DevState>,
    Query(query): Query<ResolveMacQuery>,
) -> Response {
    // Try MAC lookup first, then registration code lookup
    let lookup = state
        .config
        .get_screen_for_device(&query.mac)
        .or_else(|| state.config.get_screen_for_code(&query.mac));

    match lookup {
        Some((screen_config, device_config)) => {
            // Convert YAML params to JSON for response
            let params: HashMap<String, serde_json::Value> =
                yaml_params_to_json(&device_config.params);

            // Get screen name from the script path
            let screen_name = screen_config
                .script
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| device_config.screen.clone());

            Json(ResolveMacResponse {
                screen: screen_name,
                params,
            })
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(RenderErrorResponse {
                error: format!("Device '{}' not configured", query.mac),
                details: None,
            }),
        )
            .into_response(),
    }
}

/// Render a screen and return PNG
pub async fn handle_render(
    State(state): State<DevState>,
    Query(query): Query<RenderQuery>,
) -> Response {
    // Resolve screen config: MAC takes precedence, then explicit screen
    let (screen_config, resolved_params, device_config_colors) = if let Some(ref mac) = query.mac {
        // Try MAC lookup first, then registration code lookup
        match state
            .config
            .get_screen_for_device(mac)
            .or_else(|| state.config.get_screen_for_code(mac))
        {
            Some((sc, dc)) => {
                // Convert device params to JSON
                let params: HashMap<String, serde_json::Value> = yaml_params_to_json(&dc.params);
                (sc.clone(), Some(params), dc.colors.clone())
            }
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(RenderErrorResponse {
                        error: format!("Device '{}' not configured", mac),
                        details: None,
                    }),
                )
                    .into_response();
            }
        }
    } else if let Some(ref screen_name) = query.screen {
        if let Some(config) = state.config.screens.get(screen_name) {
            (config.clone(), None, None)
        } else {
            // Try auto-discovering screen from filesystem
            let script = format!("{}.lua", screen_name);
            let template = format!("{}.svg", screen_name);
            let files = state.asset_loader.list_screens();
            if files.iter().any(|f| f == &script) && files.iter().any(|f| f == &template) {
                (
                    ScreenConfig {
                        script: PathBuf::from(&script),
                        template: PathBuf::from(&template),
                        default_refresh: 900,
                    },
                    None,
                    None,
                )
            } else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(RenderErrorResponse {
                        error: format!("Screen '{}' not found", screen_name),
                        details: None,
                    }),
                )
                    .into_response();
            }
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(RenderErrorResponse {
                error: "Either 'screen' or 'mac' parameter is required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    // Determine dimensions from model or explicit values
    let (width, height) = match query.model.as_str() {
        "x" => (query.width.unwrap_or(1872), query.height.unwrap_or(1404)),
        _ => (query.width.unwrap_or(800), query.height.unwrap_or(480)),
    };

    // Parse custom params (these override resolved params from MAC)
    let mut custom_params: HashMap<String, serde_json::Value> = resolved_params.unwrap_or_default();

    // Merge query params on top of resolved params
    if let Some(ref params_str) = query.params {
        if let Ok(query_params) =
            serde_json::from_str::<HashMap<String, serde_json::Value>>(params_str)
        {
            custom_params.extend(query_params);
        }
    }

    // Build initial palette from colors param or model default (acts as "firmware header")
    // Priority chain: script_colors > device_config_colors > query.colors > model default
    let query_palette: Vec<(u8, u8, u8)> = if let Some(ref colors_str) = query.colors {
        crate::api::display::parse_colors_header(colors_str)
    } else if query.model == "x" {
        (0..16)
            .map(|i| {
                let v = (i * 255 / 15) as u8;
                (v, v, v)
            })
            .collect()
    } else {
        crate::api::display::parse_colors_header("#000000,#555555,#AAAAAA,#FFFFFF")
    };

    // Apply device config colors override (stronger than query/model default)
    let default_palette: Vec<(u8, u8, u8)> = if let Some(ref config_colors) = device_config_colors {
        crate::api::display::parse_colors_header(config_colors)
    } else {
        query_palette.clone()
    };

    // Create device context
    let device_ctx = DeviceContext {
        mac: query
            .mac
            .clone()
            .unwrap_or_else(|| "dev-simulator".to_string()),
        battery_voltage: Some(query.battery_voltage.unwrap_or(4.2)),
        rssi: Some(query.rssi.unwrap_or(-50)),
        model: Some(query.model.clone()),
        firmware_version: Some("dev".to_string()),
        width: Some(width),
        height: Some(height),
        registration_code: None,
        colors: Some(crate::api::display::colors_to_hex_strings(&default_palette)),
        ..Default::default()
    };

    // Run script directly with the screen config
    let pipeline = state.content_pipeline.clone();
    let script_path = screen_config.script.clone();
    let template_path = screen_config.template.clone();
    let default_refresh = screen_config.default_refresh;
    let ctx = device_ctx.clone();
    let timestamp_override = query.timestamp;

    let result = tokio::task::spawn_blocking(move || {
        // Run the Lua script with optional timestamp override
        let script_result = pipeline.run_script_direct(
            &script_path,
            &template_path,
            default_refresh,
            custom_params,
            Some(ctx.clone()),
            timestamp_override,
        )?;

        // Render SVG
        let svg = pipeline
            .render_svg_from_script(&script_result, Some(&ctx))
            .map_err(|e| e.to_string())?;

        Ok::<(String, Option<Vec<String>>), String>((svg, script_result.script_colors))
    })
    .await;

    let (svg, script_colors) = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RenderErrorResponse {
                    error: "Render error".to_string(),
                    details: Some(e),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RenderErrorResponse {
                    error: "Task error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };

    // Apply script colors override (strongest in priority chain)
    let final_palette = if let Some(ref sc) = script_colors {
        crate::api::display::parse_colors_header(&sc.join(","))
    } else {
        default_palette
    };

    // Convert SVG to PNG with palette-aware dithering
    let display_spec = DisplaySpec::from_dimensions(width, height).unwrap_or(DisplaySpec::OG);

    match state
        .content_pipeline
        .render_png_from_svg(&svg, display_spec, &final_palette, None)
    {
        Ok(png_bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            Bytes::from(png_bytes),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RenderErrorResponse {
                error: "PNG render error".to_string(),
                details: Some(e.to_string()),
            }),
        )
            .into_response(),
    }
}
