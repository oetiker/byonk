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
use super::resolution::{
    resolve_pre_script, resolve_render, PreScriptInputs, RenderInputs, RungSettings,
};
use crate::error::ApiError;
use crate::models::{
    normalize_algorithm_name, verify_ed25519_signature, ApiKey, AppConfig, CascadeValues, Device,
    DeviceId, DisplaySpec, DitherTuningValues, PanelConfig, PanelDitherConfig,
};
use crate::server::{tuning_session_active, DevOverrides, TUNING_REFRESH_SECS};
use crate::services::content_pipeline::ScriptResult;

/// Filename that makes TRMNL firmware run a full-panel wipe instead of
/// displaying the image. Recognised in `bl.cpp` right after the display
/// response is handled; it runs `display_wipe()` and then re-polls. Not a real
/// file — byonk never serves anything under this name.
const SCREEN_WIPER_FILENAME: &str = "screen_wiper.png";
use crate::services::{
    CachedContent, ContentCache, ContentPipeline, DeviceContext, DeviceRegistry, RenderService,
};

// Maximum allowed display dimensions to prevent DoS
const MAX_DISPLAY_WIDTH: u32 = 2000;
const MAX_DISPLAY_HEIGHT: u32 = 2000;

/// Default 4-grey palette for devices that don't send a Colors header
const DEFAULT_COLORS: &str = "#000000,#555555,#AAAAAA,#FFFFFF";

