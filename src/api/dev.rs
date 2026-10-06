//! Dev mode API endpoints.
//!
//! These are mounted under `/dev` by `byonk serve`, behind the `require_dev`
//! login gate (see `crate::api::dev_auth`).

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Json, Response,
    },
    Router,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::api::resolution::{
    header_value, resolve_dev_render, DevRenderInputs, Resolution, Rung, RungSettings, RungStatus,
    RESOLUTION_HEADER,
};
use crate::error::ApiError;
use crate::models::{CascadeValues, DisplaySpec, PanelDitherConfig};
use crate::server::AppState;
use crate::services::DeviceContext;

/// Body of `POST /dev/api/render`. See `docs/src/guide/dev-mode.md`
/// ("Rendering a preview") for the contract.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    /// Screen repo ref. With `device`, this screen is rendered with the
    /// device's settings; without it, the device's own screen is rendered.
    pub screen: Option<String>,
    /// MAC, config key, or registration code.
    pub device: Option<String>,
    /// Panel profile id; beats the device's own `panel`.
    pub panel: Option<String>,
    /// Device model: "og" or "x"
    #[serde(default = "default_model")]
    pub model: String,
    /// Display width (overrides model default)
    pub width: Option<u32>,
    /// Display height (overrides model default)
    pub height: Option<u32>,
    #[serde(default)]
    pub draft: RenderDraft,
    /// Ad-hoc knobs from the dev UI: the `override` rung.
    #[serde(default, rename = "override")]
    pub over: CascadeValues,
    #[serde(default)]
    pub simulated: SimulatedInputs,
    #[serde(default)]
    pub view: ViewOptions,
}

/// An uncommitted edit. Each present rung REPLACES the configured one, so a
/// preview can show the effect of removing a value (plan ruling 2).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderDraft {
    pub base: Option<CascadeValues>,
    pub panel: Option<PanelDraft>,
}

/// The panel rung of a draft. Every field absent means the panel carries
/// nothing — it does not fall back to the configured panel.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDraft {
    pub colors: Option<String>,
    pub colors_actual: Option<String>,
    pub dither: Option<PanelDitherConfig>,
}

/// What the device would report about itself.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatedInputs {
    /// Battery voltage (default: 4.2)
    pub battery_voltage: Option<f32>,
    /// WiFi RSSI in dBm (default: -50)
    pub rssi: Option<i32>,
    /// Unix timestamp override for time_now() in Lua
    pub timestamp: Option<i64>,
    /// Merged over the device's configured params.
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

/// How the answer is drawn, as opposed to how it is resolved.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewOptions {
    /// Show actual measured panel colors in preview (default: true when
    /// measured colors resolve)
    pub use_actual: Option<bool>,
}

/// Fields the cascade cannot carry until piece 3 (plan ruling 4).
fn refused_until_cascade(req: &RenderRequest) -> Option<&'static str> {
    if req
        .draft
        .base
        .as_ref()
        .is_some_and(|b| b.colors_actual.is_some())
    {
        return Some("draft.base.colors_actual: a device-level colors_actual is not supported yet");
    }
    if req.over.colors.is_some() {
        return Some("override.colors is not supported: the script could not see it");
    }
    if req.over.refresh.is_some() {
        return Some("override.refresh is not supported");
    }
    None
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
    pub panel: Option<String>,
    pub dither: Option<String>,
}

/// Panel info for the panels list
#[derive(Debug, Serialize)]
pub struct PanelInfo {
    pub id: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub colors: String,
    pub colors_actual: Option<String>,
}

/// The tuning values an algorithm renders with when nothing overrides them.
///
/// Taken straight from `DitherAlgorithm::defaults()`, so the dev UI's number
/// fields start on the values the engine would have used anyway and a render
/// only carries an override the user actually made.
#[derive(Debug, Serialize)]
pub struct DitherDefaults {
    pub max_error: f32,
    pub noise_scale: f32,
}

/// One entry of the dither algorithm list the dev UI builds its select from.
#[derive(Debug, Serialize)]
pub struct DitherAlgorithmInfo {
    /// Canonical name, as accepted by `?dither=` and by config.yaml.
    pub name: String,
    pub defaults: DitherDefaults,
}

