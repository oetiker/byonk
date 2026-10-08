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
    header_value, resolve_pre_script, resolve_render, PreScriptInputs, RenderInputs, Resolution,
    Rung, RungSettings, RungStatus, RESOLUTION_HEADER,
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
/// preview can show the effect of removing a value (controller ruling R-B).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderDraft {
    pub base: Option<CascadeValues>,
    /// The device's `final:` rung, drafted.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
    pub panel: Option<PanelDraft>,
    /// Replaces the device's configured params; `simulated.params` still
    /// merge on top (plan ruling P1-7).
    pub params: Option<HashMap<String, serde_json::Value>>,
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

/// Override fields the cascade cannot carry on this path (controller ruling
/// R-B): the script would be shown one palette and rendered with another,
/// and nothing here consumes a refresh.
fn refused_fields(req: &RenderRequest) -> Option<&'static str> {
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

/// A configured device: everything the dev UI's device editor shows.
#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub screen: String,
    pub panel: Option<String>,
    pub name: Option<String>,
    pub params: HashMap<String, serde_json::Value>,
    /// The base rung (what is written directly under the device).
    pub base: CascadeValues,
    /// The `final:` rung, the owner's veto.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
    pub temperature_profile: Option<String>,
    pub maximum_compatibility: Option<bool>,
    pub min_png_bytes: Option<u32>,
}

/// A panel profile: everything the dev UI's panel editor shows.
#[derive(Debug, Serialize)]
pub struct PanelInfo {
    pub id: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub colors: String,
    pub colors_actual: Option<String>,
    /// The panel's dither block, in the flat form config.yaml uses.
    pub dither: Option<PanelDitherConfig>,
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
    let mut devices: Vec<DeviceInfo> = config
        .devices
        .iter()
        .map(|(id, dev)| DeviceInfo {
            id: id.clone(),
            screen: dev.screen.clone(),
            panel: dev.panel.clone(),
            name: dev.name.clone(),
            params: yaml_params_to_json(&dev.params),
            base: dev.base(),
            final_rung: dev.final_rung.clone(),
            temperature_profile: dev.temperature_profile.clone(),
            maximum_compatibility: dev.maximum_compatibility,
            min_png_bytes: dev.min_png_bytes,
        })
        .collect();
    devices.sort_by(|a, b| a.id.cmp(&b.id));

    // Collect panel profiles
    let mut panels: Vec<PanelInfo> = config
        .panels
        .iter()
        .map(|(id, panel)| PanelInfo {
            id: id.clone(),
            name: panel.name.clone(),
            width: panel.width,
            height: panel.height,
            colors: panel.colors.clone(),
            colors_actual: panel.colors_actual.clone(),
            dither: panel.dither.clone(),
        })
        .collect();
    panels.sort_by(|a, b| a.id.cmp(&b.id));

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

/// Body of `POST /dev/api/panels/{panel}/save`: the panel rung, whole. An
/// absent field is removed from the panel, because the draft replaced the
/// rung (controller ruling R-B).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelRungWrite {
    pub colors_actual: Option<String>,
    pub dither: Option<PanelDitherConfig>,
}