/// Parse a single hex RGB color string (`"#RRGGBB"` or `"RRGGBB"`) into an
/// RGB tuple. Returns `None` for anything that isn't exactly 6 hex digits
/// after trimming whitespace and a leading `#`.
fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let c = s.trim().trim_start_matches('#');
    if c.len() == 6 {
        let r = u8::from_str_radix(&c[0..2], 16).ok()?;
        let g = u8::from_str_radix(&c[2..4], 16).ok()?;
        let b = u8::from_str_radix(&c[4..6], 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// Parse a comma-separated list of hex RGB color strings into RGB tuples.
/// Entries that aren't 6-digit hex are silently dropped.
pub fn parse_colors_header(s: &str) -> Vec<(u8, u8, u8)> {
    s.split(',').filter_map(parse_hex_color).collect()
}

/// Parse a list of individual hex RGB color strings (as returned by a
/// script's `colors_actual`) into RGB tuples, one entry at a time.
///
/// Unlike [`parse_colors_header`], this must NOT join the entries into a
/// single comma-separated string before parsing: a malformed entry that
/// itself contains a comma (e.g. a script accidentally returning
/// `"#111111,#222222"` as one list element) would, under a join/re-split,
/// silently fracture into extra "valid" colors and inflate the parsed
/// count — masking a genuine length mismatch instead of surfacing it.
/// Entries that aren't 6-digit hex are dropped, matching
/// `parse_colors_header`'s silent-drop behaviour for a single entry.
pub fn parse_measured_color_list(items: &[String]) -> Vec<(u8, u8, u8)> {
    items.iter().filter_map(|s| parse_hex_color(s)).collect()
}

/// Assign the handler's bindings to the six rungs of the cascade (spec §2).
///
/// Pulled out of the handler so the assignment can be tested: a pair of rungs
/// swapped here would still resolve cleanly and quietly render with the wrong
/// calibration or algorithm, and nothing downstream could tell.
#[allow(clippy::too_many_arguments)]
fn render_inputs_for<'a>(
    result: &ScriptResult,
    override_colors_actual: Option<&[(u8, u8, u8)]>,
    dev_dither: Option<&str>,
    dev_tuning: Option<&DitherTuningValues>,
    device_final: &CascadeValues,
    device_base: &CascadeValues,
    panel_colors: Option<&str>,
    panel_colors_actual: Option<&[(u8, u8, u8)]>,
    panel_dither: Option<&'a PanelDitherConfig>,
    fallback_palette: Vec<(u8, u8, u8)>,
    header_colors_actual: Option<&[(u8, u8, u8)]>,
) -> RenderInputs<'a> {
    RenderInputs {
        over: RungSettings {
            colors_actual: override_colors_actual.map(<[_]>::to_vec),
            dither: dev_dither.map(normalize_algorithm_name),
            tuning: dev_tuning.cloned().unwrap_or_default(),
            ..Default::default()
        },
        device_final: RungSettings::from_cascade(device_final),
        // `refresh` stays `None`: it is resolved in content_pipeline, which is
        // the only place the script's return value is in hand (plan ruling 3).
        screen: RungSettings::from_script(result),
        device_base: RungSettings::from_cascade(device_base),
        panel: RungSettings {
            colors: panel_colors.map(parse_colors_header),
            colors_actual: panel_colors_actual.map(<[_]>::to_vec),
            ..Default::default()
        },
        panel_dither,
        // The firmware `Colors` / `Measured-Colors` headers are fallbacks, not
        // rungs: a hardware self-report is evidence, not policy (spec §2).
        fallback_palette,
        fallback_colors_actual: header_colors_actual.map(<[_]>::to_vec),
        screen_default_refresh: 0,
        override_source: SRC_DEV_OVERRIDE,
    }
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
    /// Which layer supplied `measured_colors` — one of the `SRC_*` consts,
    /// or [`SRC_NONE`]. Surfaced via `tracing` fields (server logs) and via
    /// `RenderResult::measured_source` / the MCP `render_screen` tool's
    /// diagnostics (see the `SRC_*` consts' doc comment); it names the
    /// source that actually won the FULL chain (script included, after the
    /// length rule), not just a caller's own pre-script layer.
    pub measured_source: &'static str,
    pub dither: Option<String>,
    pub max_error: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
    pub gamut: crate::models::GamutTuningValues,
    /// Complaints about a setting's validity (e.g. a measurement rejected for
    /// having the wrong number of entries). A disagreement between rungs
    /// is NOT a warning: it is the cascade working as specified (spec §4.3).
    pub warnings: Vec<String>,
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

/// Resolve preview width/height: explicit override, else the model's
/// default (`"x"` -> the e-ink-X's 1872x1404, everything else -> the
/// standard 800x480). Shared by `/dev/render` (`api::dev::handle_render`)
/// and `ScreenStore::render` so the two preview paths can't drift on model
/// dispatch.
pub fn resolve_preview_dimensions(
    model: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> (u32, u32) {
    match model {
        "x" => (width.unwrap_or(1872), height.unwrap_or(1404)),
        _ => (width.unwrap_or(800), height.unwrap_or(480)),
    }
}

/// The "query" palette: an explicit colors-header override, else the
/// model's own default (a 16-grey ramp for `"x"`, else the standard
/// 4-grey `DEFAULT_COLORS`). This is the innermost fallback that
/// `resolve_ctx_palette` and the cascade's `fallback_palette` sit on top of.
/// Shared by `/dev/render` and `ScreenStore::render`.
pub fn resolve_query_palette(model: &str, colors_override: Option<&str>) -> Vec<(u8, u8, u8)> {
    if let Some(colors_str) = colors_override {
        parse_colors_header(colors_str)
    } else if model == "x" {
        (0..16)
            .map(|i| {
                let v = (i * 255 / 15) as u8;
                (v, v, v)
            })
            .collect()
    } else {
        parse_colors_header(DEFAULT_COLORS)
    }
}

/// Build the eink-dither tuning override + whether any override is
/// actually present, from resolved `RenderParams`. Shared by `/dev/render`
/// and `ScreenStore::render` so "did the user override anything" can't
/// drift between the two callers.
pub fn resolve_dither_tuning(
    render_params: &RenderParams,
) -> (crate::rendering::svg_to_png::DitherTuning, bool) {
    resolve_dither_tuning_from(&DitherTuningValues {
        max_error: render_params.max_error,
        noise_scale: render_params.noise_scale,
        chroma_clamp: render_params.chroma_clamp,
        strength: render_params.strength,
        gamut: render_params.gamut.clone(),
        deprecated_error_clamp: None,
    })
}

/// [`resolve_dither_tuning`] for a caller that resolved its tuning without a
/// `RenderParams` — the dev render, which resolves on the cascade ladder
/// (`api::resolution`). One `has_tuning` rule for both.
pub fn resolve_dither_tuning_from(
    values: &DitherTuningValues,
) -> (crate::rendering::svg_to_png::DitherTuning, bool) {
    let tuning = crate::rendering::svg_to_png::DitherTuning {
        serpentine: None,
        max_error: values.max_error,
        chroma_clamp: values.chroma_clamp,
        noise_scale: values.noise_scale,
        strength: values.strength,
        gamut: Some(values.gamut.resolve()),
    };
    let has_tuning = tuning.max_error.is_some()
        || tuning.chroma_clamp.is_some()
        || tuning.noise_scale.is_some()
        || tuning.strength.is_some()
        || !values.gamut.is_empty();
    (tuning, has_tuning)
}

/// Source labels for `RenderParams::measured_source`, which the cascade
/// fills in from the winning rung — see
/// [`crate::api::resolution::Rung::measured_source`].
///
/// Defined once so the four call sites (`api::display`, `api::dev`,
/// `services::screen_store`, `main`) can't drift on spelling. The resolved
/// value is not rendered anywhere in the dev UI — it's surfaced via
/// `tracing` fields (server logs) and via `RenderResult::measured_source` /
/// the MCP `render_screen` tool's diagnostics.
pub const SRC_SCRIPT: &str = "script";
/// A device rung supplied the measurement — base or final. The two are one
/// label because the diagnostic answers "whose decision was this", and both
/// rungs belong to the device owner; the `X-Byonk-Resolution` report is where
/// the exact rung is named.
pub const SRC_DEVICE: &str = "device";
pub const SRC_DEV_OVERRIDE: &str = "dev_override";
/// The authoring path's own dev-override slot: a `colors_actual` passed
/// directly in `RenderOpts` (e.g. by the MCP `render_screen` tool), so an
/// agent can preview a calibration without writing a panel into the config.
pub const SRC_RENDER_OPTS: &str = "render_opts";
pub const SRC_PANEL_ACTUAL: &str = "panel.colors_actual";
pub const SRC_MEASURED_HEADER: &str = "Measured-Colors header";
pub const SRC_NONE: &str = "none";

/// Resolve whether the rendered PNG should be drawn in the panel's measured
/// colours rather than the spec palette. `flag` is the caller's explicit
/// request (the `--use-actual` CLI flag, `/dev/render`'s `use_actual` query
/// parameter, or `RenderOpts::use_actual`); `has_measured` is whether
/// measured colours actually resolved for this render.
///
/// The rule: an explicit request wins, but only when there is something
/// measured to show. So the default (`flag == None`) is on whenever measured
/// colours are available, and `Some(true)` with no calibration is a **no-op
/// rather than an error** — hence the trailing `&& has_measured`.
///
/// Defined once, here beside the `SRC_*` consts and for the same reason: the
/// CLI, `/dev/render` and the authoring path must not drift on this rule.
/// Note this governs only the palette the output PNG is drawn in — measured
/// colours always steer the dithering itself when they resolve.
pub fn resolve_use_actual(flag: Option<bool>, has_measured: bool) -> bool {
    flag.unwrap_or(has_measured) && has_measured
}

/// A single measured-colour candidate: `(source_label, parsed_colors)`.
/// `None` means that source wasn't supplied at all.
///
/// All that is left of the pre-cascade measured chain: the **pre-script**
/// chain in `handle_display`, which populates `DeviceContext.colors_actual`
/// — what the script sees before it runs, when the resolved palette's length
/// is not known yet, so no length rule applies. It is not a cascade
/// resolution; the cascade one is
/// [`crate::api::resolution::resolve_render`]'s `colors_actual` ladder.
pub type MeasuredCandidate = (&'static str, Option<Vec<(u8, u8, u8)>>);

/// Resolve the e-ink waveform temperature profile sent to the device.
///
/// The panel selects its refresh waveform from look-up tables indexed by
/// temperature, because colder particles need longer, stronger drive pulses.
/// A non-`default` profile makes the firmware drive harder and flash more than
/// the measured ambient calls for, which is TRMNL's documented remedy for
/// ghosting: partial refreshes reach the black and white rails but leave
/// mid-greys carrying residue of the previous frame.
///
/// `default` stays the fallback even though it is the value that ghosts, so
/// that upgrading changes nothing for devices that are fine.
///
/// An unrecognised value is refused rather than forwarded. This string is
/// consumed by device firmware over the wire, so a typo that reaches it fails
/// silently on the glass — the one place nobody is watching a log.
///
/// `c` is refused for exactly that reason, even though TRMNL's documentation
/// lists it: firmware 1.8.14 still has `else if (tp == "c") u32TP = 3;`
/// commented out in `parse_response_api_display.cpp`, so a device that receives
/// `c` reads it as `default` — the opposite of what the operator asked for.
fn resolve_temperature_profile(configured: Option<&str>) -> String {
    const VALID: [&str; 3] = ["default", "a", "b"];
    let Some(raw) = configured else {
        return "default".to_string();
    };
    let normalized = raw.trim().to_ascii_lowercase();
    if VALID.contains(&normalized.as_str()) {
        normalized
    } else {
        tracing::warn!(
            configured = raw,
            "unknown temperature_profile, sending 'default' instead (valid: default, a, b)"
        );
        "default".to_string()
    }
}

/// Resolve a device's panel profile, returning the profile, its **config key**
/// and a human-readable source label.
///
/// The key is what dev colour overrides are keyed by (`DevOverrides::panel_colors`),
/// so it must be the `config.panels` map key and not the profile's display name.
pub fn resolve_panel<'a>(
    config: &'a AppConfig,
    device_config_panel: Option<&str>,
    board_header: Option<&str>,
    model_str: &str,
) -> (Option<&'a PanelConfig>, Option<String>, Option<String>) {
    if let Some(panel_name) = device_config_panel {
        return match config.get_panel(panel_name) {
            Some(p) => (
                Some(p),
                Some(panel_name.to_string()),
                Some(format!("device_config:{panel_name}")),
            ),
            None => (None, None, None),
        };
    }
    if let Some((name, p)) = board_header.and_then(|b| config.find_panel_for_board(b)) {
        return (
            Some(p),
            Some(name.to_string()),
            Some(format!("board_header:{name}")),
        );
    }
    // Some devices (e.g. reTerminal E1004) report their panel identity in the
    // `Model` header rather than `Board`; fall back to matching against it.
    if let Some((name, p)) = config.find_panel_for_board(model_str) {
        return (
            Some(p),
            Some(name.to_string()),
            Some(format!("model_header:{name}")),
        );
    }
    (None, None, None)
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
        ("Model" = Option<String>, Header, description = "Device model string reported by the device"),
    ),
    tag = "Display"
)]
pub async fn handle_display<R: DeviceRegistry>(
    State(config): State<Arc<AppConfig>>,
    State(registry): State<Arc<R>>,
    State(content_pipeline): State<Arc<ContentPipeline>>,
    State(content_cache): State<Arc<ContentCache>>,
    State(dev_overrides): State<DevOverrides>,
    State(recovery): State<Arc<crate::services::RecoveryRegistry>>,
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
                board = ?headers.get_str("Board"),
                model = ?headers.get_str("Model"),
                colors = ?headers.get_str("Colors"),
                width = width,
                height = height,
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
                ..Default::default()
            };

            // The reserved DEFAULT device's screen (registration-aware via
            // device_context.registration_code). No DEFAULT screen -> built-in.
            let screen_to_use = config.default_device_screen().filter(|s| !s.is_empty());

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
                                    content_pipeline.render_builtin_fallback(
                                        Some(code),
                                        width,
                                        height,
                                    ),
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
                            content_pipeline.render_builtin_fallback(Some(code), width, height),
                            "_registration".to_string(),
                            300,
                        )
                    }
                }
            } else {
                // No DEFAULT device screen configured, use built-in fallback
                (
                    content_pipeline.render_builtin_fallback(Some(code), width, height),
                    "_registration".to_string(),
                    300,
                )
            };

            let cached = CachedContent::builder(registration_svg, screen_name, width, height)
                .with_colors(Some(palette))
                .build();
            let hash = cached.content_hash.clone();
            content_cache.store(cached);

            // Record the unregistered device in the registry so it surfaces in
            // /api/admin/pending for onboarding. Store the identity key so the
            // pending registration_code matches the code shown on this screen
            // (Device::new() would otherwise assign a random api_key).
            let pending_id = DeviceId::new(device_id_str);
            let mut pending_device = registry.find_by_id(&pending_id).await?.unwrap_or_else(|| {
                Device::new(
                    pending_id.clone(),
                    model_str.to_string(),
                    headers
                        .get_str("FW-Version")
                        .unwrap_or("unknown")
                        .to_string(),
                )
            });
            pending_device.api_key = identity_key.clone();
            pending_device.model = model_str.to_string();
            pending_device.firmware_version = headers
                .get_str("FW-Version")
                .unwrap_or("unknown")
                .to_string();
            if let Some(battery) = headers.get_parsed::<f32>("Battery-Voltage") {
                pending_device.battery_voltage = Some(battery);
            }
            if let Some(rssi) = headers.get_parsed::<i32>("RSSI") {
                pending_device.rssi = Some(rssi);
            }
            pending_device.last_seen = chrono::Utc::now();
            registry.upsert(pending_device).await?;

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
                // Unconditional: this device has no config yet -- it is being
                // shown the registration code. That screen is high-contrast
                // text, the content ghosting hurts least.
                temperature_profile: Some("default".to_string()),
                special_function: None,
                maximum_compatibility: None,
            })
            .into_response());
        }
        // Device is registered - continue with normal flow
    }

    // Get or create device metadata
    let device_id = DeviceId::new(device_id_str);
    let model_str = headers.get_str("Model").unwrap_or("og");
    let model = model_str.to_string();
    let fw_version = headers
        .get_str("FW-Version")
        .unwrap_or("unknown")
        .to_string();

    let mut device = registry
        .find_by_id(&device_id)
        .await?
        .unwrap_or_else(|| Device::new(device_id.clone(), model.clone(), fw_version.clone()));

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

    // Resolve device config — also capture the actual config key (used for dev override lookups).
    // get_key_value returns the key as stored in the HashMap, so it matches the dev UI exactly.
    let (device_config, device_entry_key) = {
        // Try MAC (exact, then uppercase-normalized)
        let by_mac = config.devices.get_key_value(device_id_str).or_else(|| {
            let upper = device_id_str.to_uppercase();
            if upper != device_id_str {
                config.devices.get_key_value(&upper)
            } else {
                None
            }
        });
        if let Some((k, dc)) = by_mac {
            (Some(dc), Some(k.clone()))
        } else {
            // Try registration code (hyphenated + raw normalized)
            let norm = registration_code.to_uppercase().replace('-', "");
            let by_code = if norm.len() == 10 {
                let hyph = format!("{}-{}", &norm[..5], &norm[5..]);
                config
                    .devices
                    .get_key_value(&hyph)
                    .or_else(|| config.devices.get_key_value(&norm))
            } else {
                config.devices.get_key_value(&norm)
            };
            if let Some((k, dc)) = by_code {
                (Some(dc), Some(k.clone()))
            } else {
                (None, None)
            }
        }
    };
    let device_config_panel = device_config.and_then(|dc| dc.panel.clone());
    let device_config_colors = device_config.and_then(|dc| dc.colors.clone());

    let (panel, panel_key, panel_source) = resolve_panel(
        &config,
        device_config_panel.as_deref(),
        board_header.as_deref(),
        model_str,
    );

    // Resolve measured colors: dev override > panel.colors_actual >
    // Measured-Colors header > None. Dev overrides are keyed by PANEL config
    // key, because a calibration belongs to the panel profile: every device
    // using that profile shares it. (2026-09-10 colour-tuning amendment.)
    let measured_colors_header = headers.get_str("Measured-Colors").map(|s| s.to_string());
    let override_colors = if let Some(ref key) = panel_key {
        dev_overrides.panel_colors.read().await.get(key).cloned()
    } else {
        None
    };
    let override_colors_parsed: Option<Vec<(u8, u8, u8)>> =
        override_colors.as_deref().map(parse_colors_header);
    let panel_actual_parsed: Option<Vec<(u8, u8, u8)>> = panel
        .and_then(|p| p.colors_actual.as_deref())
        .map(parse_colors_header);
    let header_parsed: Option<Vec<(u8, u8, u8)>> =
        measured_colors_header.as_deref().map(parse_colors_header);
    // Panel official colors for palette chain
    let panel_colors_for_chain: Option<String> = panel.map(|p| p.colors.clone());

    tracing::debug!(
        device_id = device_id_str,
        registration_code = %registration_code,
        device_entry_key = ?device_entry_key,
        device_config_screen = ?device_config.map(|dc| &dc.screen),
        device_name = ?device_config.and_then(|dc| dc.name.as_deref()),
        panel_source = ?panel_source,
        panel_colors = ?panel_colors_for_chain,
        panel_colors_actual = ?panel.and_then(|p| p.colors_actual.as_deref()),
        board_header = ?board_header,
        measured_colors_header = ?measured_colors_header,
        "Device config and panel resolution"
    );

    // Pre-script resolved palette for device context:
    // device_config_colors > panel_colors > firmware header > default
    // Matches the resolve_palette chain (minus script_colors, unknown until script runs)
    // Dither: dev override (highest priority, set via dev UI) vs device config
    let dev_dither_override = if let Some(ref key) = device_entry_key {
        dev_overrides.dither.read().await.get(key).cloned()
    } else {
        None
    };
    let device_config_dither = device_config.and_then(|dc| dc.dither.clone());

    // Tuning override, handed to the render resolution further down.
    let dev_tuning_override: Option<DitherTuningValues> = if let Some(ref key) = device_entry_key {
        dev_overrides.tuning.read().await.get(key).cloned()
    } else {
        None
    };

    // What the script sees as `device.*`, from the one shared resolution
    // (`override -> device final -> device base -> panel -> fallback`, the
    // screen rung empty). Not a cascade resolution of its own: the palette
    // length is not known yet, so no length rule applies.
    let panel_dither_config = panel.and_then(|p| p.dither.clone());
    let device_base_rung = device_config.map(|dc| dc.base()).unwrap_or_default();
    let device_final_rung = device_config
        .map(|dc| dc.device_final())
        .unwrap_or_default();
    let pre = resolve_pre_script(&PreScriptInputs {
        over: RungSettings {
            dither: dev_dither_override.clone(),
            colors_actual: override_colors_parsed.clone(),
            ..Default::default()
        },
        device_final: RungSettings::from_cascade(&device_final_rung),
        device_base: RungSettings::from_cascade(&device_base_rung),
        panel: RungSettings {
            colors: panel_colors_for_chain.as_deref().map(parse_colors_header),
            colors_actual: panel_actual_parsed.clone(),
            ..Default::default()
        },
        panel_dither: panel_dither_config.as_ref(),
        fallback_palette: initial_palette.clone(),
        fallback_colors_actual: header_parsed.clone(),
    });
    let pre_script_measured_source = pre
        .colors_actual_from
        .measured_source(SRC_DEV_OVERRIDE, pre.colors_actual.is_some());
    let pre_script_tuning = &pre.tuning;
    tracing::debug!(
        pre_script_measured_source = pre_script_measured_source,
        "Pre-script device context resolved"
    );
    let ctx_color_hex = colors_to_hex_strings(&pre.palette);

    // Build device context for script (palette matches dithering chain)
    let device_ctx = DeviceContext {
        mac: device.device_id.to_string(),
        battery_voltage: device.battery_voltage,
        rssi: device.rssi,
        model: Some(device.model.to_string()),
        firmware_version: Some(device.firmware_version.clone()),
        width: Some(width),
        height: Some(height),
        // Cloned: the recovery lookup further down needs the code too.
        registration_code: Some(registration_code.clone()),
        board: board_header.clone(),
        colors: Some(ctx_color_hex),
        colors_actual: pre.colors_actual.as_deref().map(colors_to_hex_strings),
        dither_algorithm: Some(pre.algorithm.clone()),
        dither_max_error: pre_script_tuning.max_error,
        dither_noise_scale: pre_script_tuning.noise_scale,
        dither_chroma_clamp: pre_script_tuning.chroma_clamp,
        dither_strength: pre_script_tuning.strength,
        dither_gamut_knee: pre_script_tuning.gamut.knee,
        dither_gamut_amount: pre_script_tuning.gamut.amount,
        dither_gamut_max_compression: pre_script_tuning.gamut.max_compression,
        refresh_override: None,
        // Filled by `content_pipeline` from the device config, as the base is.
        refresh_final: None,
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
    let dc_min_png_bytes = device_config.and_then(|dc| dc.min_png_bytes);
    let dev_dither = dev_dither_override;
    let dev_tuning = dev_tuning_override;
    let panel_dither_for_closure = panel_dither_config;

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
                        dc_colors = ?dc_colors,
                        dc_dither = ?dc_dither,
                        dev_dither = ?dev_dither,
                        panel_colors = ?panel_colors_for_chain,
                        header_colors = ?header_colors_for_chain,
                        measured_colors = ?pre.colors_actual.as_ref().map(|mc| colors_to_hex_strings(mc)),
                        "Resolving render params for device"
                    );

                    // One ladder, spec §2. `render_inputs_for` is the only
                    // place a binding is assigned to a rung.
                    let params = resolve_render(&render_inputs_for(
                        &result,
                        override_colors_parsed.as_deref(),
                        dev_dither.as_deref(),
                        dev_tuning.as_ref(),
                        &device_final_rung,
                        &device_base_rung,
                        panel_colors_for_chain.as_deref(),
                        panel_actual_parsed.as_deref(),
                        panel_dither_for_closure.as_ref(),
                        fallback.clone(),
                        header_parsed.as_deref(),
                    ))
                    .into_params();
                    for w in &params.warnings {
                        tracing::warn!(device = %mac, "{w}");
                    }
                    let tuning = DitherTuningValues {
                        deprecated_error_clamp: None,
                        max_error: params.max_error,
                        noise_scale: params.noise_scale,
                        chroma_clamp: params.chroma_clamp,
                        strength: params.strength,
                        gamut: params.gamut.clone(),
                    };

                    tracing::debug!(
                        device = %mac,
                        resolved_palette = ?colors_to_hex_strings(&params.palette),
                        resolved_dither = ?params.dither,
                        has_measured = params.measured_colors.is_some(),
                        measured_source = params.measured_source,
                        "Resolved render params"
                    );

                    if result.skip_update {
                        (result.refresh_rate, true, None, None)
                    } else {
                        // Render SVG from script result (template processing happens here)
                        match pipeline.render_svg_from_script(&result, Some(&ctx)) {
                            Ok(svg) => {
                                // Cache the pre-rendered SVG (keyed by content hash)
                                let cached = CachedContent::builder(
                                    svg,
                                    result.screen_name.clone(),
                                    width,
                                    height,
                                )
                                .with_colors(Some(params.palette))
                                .with_colors_actual(params.measured_colors)
                                .with_dither(params.dither)
                                .with_font_hinting(result.font_hinting.clone())
                                .with_min_png_bytes(dc_min_png_bytes)
                                .with_tuning(&tuning)
                                .build();
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
                                let cached = CachedContent::builder(
                                    error_svg,
                                    "_error".to_string(),
                                    width,
                                    height,
                                )
                                .with_colors(Some(fallback_palette))
                                .build();
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
                    let cached = CachedContent::builder(error_svg, "_error".to_string(), width, height)
                        .with_colors(Some(fallback_palette))
                        .build();
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

    // Resolved and logged rather than inlined into the struct below: this value
    // is consumed by device firmware and has no visible effect on the server,
    // so when a panel keeps ghosting there is otherwise no way to tell "byonk
    // never sent a profile" apart from "the firmware ignored the one we sent".
    // That ambiguity cost a full debugging cycle.
    let temperature_profile =
        resolve_temperature_profile(device_config.and_then(|dc| dc.temperature_profile.as_deref()));
    let maximum_compatibility = device_config.and_then(|dc| dc.maximum_compatibility);
    tracing::info!(
        device = %device_id_str,
        temperature_profile = %temperature_profile,
        maximum_compatibility = ?maximum_compatibility,
        "Resolved display refresh settings for device"
    );

    // A panel-recovery session replaces this poll's content with a wipe
    // instruction. `screen_wiper.png` is TRMNL's own trigger: the firmware
    // recognises the filename and runs display_wipe() -- ~200 black/white
    // cycles with panel power held, ~190 s -- instead of showing the image. The image
    // URL still rides along, because the firmware handles the response
    // normally before it inspects the filename.
    // Filed under whichever identifier the admin caller used. The route is
    // `/api/admin/devices/{key}/recover`, the same `{key}` as `PATCH
    // /devices/{key}` — the *config* key, which may be a registration code
    // rather than a MAC. The device polls with its MAC alone, so looking only
    // there would lose a run started under the code, with no error anywhere.
    let recovery_device_id = {
        let mac = crate::models::DeviceId::new(device_id_str);
        // The MAC is canonical: it is what `/api/admin/devices` reports as the
        // key for any device that has checked in, so it leads and stays the
        // identifier a new session is filed under. After it come the config
        // key, then every written form of the device's registration code —
        // a run started before the device first checked in could not have been
        // resolved to a MAC, so it sits under whichever form was typed.
        let mut candidates = vec![mac.clone()];
        if let Some(key) = device_entry_key.as_deref() {
            if key != device_id_str {
                candidates.push(crate::models::DeviceId::new(key));
            }
        }
        for id in crate::services::recovery::spellings_of(&registration_code) {
            if !candidates.contains(&id) {
                candidates.push(id);
            }
        }
        recovery.resolve_key(&candidates).await.unwrap_or(mac)
    };
    let wipe = recovery.on_poll(&recovery_device_id).await;
    if let Some(ref session) = wipe {
        tracing::info!(
            device = %device_id_str,
            done = session.done,
            total = session.total,
            remaining = session.remaining(),
            "Panel recovery: serving a wipe instead of content"
        );
    }

    // A run sets its own poll cadence, overriding the screen's `refresh`.
    //
    // The decisive poll is the firmware's follow-up after a wipe: it is served
    // content, so without this it sleeps for however long the *content* takes
    // to go stale before the next wipe. A calibration screen's `refresh: 3600`
    // turned a 10-wipe run into a 10-hour one.
    //
    // Note this asks the registry again rather than reusing `wipe`. They differ
    // exactly where it matters: the follow-up poll returns `None` while the run
    // is still going. `on_poll` also drops the session as the final wipe goes
    // out, so the follow-up poll after that one correctly gets the screen's own
    // rate back.
    let recovery_active = wipe.is_some() || recovery.get(&recovery_device_id).await.is_some();
    let refresh_rate = if recovery_active {
        // A tuning session must lose to recovery, never win over it: recovery
        // is checked and applied first, and the tuning check below only runs
        // when recovery isn't active.
        crate::services::RECOVERY_REFRESH_RATE_SECS
    } else {
        // A live tuning session shortens this device's cadence so a colour
        // calibration iterates in a minute. This deliberately overrides
        // `resolve_refresh_rate`'s rule that the screen owns its own cadence:
        // during a calibration the thing changing the content is a person
        // with a slider. Narrow (one device), bounded (TUNING_SESSION_TTL)
        // and logged.
        match device_entry_key.as_deref() {
            Some(key) if tuning_session_active(&dev_overrides, key).await => {
                tracing::info!(
                    device = key,
                    refresh_rate = TUNING_REFRESH_SECS,
                    "tuning session active — shortening refresh rate"
                );
                TUNING_REFRESH_SECS
            }
            _ => refresh_rate,
        }
    };

    // Return JSON response
    // Note: firmware expects status=0 for success (not 200!)
    // The filename is a hash of the SVG content, so TRMNL can detect changes
    Ok(Json(DisplayJsonResponse {
        status: 0,
        image_url,
        filename: if wipe.is_some() {
            SCREEN_WIPER_FILENAME.to_string()
        } else {
            content_hash.unwrap_or_else(|| "unchanged".to_string())
        },
        update_firmware: false,
        firmware_url: None,
        refresh_rate,
        reset_firmware: false,
        temperature_profile: Some(temperature_profile),
        special_function: None,
        maximum_compatibility,
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

    tracing::debug!(
        content_hash = %content_hash,
        palette = ?colors_to_hex_strings(palette),
        colors_actual = ?colors_actual.map(colors_to_hex_strings),
        dither = ?dither,
        "Dither parameters for PNG render"
    );

    // Build DitherTuning from cached tuning values (set by script or device config)
    let tuning = crate::rendering::svg_to_png::DitherTuning {
        serpentine: None,
        max_error: cached.max_error,
        chroma_clamp: cached.chroma_clamp,
        noise_scale: cached.noise_scale,
        strength: cached.strength,
        gamut: Some(cached.gamut.resolve()),
    };
    let has_tuning = tuning.max_error.is_some()
        || tuning.chroma_clamp.is_some()
        || tuning.noise_scale.is_some()
        || tuning.strength.is_some()
        || !cached.gamut.is_empty();

    let png_bytes = content_pipeline.render_png_from_svg(
        &cached.rendered_svg,
        spec,
        palette,
        colors_actual,
        false, // production always uses official colors
        dither,
        if has_tuning { Some(&tuning) } else { None },
        cached.font_hinting.as_ref(),
        // The device path has no author to talk to: whoever wrote this screen
        // is not the one holding the panel. The warning belongs to the
        // authoring paths (`ScreenStore::render`, `byonk render`).
        &mut None,
    )?;

    let png_bytes = match cached.min_png_bytes {
        Some(min) => {
            let rendered = png_bytes.len();
            let padded = crate::rendering::png_pad::pad_png_to_min_size(png_bytes, min as usize);
            if padded.len() != rendered {
                // Logged because the whole point is a size the *firmware* reads;
                // if this line is missing, the device never saw the padding.
                tracing::info!(
                    rendered_bytes = rendered,
                    padded_bytes = padded.len(),
                    min_png_bytes = min,
                    "Padded PNG to reach the device's minimum image size"
                );
            }
            padded
        }
        None => png_bytes,
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

    /// Force full-waveform refreshes on the device, disabling fast refresh.
    ///
    /// Omitted from the JSON when unset so the device keeps its own default —
    /// sending `false` would actively assert "use fast refresh", which is not
    /// the same as having no opinion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum_compatibility: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_panel_reports_the_config_key_in_every_branch() {
        // `AppConfig` derives `Deserialize`, so YAML is the shortest honest
        // way to build one — and it exercises the same parse the server uses.
        let config: AppConfig = serde_yaml::from_str(
            "\
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF'
",
        )
        .expect("fixture config parses");

        // (a) the device names the panel
        let (p, key, src) = resolve_panel(&config, Some("e1004"), None, "");
        assert!(p.is_some());
        assert_eq!(key.as_deref(), Some("e1004"));
        assert_eq!(src.as_deref(), Some("device_config:e1004"));

        // (b) the Board header matches
        let (p, key, src) = resolve_panel(&config, None, Some("e1004"), "");
        assert!(p.is_some());
        assert_eq!(key.as_deref(), Some("e1004"));
        assert_eq!(src.as_deref(), Some("board_header:e1004"));

        // (c) the Model header matches
        let (p, key, src) = resolve_panel(&config, None, None, "e1004");
        assert!(p.is_some());
        assert_eq!(key.as_deref(), Some("e1004"));
        assert_eq!(src.as_deref(), Some("model_header:e1004"));

        // (d) nothing matches
        let (p, key, src) = resolve_panel(&config, None, None, "unknown");
        assert!(p.is_none());
        assert_eq!(key, None);
        assert_eq!(src, None);
    }

    #[test]
    fn temperature_profile_defaults_when_unset() {
        assert_eq!(resolve_temperature_profile(None), "default");
    }

    #[test]
    fn temperature_profile_passes_through_valid_values() {
        for v in ["default", "a", "b"] {
            assert_eq!(resolve_temperature_profile(Some(v)), v);
        }
    }

    #[test]
    fn temperature_profile_normalizes_case_and_whitespace() {
        // YAML makes " A " and "A" easy to write by accident, and firmware
        // compares the string it receives.
        assert_eq!(resolve_temperature_profile(Some(" A ")), "a");
        assert_eq!(resolve_temperature_profile(Some("B")), "b");
    }

    #[test]
    fn maximum_compatibility_is_omitted_from_json_when_unset() {
        // Absent must mean "no opinion", not "use fast refresh": serializing
        // `false` would actively override a device default we never chose.
        let r = DisplayJsonResponse {
            status: 0,
            image_url: None,
            filename: "x".into(),
            update_firmware: false,
            firmware_url: None,
            refresh_rate: 60,
            reset_firmware: false,
            temperature_profile: Some("default".into()),
            special_function: None,
            maximum_compatibility: None,
        };
        let j = serde_json::to_string(&r).expect("serializes");
        assert!(
            !j.contains("maximum_compatibility"),
            "unset must be omitted entirely, got {j}"
        );
    }

    #[test]
    fn maximum_compatibility_is_sent_when_set() {
        let r = DisplayJsonResponse {
            status: 0,
            image_url: None,
            filename: "x".into(),
            update_firmware: false,
            firmware_url: None,
            refresh_rate: 60,
            reset_firmware: false,
            temperature_profile: Some("a".into()),
            special_function: None,
            maximum_compatibility: Some(true),
        };
        let j = serde_json::to_string(&r).expect("serializes");
        assert!(
            j.contains("\"maximum_compatibility\":true"),
            "must reach the firmware verbatim, got {j}"
        );
    }

    /// Firmware 1.8.14 never implemented `c` — it parses to `default`, which
    /// silently *disables* the extra clearing the operator asked for. Refusing
    /// it server-side turns a silent wrong-on-glass into a log line.
    #[test]
    fn temperature_profile_refuses_c_because_firmware_ignores_it() {
        assert_eq!(resolve_temperature_profile(Some("c")), "default");
    }

    #[test]
    fn temperature_profile_refuses_unknown_values() {
        // Must not reach firmware: a bad profile fails on the glass, silently.
        for v in ["", "d", "1", "profile-a", "true"] {
            assert_eq!(
                resolve_temperature_profile(Some(v)),
                "default",
                "unknown profile {v:?} must fall back rather than be forwarded"
            );
        }
    }

    /// Pins the three settled cases of the `use_actual` rule (see
    /// `resolve_use_actual`'s doc comment): no explicit flag defaults to
    /// "on if calibrated"; an explicit `true` is a no-op without a
    /// calibration; an explicit `false` always wins.
    #[test]
    fn resolve_use_actual_defaults_to_on_when_calibrated() {
        assert!(resolve_use_actual(None, true));
    }

    #[test]
    fn resolve_use_actual_true_is_noop_without_calibration() {
        assert!(!resolve_use_actual(Some(true), false));
    }

    #[test]
    fn resolve_use_actual_false_wins_even_with_calibration() {
        assert!(!resolve_use_actual(Some(false), true));
    }

    #[test]
    fn parse_measured_color_list_drops_malformed_entries() {
        // parse_colors_header silently drops unparseable entries; the list
        // variant must do the same, one entry at a time.
        let parsed =
            parse_measured_color_list(&["#0A0A0A".to_string(), "not-a-colour".to_string()]);
        assert_eq!(parsed, vec![(0x0A, 0x0A, 0x0A)]);
    }

    #[test]
    fn script_entry_containing_a_comma_does_not_inflate_the_parsed_count() {
        // A malformed script entry that itself contains a comma must not,
        // when parsed, silently fracture into extra "valid" colors. Joining
        // the whole list with commas before parsing (the old behaviour)
        // would inflate the count from 1 real usable entry to 3.
        let items = vec![
            "#111111,#222222".to_string(), // malformed: this is one entry, not two
            "#333333".to_string(),
        ];
        let parsed = parse_measured_color_list(&items);
        assert_eq!(parsed, vec![(0x33, 0x33, 0x33)]);
    }

    /// Minimal screen-repo source: `ScriptResult` carries an `Arc` of one, and
    /// the wiring tests need a result, not a repo.
    struct NoRepo(crate::models::screen_repo_manifest::ScreenRepoManifest);
    impl crate::services::screen_repo_loader::ScreenRepoSource for NoRepo {
        fn read(&self, _rel: &str) -> Option<Vec<u8>> {
            None
        }
        fn screen_paths(&self) -> Vec<String> {
            Vec::new()
        }
        fn svg_files(&self) -> Vec<String> {
            Vec::new()
        }
        fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
            &self.0
        }
        fn screen_files(&self, _screen_path: &str) -> Vec<String> {
            Vec::new()
        }
    }

    fn script_result(colors: Option<&[&str]>, actual: Option<&[&str]>) -> ScriptResult {
        let owned = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let manifest = crate::models::screen_repo_manifest::ScreenRepoManifest::from_yaml(
            "name: t\ndescription: d\nauthor: a\nlicense: MIT\n",
        )
        .unwrap();
        ScriptResult {
            data: serde_json::Value::Null,
            refresh_rate: 900,
            script_refresh_rate: None,
            screen_default_refresh: 900,
            skip_update: false,
            screen_name: "t".into(),
            source: Arc::new(NoRepo(manifest)),
            screen_dir: String::new(),
            params: Default::default(),
            script_colors: colors.map(owned),
            script_colors_actual: actual.map(owned),
            script_dither: Some("sierra-lite".into()),
            script_max_error: Some(0.11),
            script_noise_scale: Some(0.12),
            script_chroma_clamp: Some(0.13),
            script_strength: Some(0.14),
            script_gamut: None,
            font_hinting: None,
            logs: Vec::new(),
        }
    }

    /// Every rung carries the value from the binding it is meant to carry.
    ///
    /// If this breaks, it means: two bindings were swapped in
    /// `render_inputs_for`. The resolver would accept that happily and the
    /// device would render against another source's calibration or
    /// algorithm; nothing else in the suite compares dither precedence
    /// against the ladder, so nothing else would notice.
    #[test]
    fn render_inputs_for_assigns_each_binding_to_its_own_rung() {
        let result = script_result(Some(&["#000001", "#FFFFFE"]), Some(&["#010101", "#FEFEFE"]));
        let over_actual = parse_colors_header("#020202,#FDFDFD");
        let panel_actual = parse_colors_header("#030303,#FCFCFC");
        let header_actual = parse_colors_header("#040404,#FBFBFB");
        let dev_tuning = DitherTuningValues {
            max_error: Some(0.91),
            ..Default::default()
        };
        let device_final = CascadeValues {
            dither: Some("floyd_steinberg".into()),
            colors_actual: Some("#050505,#FAFAFA".into()),
            ..Default::default()
        };
        let device_base = CascadeValues {
            dither: Some("atkinson".into()),
            colors_actual: Some("#060606,#F9F9F9".into()),
            ..Default::default()
        };
        let fallback = parse_colors_header("#0A0A0A,#F0F0F0");
        let i = render_inputs_for(
            &result,
            Some(&over_actual),
            Some("stucki"),
            Some(&dev_tuning),
            &device_final,
            &device_base,
            Some("#000002,#FFFFFD"),
            Some(&panel_actual),
            None,
            fallback.clone(),
            Some(&header_actual),
        );

        // Override: the dev override and nothing else.
        assert_eq!(i.over.colors_actual, Some(over_actual));
        assert_eq!(i.over.dither.as_deref(), Some("stucki"));
        assert_eq!(i.over.tuning.max_error, Some(0.91));
        // Final and base come from their own cascade values.
        assert_eq!(i.device_final.dither.as_deref(), Some("floyd_steinberg"));
        assert_eq!(
            i.device_final.colors_actual,
            Some(parse_colors_header("#050505,#FAFAFA"))
        );
        assert_eq!(i.device_base.dither.as_deref(), Some("atkinson"));
        assert_eq!(
            i.device_base.colors_actual,
            Some(parse_colors_header("#060606,#F9F9F9"))
        );
        // Screen: the script's return value, normalised.
        assert_eq!(
            i.screen.colors,
            Some(parse_colors_header("#000001,#FFFFFE"))
        );
        assert_eq!(
            i.screen.colors_actual,
            Some(parse_colors_header("#010101,#FEFEFE"))
        );
        assert_eq!(i.screen.dither.as_deref(), Some("sierra-lite"));
        assert_eq!(i.screen.tuning.max_error, Some(0.11));
        assert_eq!(i.screen.tuning.noise_scale, Some(0.12));
        assert_eq!(i.screen.tuning.chroma_clamp, Some(0.13));
        assert_eq!(i.screen.tuning.strength, Some(0.14));
        // Panel: its palette and its measurement, no dither of its own here.
        assert_eq!(i.panel.colors, Some(parse_colors_header("#000002,#FFFFFD")));
        assert_eq!(i.panel.colors_actual, Some(panel_actual));
        assert_eq!(i.panel.dither, None);
        // Firmware self-report and model default are fallbacks, not rungs.
        assert_eq!(i.fallback_palette, fallback);
        assert_eq!(i.fallback_colors_actual, Some(header_actual));
        assert_eq!(i.over.colors, None);
        assert_eq!(i.device_final.colors, None);
        assert_eq!(i.device_base.colors, None);
    }

    /// A script's `colors_actual` entry that itself contains a comma must be
    /// rejected against a palette of a different length, not fractured into
    /// extra "valid" colours by a join-then-split.
    ///
    /// If this breaks, it means: the screen rung's `colors_actual` is parsed
    /// through a join again, and a mismatched script calibration is accepted.
    #[test]
    fn a_script_colors_actual_entry_with_a_comma_is_rejected_not_fractured() {
        use crate::api::resolution::{resolve_render, Rung, RungStatus};
        // Two entries, one of them malformed: 1 usable colour. A join would
        // turn it into 3, matching the 3-colour palette below.
        let result = script_result(None, Some(&["#111111,#222222", "#333333"]));
        let panel_actual = parse_colors_header("#0A0A0A,#808080,#F5F5F5");
        let res = resolve_render(&render_inputs_for(
            &result,
            None,
            None,
            None,
            &CascadeValues::default(),
            &CascadeValues::default(),
            None,
            Some(&panel_actual),
            None,
            parse_colors_header("#000000,#808080,#FFFFFF"),
            None,
        ));
        let report = &res.report["colors_actual"];
        assert!(
            matches!(
                report.rungs[&Rung::Screen].status,
                RungStatus::Rejected { .. }
            ),
            "got {:?}",
            report.rungs[&Rung::Screen].status
        );
        assert_eq!(
            report.from,
            Rung::Panel,
            "must fall through to a lower rung"
        );
        let params = res.into_params();
        assert_eq!(params.measured_colors, Some(panel_actual));
        assert_eq!(params.warnings.len(), 1);
    }

    /// A device/screen dither disagreement is the cascade working as
    /// specified (§4.3), so it is no longer a warning: the screen wins.
    #[test]
    fn a_dither_disagreement_produces_no_warning() {
        use crate::api::resolution::resolve_render;
        let result = script_result(None, None); // screen dither: sierra-lite
        let device_base = CascadeValues {
            dither: Some("atkinson".into()),
            ..Default::default()
        };
        let params = resolve_render(&render_inputs_for(
            &result,
            None,
            None,
            None,
            &CascadeValues::default(),
            &device_base,
            None,
            None,
            None,
            parse_colors_header("#000000,#FFFFFF"),
            None,
        ))
        .into_params();
        assert_eq!(params.dither.as_deref(), Some("sierra-lite"));
        assert!(
            params.warnings.is_empty(),
            "the cascade working as specified is not a warning: {:?}",
            params.warnings
        );
    }

    #[test]
    fn render_params_carry_gamut_into_the_dither_tuning() {
        let params = RenderParams {
            palette: vec![(0, 0, 0), (255, 255, 255)],
            measured_colors: None,
            measured_source: SRC_NONE,
            dither: None,
            max_error: None,
            noise_scale: None,
            chroma_clamp: None,
            strength: None,
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.45),
                ..Default::default()
            },
            warnings: Vec::new(),
        };
        let (tuning, has_tuning) = resolve_dither_tuning(&params);
        assert!(has_tuning, "a gamut knob is a tuning override");
        assert_eq!(tuning.gamut.expect("gamut must be set").knee, 0.45);
    }
}