/// Response from /dev/screens
#[derive(Debug, Serialize)]
pub struct ScreensResponse {
    pub screens: Vec<ScreenInfo>,
    pub devices: Vec<DeviceInfo>,
    pub panels: Vec<PanelInfo>,
    /// Every algorithm byonk understands, with its tuning defaults. The dev
    /// UI used to hard-code both; a copy that drifted made the preview lie
    /// about what the device would show.
    pub dither_algorithms: Vec<DitherAlgorithmInfo>,
}

/// Response from /dev/render when there's an error
#[derive(Debug, Serialize)]
pub struct RenderErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

/// Serve the dev UI's HTML shell, built by `make webui`.
pub async fn handle_dev_page() -> Html<&'static str> {
    Html(include_str!("../../webui/dist/index.html"))
}

/// Serve the dev UI application bundle.
pub async fn handle_app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("../../webui/dist/app.js"),
    )
}

/// Serve the dev UI stylesheet.
pub async fn handle_app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../webui/dist/app.css"),
    )
}

/// Serve the 1:1 pixel lens, shared by the editor page and the pop-out
/// preview window.
pub async fn handle_lens_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("../../static/dev/lens.js"),
    )
}

/// Serve the pop-out preview window: a second window that mirrors the render
/// the editor page is showing, over a BroadcastChannel.
pub async fn handle_preview_page() -> Html<&'static str> {
    Html(include_str!("../../static/dev/preview.html"))
}

/// List available screens by discovering them from the filesystem.
///
/// Screens are discovered by scanning for `.lua` files with matching `.svg` templates.
/// Config-defined screens are included first, then any filesystem screens not in config
/// are auto-discovered. This allows new screens to appear without restarting the server.
pub async fn handle_screens(State(state): State<AppState>) -> Json<ScreensResponse> {
    let config = state.config.load();

    // Every screen comes from the screen repo loader (the source of truth). The
    // `name` is the qualified `handle/path` ref the dev UI feeds back into
    // `?screen=<ref>`; script/template are informational only.
    let mut screens: Vec<ScreenInfo> = state
        .screen_repo_manager
        .loader()
        .list_all()
        .into_iter()
        .map(|screen| {
            let r#ref = format!("{}/{}", screen.handle, screen.path);
            ScreenInfo {
                name: r#ref,
                script: format!("{}/script.lua", screen.path),
                template: format!("{}/screen.svg", screen.path),
                default_refresh: screen.meta.refresh.unwrap_or(900),
            }
        })
        .collect();
    screens.sort_by(|a, b| a.name.cmp(&b.name));

    // Collect configured devices
    let devices: Vec<DeviceInfo> = config
        .devices
        .iter()
        .map(|(id, dev)| DeviceInfo {
            id: id.clone(),
            screen: dev.screen.clone(),
            panel: dev.panel.clone(),
            dither: dev.dither.clone(),
        })
        .collect();

    // Collect panel profiles
    let panels: Vec<PanelInfo> = config
        .panels
        .iter()
        .map(|(id, panel)| PanelInfo {
            id: id.clone(),
            name: panel.name.clone(),
            width: panel.width,
            height: panel.height,
            colors: panel.colors.clone(),
            colors_actual: panel.colors_actual.clone(),
        })
        .collect();

    // The engine owns both the list and the numbers; this endpoint only
    // relays them.
    let dither_algorithms: Vec<DitherAlgorithmInfo> = crate::models::config::DITHER_ALGORITHMS
        .iter()
        .map(|name| {
            let (max_error, noise_scale) =
                crate::models::config::dither_algorithm_for_name(name).defaults();
            DitherAlgorithmInfo {
                name: (*name).to_string(),
                defaults: DitherDefaults {
                    max_error,
                    noise_scale,
                },
            }
        })
        .collect();

    Json(ScreensResponse {
        screens,
        devices,
        panels,
        dither_algorithms,
    })
}