/// Write a panel's whole rung into `config.yaml`, durably (controller ruling
/// R-D), in place, key by key: lines it does not change are not rewritten,
/// so comments and the order of `panels:` survive (ruling C2). Then drop the
/// panel's live override (P1-9): the config now carries the value, and an
/// override that merely restates it is invisible weight.
///
/// Deliberately NOT guarded by `require_writable_global`: `panels` is absent
/// from the Home Assistant add-on Options schema, so nothing else owns it.
pub async fn handle_save_panel(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<PanelRungWrite>,
) -> Result<StatusCode, ApiError> {
    use crate::services::config_writer::{remove_path, set_path, sync_mapping};
    require_known_panel(&state, &panel)?;
    let path = crate::api::config_persist::require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let patch = |e: crate::services::config_writer::ConfigWriteError| {
        ApiError::Internal(format!("patch config: {e}"))
    };
    let mut yaml = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;

    let actual_path = ["panels", panel.as_str(), "colors_actual"];
    yaml = match &body.colors_actual {
        Some(c) => set_path(&yaml, &actual_path, &serde_yaml::Value::from(c.as_str())),
        None => remove_path(&yaml, &actual_path),
    }
    .map_err(patch)?;

    let dither_path = ["panels", panel.as_str(), "dither"];
    yaml = match body.dither.as_ref().filter(|d| !d.is_empty()) {
        Some(d) => {
            let serde_yaml::Value::Mapping(target) = serde_yaml::to_value(d)
                .map_err(|e| ApiError::Internal(format!("serialize dither: {e}")))?
            else {
                return Err(ApiError::Internal(
                    "dither did not serialize to a mapping".into(),
                ));
            };
            sync_mapping(&yaml, &dither_path, &target)
        }
        None => remove_path(&yaml, &dither_path),
    }
    .map_err(patch)?;

    crate::api::config_persist::persist(&state, &path, yaml)?;
    state.dev_overrides.panels.remove(&panel).await;
    Ok(StatusCode::OK)
}

/// The first field no dev override may carry (ruling C1): `colors` would
/// show the script one palette and render another, and nothing on the
/// override rung consumes `refresh`. Everything else -- `dither` included --
/// is a legitimate draft value for both the panel and the device rung.
/// Returns the complete error message, so both PUT routes say the same thing
/// (ruling PF8).
fn refused_override_field(v: &CascadeValues) -> Option<String> {
    let field = if v.colors.is_some() {
        "colors"
    } else if v.refresh.is_some() {
        "refresh"
    } else {
        return None;
    };
    Some(format!(
        "`{field}` cannot be overridden: an override carries colors_actual, dither, \
         max_error, noise_scale, chroma_clamp, strength and gamut"
    ))
}

/// `PUT /dev/api/overrides/panels/{panel}`: push a panel draft to every
/// device on that panel, as the cascade's override rung. In memory, expiring
/// 30 minutes after the last PUT. Does NOT touch a tuning session: the client
/// re-PUTs the watched device's session alongside each push.
pub async fn handle_put_panel_override(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<CascadeValues>,
) -> Result<StatusCode, ApiError> {
    require_known_panel(&state, &panel)?;
    if let Some(message) = refused_override_field(&body) {
        return Err(ApiError::BadRequest(message));
    }
    state.dev_overrides.panels.insert(panel, body).await;
    Ok(StatusCode::OK)
}

/// `DELETE /dev/api/overrides/panels/{panel}`. Idempotent.
pub async fn handle_delete_panel_override(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
) -> StatusCode {
    state.dev_overrides.panels.remove(&panel).await;
    StatusCode::OK
}

/// `PUT /dev/api/overrides/devices/{device}`: push a device draft as the
/// override rung for that one device. Stored under the config key, so a MAC
/// in another case or a registration code addresses the same entry.
pub async fn handle_put_device_override(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
    Json(body): Json<CascadeValues>,
) -> Result<StatusCode, ApiError> {
    let key = state
        .config
        .load()
        .resolve_device_key(&device, Some(&device))
        .ok_or(ApiError::NotFound)?;
    if let Some(message) = refused_override_field(&body) {
        return Err(ApiError::BadRequest(message));
    }
    state.dev_overrides.devices.insert(key, body).await;
    Ok(StatusCode::OK)
}

/// `DELETE /dev/api/overrides/devices/{device}`. Idempotent.
pub async fn handle_delete_device_override(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    let key = state
        .config
        .load()
        .resolve_device_key(&device, Some(&device))
        .unwrap_or(device);
    state.dev_overrides.devices.remove(&key).await;
    StatusCode::OK
}

/// `PATCH /dev/api/devices/{key}`: Save a device draft. The same write as the
/// admin PATCH and the MCP tool -- `apply_device_patch`, with its screen,
/// params, clear and read-only-config guards -- then drop the device's live
/// override, which the config now carries (P1-9). The override is keyed by the
/// resolved config key, as the override routes are.
pub async fn handle_save_device(
    State(state): State<AppState>,
    axum::extract::Path(key): axum::extract::Path<String>,
    Json(body): Json<crate::api::admin::write::DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let key = state
        .config
        .load()
        .resolve_device_key(&key, Some(&key))
        .unwrap_or(key);
    let out = crate::api::admin::write::apply_device_patch(&state, &key, body).await?;
    state.dev_overrides.devices.remove(&key).await;
    Ok(Json(out))
}

/// One live override in the listing.
#[derive(Debug, Serialize)]
pub struct LiveOverride {
    pub key: String,
    pub values: CascadeValues,
    pub expires_in_secs: u64,
}

/// One live tuning session in the listing.
#[derive(Debug, Serialize)]
pub struct LiveSession {
    pub key: String,
    pub expires_in_secs: u64,
}

/// `GET /dev/api/overrides`.
#[derive(Debug, Serialize)]
pub struct OverridesResponse {
    pub panels: Vec<LiveOverride>,
    pub devices: Vec<LiveOverride>,
    pub tuning_sessions: Vec<LiveSession>,
}

/// List everything the dev UI has made live, so a reloaded page can show a
/// Push that is still reaching hardware (spec section 6).
pub async fn handle_list_overrides(State(state): State<AppState>) -> Json<OverridesResponse> {
    let o = &state.dev_overrides;
    let to_live = |v: Vec<(String, CascadeValues, std::time::Duration)>| {
        v.into_iter()
            .map(|(key, values, left)| LiveOverride {
                key,
                values,
                expires_in_secs: left.as_secs(),
            })
            .collect::<Vec<_>>()
    };
    let panels = to_live(o.panels.live().await);
    let devices = to_live(o.devices.live().await);
    // One TTL rule for every map (ruling PF8): `live_tuning_sessions` shares
    // `ttl_left` with `tuning_session_active` and `TimedMap`.
    let tuning_sessions: Vec<LiveSession> = crate::server::live_tuning_sessions(o)
        .await
        .into_iter()
        .map(|(key, left)| LiveSession {
            key,
            expires_in_secs: left.as_secs(),
        })
        .collect();
    Json(OverridesResponse {
        panels,
        devices,
        tuning_sessions,
    })
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
) -> Result<StatusCode, ApiError> {
    // `/api/display` looks the session up by config key, so store it there.
    let key = state
        .config
        .load()
        .resolve_device_key(&device, Some(&device))
        .ok_or(ApiError::NotFound)?;
    state
        .dev_overrides
        .tuning_sessions
        .write()
        .await
        .insert(key, std::time::Instant::now());
    Ok(StatusCode::OK)
}