/// SSE endpoint for file change events
pub async fn handle_events(
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    Query(query): Query<ResolveMacQuery>,
) -> Response {
    let config = state.config.load();

    // Try MAC lookup first, then registration code lookup
    let device_config = config
        .get_device_config(&query.mac)
        .or_else(|| config.get_device_config_for_code(&query.mac));

    match device_config {
        Some(device_config) => {
            // Convert YAML params to JSON for response
            let params: HashMap<String, serde_json::Value> =
                yaml_params_to_json(&device_config.params);

            Json(ResolveMacResponse {
                screen: device_config.screen.clone(),
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

/// Request body for `PUT /dev/api/panel-colors/{panel}`.
#[derive(Debug, Deserialize)]
pub struct PanelColorsRequest {
    pub colors_actual: String,
}

/// Set a temporary colour override for a panel profile (dev colour tuning).
///
/// Keyed by panel, not device: a calibration belongs to the panel profile, so
/// every device using it picks the override up on its next poll. In memory
/// only — `POST .../save` (below) is what makes a calibration durable.
///
/// Note: this does NOT refresh a tuning session. The override is keyed by
/// panel and a session by device, and one panel may have several. The client
/// re-PUTs its own session alongside each push (spec §3).
pub async fn handle_set_panel_colors(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<PanelColorsRequest>,
) -> Result<StatusCode, ApiError> {
    require_known_panel(&state, &panel)?;
    state
        .dev_overrides
        .panel_colors
        .write()
        .await
        .insert(panel, body.colors_actual);
    Ok(StatusCode::OK)
}

/// Write a panel's tuned measured colours into `config.yaml`, durably.
///
/// This is stage 2 of colour tuning: stage 1 is the in-memory override above,
/// which lets a calibration reach real hardware without becoming the
/// configuration. Saving makes it the configuration and drops the override,
/// because an override that merely restates the config is invisible weight.
///
/// Deliberately NOT guarded by `require_writable_global`: `panels` is absent
/// from the Home Assistant add-on Options schema, so nothing else owns it, and
/// the sidebar dev UI is the main way this is used. See spec §5.
pub async fn handle_save_panel_colors(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<PanelColorsRequest>,
) -> Result<StatusCode, ApiError> {
    require_known_panel(&state, &panel)?;
    let path = crate::api::config_persist::require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let yaml = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;
    let new_yaml = crate::services::config_writer::set_scalar(
        &yaml,
        &["panels", &panel, "colors_actual"],
        serde_yaml::Value::from(body.colors_actual.as_str()),
    )
    .map_err(|e| ApiError::Internal(format!("patch config: {e}")))?;

    crate::api::config_persist::persist(&state, &path, new_yaml)?;

    // The config now carries the value, so the override would only restate it.
    state
        .dev_overrides
        .panel_colors
        .write()
        .await
        .remove(&panel);
    Ok(StatusCode::OK)
}

/// Remove a panel's temporary colour override, reverting to `config.yaml`.
pub async fn handle_delete_panel_colors(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
) -> StatusCode {
    state
        .dev_overrides
        .panel_colors
        .write()
        .await
        .remove(&panel);
    StatusCode::OK
}

/// Guard: an override for a panel that does not exist could never be read
/// back, because the lookup is by resolved panel key. Refuse it rather than
/// storing it where nothing will find it.
fn require_known_panel(state: &AppState, panel: &str) -> Result<(), ApiError> {
    if state.config.load().get_panel(panel).is_some() {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

/// Start or refresh a tuning session for a device.
///
/// Idempotent: a repeat PUT is how the dev UI keeps the session alive while a
/// calibration is in progress.
pub async fn handle_start_tuning_session(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    state
        .dev_overrides
        .tuning_sessions
        .write()
        .await
        .insert(device, std::time::Instant::now());
    StatusCode::OK
}

/// End a tuning session, handing the device its normal refresh rate back.
pub async fn handle_end_tuning_session(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    state
        .dev_overrides
        .tuning_sessions
        .write()
        .await
        .remove(&device);
    StatusCode::OK
}

fn render_error(status: StatusCode, error: impl Into<String>, details: Option<String>) -> Response {
    (
        status,
        Json(RenderErrorResponse {
            error: error.into(),
            details,
        }),
    )
        .into_response()
}

/// What the dev render needs back from the blocking script run.
struct ScriptOutputs {
    svg: String,
    script: RungSettings,
    screen_default_refresh: u32,
    font_hinting: Option<crate::rendering::font_config::FontHintingDirective>,
}

/// Render a screen and return PNG, with the resolution of every render
/// setting in `X-Byonk-Resolution`.
pub async fn handle_render(
    State(state): State<AppState>,
    Json(req): Json<RenderRequest>,
) -> Response {
    use crate::api::display;

    if let Some(refusal) = refused_until_cascade(&req) {
        return render_error(StatusCode::BAD_REQUEST, refusal, None);
    }

    let config = state.config.load();

    // The device supplies every render setting and its params; `screen`, when
    // given, replaces only which screen is rendered (spec §4.6).
    let device = match req.device.as_deref() {
        Some(key) => match config
            .get_device_config(key)
            .or_else(|| config.get_device_config_for_code(key))
        {
            Some(dc) => Some(dc),
            None => {
                return render_error(
                    StatusCode::NOT_FOUND,
                    format!("Device '{key}' not configured"),
                    None,
                );
            }
        },
        None => None,
    };
    // A screen ref is a `handle/path` screen repo ref; validity is checked
    // when the pipeline resolves it below.
    let Some(screen_ref) = req
        .screen
        .clone()
        .or_else(|| device.map(|d| d.screen.clone()))
    else {
        return render_error(
            StatusCode::BAD_REQUEST,
            "Either 'screen' or 'device' is required",
            None,
        );
    };

    // The rungs, draft-aware: a draft rung REPLACES the configured one
    // (plan ruling 2).
    let device_rung: CascadeValues = req
        .draft
        .base
        .clone()
        .unwrap_or_else(|| device.map(|d| d.base()).unwrap_or_default());
    let panel_cfg = req
        .panel
        .as_deref()
        .or(device.and_then(|d| d.panel.as_deref()))
        .and_then(|name| config.get_panel(name));
    let (panel_colors, panel_colors_actual, panel_dither_config): (
        Option<String>,
        Option<String>,
        Option<PanelDitherConfig>,
    ) = match &req.draft.panel {
        Some(d) => (d.colors.clone(), d.colors_actual.clone(), d.dither.clone()),
        None => (
            panel_cfg.map(|p| p.colors.clone()),
            panel_cfg.and_then(|p| p.colors_actual.clone()),
            panel_cfg.and_then(|p| p.dither.clone()),
        ),
    };

    // Determine dimensions from model or explicit values
    let (width, height) = display::resolve_preview_dimensions(&req.model, req.width, req.height);

    // Device params, with the simulated ones on top.
    let mut custom_params: HashMap<String, serde_json::Value> = device
        .map(|d| yaml_params_to_json(&d.params))
        .unwrap_or_default();
    custom_params.extend(req.simulated.params.clone());

    // The model's default palette stands in for the firmware `Colors` header.
    let query_palette: Vec<(u8, u8, u8)> = display::resolve_query_palette(&req.model, None);

    // ---- Pre-script device context: what the script sees as `device.*`.
    // Unchanged logic (plan ruling 6), fed from the draft-aware rungs.

    // Chain: device colors > panel colors > model default
    let default_palette: Vec<(u8, u8, u8)> = display::resolve_ctx_palette(
        device_rung.colors.as_deref(),
        panel_colors.as_deref(),
        &query_palette,
    );

    // Pre-script dither algorithm: override > device > "atkinson"
    let pre_script_algo = req
        .over
        .dither
        .as_deref()
        .or(device_rung.dither.as_deref())
        .unwrap_or("atkinson");

    // Pre-script panel tuning for device context
    let pre_panel_tuning = panel_dither_config
        .as_ref()
        .map(|pdc| pdc.resolve_for_algorithm(Some(pre_script_algo)))
        .unwrap_or_default();
    let pre_script_tuning = RungSettings::from_cascade(&device_rung)
        .tuning
        .or(&pre_panel_tuning);

    // Measured colours: override (dev UI colour tuning) > panel.colors_actual
    let over_actual: Option<Vec<(u8, u8, u8)>> = req
        .over
        .colors_actual
        .as_deref()
        .map(display::parse_colors_header);
    let panel_actual: Option<Vec<(u8, u8, u8)>> = panel_colors_actual
        .as_deref()
        .map(display::parse_colors_header);
    let pre_script_measured = over_actual.clone().or_else(|| panel_actual.clone());

    let device_ctx = DeviceContext {
        mac: req
            .device
            .clone()
            .unwrap_or_else(|| "dev-simulator".to_string()),
        battery_voltage: Some(req.simulated.battery_voltage.unwrap_or(4.2)),
        rssi: Some(req.simulated.rssi.unwrap_or(-50)),
        model: Some(req.model.clone()),
        firmware_version: Some("dev".to_string()),
        width: Some(width),
        height: Some(height),
        registration_code: None,
        colors: Some(display::colors_to_hex_strings(&default_palette)),
        colors_actual: pre_script_measured
            .as_deref()
            .map(display::colors_to_hex_strings),
        dither_algorithm: Some(pre_script_algo.to_string()),
        dither_max_error: pre_script_tuning.max_error,
        dither_noise_scale: pre_script_tuning.noise_scale,
        dither_chroma_clamp: pre_script_tuning.chroma_clamp,
        dither_strength: pre_script_tuning.strength,
        dither_gamut_knee: pre_script_tuning.gamut.knee,
        dither_gamut_amount: pre_script_tuning.gamut.amount,
        dither_gamut_max_compression: pre_script_tuning.gamut.max_compression,
        ..Default::default()
    };

    // ---- Run the script with the resolved screen ref
    let pipeline = state.content_pipeline.clone();
    let ctx = device_ctx.clone();
    let timestamp_override = req.simulated.timestamp;

    let result = tokio::task::spawn_blocking(move || {
        let script_result = pipeline.run_script_direct(
            &screen_ref,
            crate::services::content_pipeline::json_params_to_yaml(custom_params),
            Some(ctx.clone()),
            timestamp_override,
            None,
        )?;

        let svg = pipeline
            .render_svg_from_script(&script_result, Some(&ctx))
            .map_err(|e| e.to_string())?;

        let script = RungSettings {
            colors: script_result
                .script_colors
                .as_ref()
                .map(|v| display::parse_colors_header(&v.join(","))),
            colors_actual: script_result
                .script_colors_actual
                .as_deref()
                .map(display::parse_measured_color_list),
            dither: script_result.script_dither.clone(),
            tuning: crate::models::DitherTuningValues {
                deprecated_error_clamp: None,
                max_error: script_result.script_max_error,
                noise_scale: script_result.script_noise_scale,
                chroma_clamp: script_result.script_chroma_clamp,
                strength: script_result.script_strength,
                gamut: script_result.script_gamut.clone().unwrap_or_default(),
            },
            refresh: script_result.script_refresh_rate,
        };
        Ok::<ScriptOutputs, String>(ScriptOutputs {
            svg,
            script,
            screen_default_refresh: script_result.screen_default_refresh,
            font_hinting: script_result.font_hinting,
        })
    })
    .await;

    let out = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            return render_error(StatusCode::INTERNAL_SERVER_ERROR, "Render error", Some(e));
        }
        Err(e) => {
            return render_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Task error",
                Some(e.to_string()),
            );
        }
    };

    // This handler writes NO server state. It renders a preview and answers
    // with it; nothing it does can reach a physical device.
    //
    // It used to sync `query.dither`/`query.colors_actual`/the tuning numbers
    // into `DevOverrides`, keyed by `query.mac`, so that `/api/display` would
    // serve them to the real device. Two things were wrong with that, and
    // `tests/dev_render_is_read_only_test.rs` pins both:
    //
    //  - Merely *looking* at a preview re-tuned the selected display, with no
    //    button and no indication — and a preview that omitted `dither` or the
    //    tuning numbers took the `else` branch and *removed* an override
    //    someone had set on purpose.
    //  - The colour sync inserted into `dev_overrides.panel_colors` under the
    //    DEVICE key, while `api::display` reads that map by PANEL key (see
    //    `resolve_panel`'s doc comment). Inert while the two key spaces don't
    //    collide; a silent cross-device repaint when they do.
    //
    // Reaching real hardware is a separate, explicit act — `PUT
    // /dev/api/panel-colors/{panel}` — so that a push is something the
    // operator does, not something a preview does to them. See
    // `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
    // §5.1/§5.2, which is where the explicit routes for the dither and tuning
    // overrides are specified.

    // ---- Post-script: every render setting resolves on the ladder, and the
    // renderer is fed from that same resolution — the report and the pixels
    // cannot disagree.
    let res = resolve_dev_render(&DevRenderInputs {
        over: RungSettings::from_cascade(&req.over),
        screen: out.script,
        device_base: RungSettings::from_cascade(&device_rung),
        panel: RungSettings {
            colors: panel_colors.as_deref().map(display::parse_colors_header),
            colors_actual: panel_actual,
            ..Default::default()
        },
        panel_dither: panel_dither_config.as_ref(),
        fallback_palette: query_palette,
        screen_default_refresh: out.screen_default_refresh,
    });
    log_render_warnings(&res.report, &req);

    let (tuning, has_tuning) = display::resolve_dither_tuning_from(&res.tuning);

    tracing::debug!(
        resolved_palette = ?display::colors_to_hex_strings(&res.palette),
        resolved_dither = ?res.dither,
        has_measured = res.measured_colors.is_some(),
        "Resolved dev render params"
    );

    // Convert SVG to PNG with palette-aware dithering
    let display_spec = DisplaySpec::from_dimensions(width, height).unwrap_or(DisplaySpec::OG);

    // Dev mode uses measured colors for preview.
    // Respect an explicit request; default to true when measured colors
    // resolve. Shared rule — see `resolve_use_actual`'s doc comment.
    let use_actual =
        display::resolve_use_actual(req.view.use_actual, res.measured_colors.is_some());

    match state.content_pipeline.render_png_from_svg(
        &out.svg,
        display_spec,
        &res.palette,
        res.measured_colors.as_deref(),
        use_actual,
        res.dither.as_deref(),
        if has_tuning { Some(&tuning) } else { None },
        out.font_hinting.as_ref(),
        // `/dev/render` does not carry the script log (see
        // `run_script_direct`), so there is nowhere to put this.
        &mut None,
    ) {
        Ok(png_bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png".to_string()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
                (
                    header::HeaderName::from_static(RESOLUTION_HEADER),
                    header_value(&res.report),
                ),
            ],
            Bytes::from(png_bytes),
        )
            .into_response(),
        Err(e) => render_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PNG render error",
            Some(e.to_string()),
        ),
    }
}

/// The two warnings the legacy `resolve_render_params` pushed, read from the
/// resolution so the server log keeps saying what it said: a rejected
/// `colors_actual`, and a screen `dither` the device outranks.
fn log_render_warnings(report: &Resolution, req: &RenderRequest) {
    if let Some(actual) = report.get("colors_actual") {
        for (rung, r) in &actual.rungs {
            if let RungStatus::Rejected { reason } = &r.status {
                tracing::warn!(
                    screen = ?req.screen,
                    device = ?req.device,
                    "{rung:?}: colors_actual {reason}; skipping it."
                );
            }
        }
    }
    if let Some(dither) = report.get("dither") {
        let screen_value = dither
            .rungs
            .get(&Rung::Screen)
            .and_then(|r| r.value.as_ref());
        if let (Rung::DeviceBase, Some(screen_value)) = (dither.from, screen_value) {
            if *screen_value != dither.effective {
                tracing::warn!(
                    screen = ?req.screen,
                    device = ?req.device,
                    "screen asked for dither {screen_value} but the device is configured \
                     for {}; the device wins.",
                    dither.effective
                );
            }
        }
    }
}

/// Everything under `/dev`. Protected routes get `require_dev` as a route
/// layer; `/login` and `/logout` stay open. The hand-written UI's login page
/// used to need an open `/dev.css` route alongside them (so it could style
/// itself before authenticating) — Task 11 inlined the login page's CSS, so
/// that route is gone outright, not repointed at the app's own stylesheet.
///
/// This registers the nest root (`/`, i.e. bare `/dev`) but — per axum's own
/// `nest` semantics — nothing at `/dev/`; `server::build_router` registers
/// that explicitly, outside this nest, wearing the same `require_dev` layer.
pub fn dev_router(state: &AppState) -> Router<AppState> {
    use axum::routing::{get, post, put};
    let api = Router::new()
        .route("/screens", get(handle_screens))
        .route("/events", get(handle_events))
        .route("/render", post(handle_render))
        .route("/resolve-mac", get(handle_resolve_mac))
        .route(
            "/panel-colors/{panel}",
            put(handle_set_panel_colors).delete(handle_delete_panel_colors),
        )
        .route("/panel-colors/{panel}/save", post(handle_save_panel_colors))
        .route(
            "/tuning-session/{device}",
            put(handle_start_tuning_session).delete(handle_end_tuning_session),
        )
        .route("/whoami", get(crate::api::dev_auth::whoami))
        .merge(crate::api::dev_files::files_router());
    let protected = Router::new()
        .route("/", get(handle_dev_page))
        .route("/app.js", get(handle_app_js))
        .route("/app.css", get(handle_app_css))
        .route("/lens.js", get(handle_lens_js))
        // Flat, not nested: `require_dev` redirects an unauthenticated visitor
        // to the relative string "login", which is only correct one level
        // below `/dev/`.
        .route("/preview.html", get(handle_preview_page))
        .nest("/api", api)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::api::dev_auth::require_dev,
        ));
    Router::new()
        .merge(protected)
        .merge(crate::api::dev_auth::dev_auth_router())
}