/// End a tuning session, handing the device its normal refresh rate back.
/// Idempotent: an unknown device is a no-op.
pub async fn handle_end_tuning_session(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    let key = state
        .config
        .load()
        .resolve_device_key(&device, Some(&device))
        .unwrap_or(device);
    state
        .dev_overrides
        .tuning_sessions
        .write()
        .await
        .remove(&key);
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

    if let Some(refusal) = refused_fields(&req) {
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
    // The config key the device is stored under: override maps are keyed by it.
    let device_key: Option<String> = req
        .device
        .as_deref()
        .and_then(|k| config.resolve_device_key(k, Some(k)));
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
    // The device's `final:` block, unless the draft replaces it.
    let device_final_rung: CascadeValues = req
        .draft
        .final_rung
        .clone()
        .unwrap_or_else(|| device.map(|d| d.device_final()).unwrap_or_default());
    let panel_key: Option<String> = req
        .panel
        .clone()
        .or_else(|| device.and_then(|d| d.panel.clone()))
        .filter(|k| config.get_panel(k).is_some());
    let panel_cfg = panel_key.as_deref().and_then(|name| config.get_panel(name));
    // The override rung: the request's own ad-hoc values over whatever a
    // Push made live (plan ruling P1-4). Reading only: this handler writes
    // no server state.
    let live = state
        .dev_overrides
        .live_override(device_key.as_deref(), panel_key.as_deref())
        .await;
    let over: CascadeValues = req.over.or(&live);
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

    // The device's params (or the draft's, which replace them), with the
    // simulated ones on top.
    let mut custom_params: HashMap<String, serde_json::Value> = match &req.draft.params {
        Some(p) => p.clone(),
        None => device
            .map(|d| yaml_params_to_json(&d.params))
            .unwrap_or_default(),
    };
    custom_params.extend(req.simulated.params.clone());

    // The model's default palette stands in for the firmware `Colors` header.
    let query_palette: Vec<(u8, u8, u8)> = display::resolve_query_palette(&req.model, None);

    // ---- Pre-script device context: what the script sees as `device.*`,
    // from the one shared resolution (`override -> device final -> device
    // base -> panel -> model default`, the screen rung empty), fed from the
    // draft-aware rungs.
    let panel_actual: Option<Vec<(u8, u8, u8)>> = panel_colors_actual
        .as_deref()
        .map(display::parse_colors_header);
    let pre = resolve_pre_script(&PreScriptInputs {
        over: RungSettings {
            dither: over.dither.clone(),
            colors_actual: over
                .colors_actual
                .as_deref()
                .map(display::parse_colors_header),
            ..Default::default()
        },
        device_final: RungSettings::from_cascade(&device_final_rung),
        device_base: RungSettings::from_cascade(&device_rung),
        panel: RungSettings {
            colors: panel_colors.as_deref().map(display::parse_colors_header),
            colors_actual: panel_actual.clone(),
            ..Default::default()
        },
        panel_dither: panel_dither_config.as_ref(),
        fallback_palette: query_palette.clone(),
        fallback_colors_actual: None,
    });
    let pre_script_tuning = &pre.tuning;
    let default_palette = &pre.palette;

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
        colors: Some(display::colors_to_hex_strings(default_palette)),
        colors_actual: pre
            .colors_actual
            .as_deref()
            .map(display::colors_to_hex_strings),
        dither_algorithm: Some(pre.algorithm.clone()),
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

        // The dev preview has the script's return value in hand, so it
        // resolves `refresh` through the ladder as well.
        let script = RungSettings::from_script(&script_result)
            .with_refresh(script_result.script_refresh_rate);
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
    // /dev/api/overrides/panels/{panel}` — so that a push is something the
    // operator does, not something a preview does to them. See
    // `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
    // §5.1/§5.2, which is where the explicit routes for the dither and tuning
    // overrides are specified.

    // ---- Post-script: every render setting resolves on the ladder, and the
    // renderer is fed from that same resolution — the report and the pixels
    // cannot disagree.
    let res = resolve_render(&RenderInputs {
        over: RungSettings::from_cascade(&over),
        device_final: RungSettings::from_cascade(&device_final_rung),
        screen: out.script,
        device_base: RungSettings::from_cascade(&device_rung),
        panel: RungSettings {
            colors: panel_colors.as_deref().map(display::parse_colors_header),
            colors_actual: panel_actual,
            ..Default::default()
        },
        panel_dither: panel_dither_config.as_ref(),
        fallback_palette: query_palette,
        fallback_colors_actual: None,
        screen_default_refresh: out.screen_default_refresh,
        override_source: display::SRC_DEV_OVERRIDE,
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

/// The two warnings the pre-cascade resolver pushed, read from the
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
    use axum::routing::{get, patch, post, put};
    let api = Router::new()
        .route("/screens", get(handle_screens))
        .route("/events", get(handle_events))
        .route("/render", post(handle_render))
        .route("/resolve-mac", get(handle_resolve_mac))
        .route("/overrides", get(handle_list_overrides))
        .route(
            "/overrides/devices/{device}",
            put(handle_put_device_override).delete(handle_delete_device_override),
        )
        .route(
            "/overrides/panels/{panel}",
            put(handle_put_panel_override).delete(handle_delete_panel_override),
        )
        .route("/panels/{panel}/save", post(handle_save_panel))
        .route("/devices/{key}", patch(handle_save_device))
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
