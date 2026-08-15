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
use crate::models::{
    normalize_algorithm_name, verify_ed25519_signature, ApiKey, AppConfig, Device, DeviceId,
    DisplaySpec, DitherTuningValues,
};
use crate::server::DevOverrides;
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
    pub error_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
    pub gamut: crate::models::GamutTuningValues,
}

/// Resolve dither tuning parameters.
/// Priority: script > device config > panel > None (algorithm defaults)
pub fn resolve_tuning(
    script: &DitherTuningValues,
    device_config: &DitherTuningValues,
    panel: &DitherTuningValues,
) -> DitherTuningValues {
    script.or(device_config).or(panel)
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
/// `resolve_ctx_palette`/`resolve_render_params` sit on top of. Shared by
/// `/dev/render` and `ScreenStore::render`.
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

/// If `override_tuning` has any field set, it wins wholesale — an explicit
/// UI/API tuning override beats the script/device-config/panel chain even
/// for the fields the caller left `None` (matching `/dev/render`'s existing
/// "an explicit override replaces the whole tuning struct" behavior).
/// Otherwise falls back to the normal `resolve_tuning` priority chain.
/// Shared by `/dev/render` and `ScreenStore::render`.
pub fn resolve_effective_tuning(
    override_tuning: &DitherTuningValues,
    script_tuning: &DitherTuningValues,
    device_config_tuning: &DitherTuningValues,
    panel_tuning: &DitherTuningValues,
) -> DitherTuningValues {
    if override_tuning.error_clamp.is_some()
        || override_tuning.noise_scale.is_some()
        || override_tuning.chroma_clamp.is_some()
        || override_tuning.strength.is_some()
        || !override_tuning.gamut.is_empty()
    {
        override_tuning.clone()
    } else {
        resolve_tuning(script_tuning, device_config_tuning, panel_tuning)
    }
}

/// Build the eink-dither tuning override + whether any override is
/// actually present, from resolved `RenderParams`. Shared by `/dev/render`
/// and `ScreenStore::render` so "did the user override anything" can't
/// drift between the two callers.
pub fn resolve_dither_tuning(
    render_params: &RenderParams,
) -> (crate::rendering::svg_to_png::DitherTuning, bool) {
    let tuning = crate::rendering::svg_to_png::DitherTuning {
        serpentine: None,
        error_clamp: render_params.error_clamp,
        chroma_clamp: render_params.chroma_clamp,
        noise_scale: render_params.noise_scale,
        strength: render_params.strength,
        gamut: Some(render_params.gamut.resolve()),
    };
    let has_tuning = tuning.error_clamp.is_some()
        || tuning.chroma_clamp.is_some()
        || tuning.noise_scale.is_some()
        || tuning.strength.is_some()
        || !render_params.gamut.is_empty();
    (tuning, has_tuning)
}

/// Source labels for [`MeasuredResolution::source`] / [`resolve_measured_colors`].
///
/// Defined once so the four call sites (`api::display`, `api::dev`,
/// `services::screen_store`, `main`) can't drift on spelling. The resolved
/// value is not rendered anywhere in the dev UI — it's surfaced via
/// `tracing` fields (server logs) and via `RenderResult::measured_source` /
/// the MCP `render_screen` tool's diagnostics.
pub const SRC_SCRIPT: &str = "script";
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
/// `None` means that source wasn't supplied at all (distinct from being
/// supplied with the wrong length, which is still `Some` but discarded by
/// the length rule in [`resolve_measured_colors`]).
pub type MeasuredCandidate = (&'static str, Option<Vec<(u8, u8, u8)>>);

/// Outcome of resolving the measured ("actual") panel colours.
///
/// `source` names which layer supplied the value, for the debug log and the
/// dev UI; `warning` carries a human-readable diagnostic that the caller is
/// responsible for surfacing — `tracing::warn!` on device paths, the script
/// log on authoring paths.
pub struct MeasuredResolution {
    pub colors: Option<Vec<(u8, u8, u8)>>,
    pub source: &'static str,
    pub warning: Option<String>,
}

/// Resolve the measured colours for a render from an ordered list of
/// candidate sources.
///
/// `candidates` is the full chain in precedence order, e.g.
/// `[(SRC_SCRIPT, ..), (SRC_DEV_OVERRIDE, ..), (SRC_PANEL_ACTUAL, ..),
/// (SRC_MEASURED_HEADER, ..)]` — each entry already parsed into RGB tuples
/// by the caller (see [`parse_measured_color_list`] for the script list,
/// [`parse_colors_header`] for the comma-joined string sources).
///
/// The chain is walked in order. A candidate that is `None` is skipped
/// silently — that source simply wasn't supplied. A candidate that IS
/// supplied but whose length doesn't match `palette_len` is **discarded,
/// not fatal**: a device fetching its screen must never be denied content
/// over a calibration mistake at any single layer. The length rule applies
/// uniformly to every position in the chain, not just the first — every
/// mismatch is recorded and the walk continues to the next candidate. The
/// first candidate whose length matches wins outright.
///
/// If no candidate resolves, `colors` is `None` and `source` is
/// [`SRC_NONE`]. `warning`, when present, is the concatenation of every
/// mismatch encountered along the way (not just the first) — these are
/// diagnostics a script author or panel maintainer reads, so all of them
/// are worth surfacing, not just the one that happened to be checked first.
pub fn resolve_measured_colors(
    palette_len: usize,
    candidates: &[MeasuredCandidate],
) -> MeasuredResolution {
    let mut warnings: Vec<String> = Vec::new();

    for (source, candidate) in candidates {
        let Some(colors) = candidate else {
            continue;
        };
        if colors.len() == palette_len {
            return MeasuredResolution {
                colors: Some(colors.clone()),
                source,
                warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
            };
        }
        warnings.push(format!(
            "{source}: colors_actual has {} usable entries but the resolved \
             palette has {}; skipping it. (Entries that are not 6-digit hex \
             are dropped, which also shortens the list.)",
            colors.len(),
            palette_len
        ));
    }

    MeasuredResolution {
        colors: None,
        source: SRC_NONE,
        warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
    }
}

/// Resolve all rendering parameters after script execution.
///
/// Palette:  script_colors > device_config_colors > panel_colors > fallback
/// Measured: script_colors_actual > pre_script_measured_candidates (in the
///           order the caller supplies them, e.g. dev override >
///           panel.colors_actual > Measured-Colors header)
/// Dither: script_dither > device_config_dither > None
///
/// `pre_script_measured_candidates` is the caller's own pre-script chain —
/// each entry already parsed and labelled by the caller, in precedence
/// order (see each call site's own doc comment for its exact chain: not
/// every caller has a dev-override or header layer). This function doesn't
/// re-derive that chain; it just prepends the script's own `colors_actual`
/// and applies the length rule uniformly across the whole thing, so a
/// mismatch at ANY position — including inside the caller's own chain —
/// falls through to the next, not just a mismatch at the very front. A
/// mismatch never fails the render: it's recorded into `warning_sink` and
/// the resolver falls through (see [`resolve_measured_colors`]). The caller
/// decides where that warning goes — `tracing::warn!` on device paths, the
/// script log on authoring paths.
#[allow(clippy::too_many_arguments)]
pub fn resolve_render_params(
    script_colors: Option<&[String]>,
    script_colors_actual: Option<&[String]>,
    script_dither: Option<&str>,
    device_config_colors: Option<&str>,
    device_config_dither: Option<&str>,
    panel_colors: Option<&str>,
    fallback_palette: &[(u8, u8, u8)],
    pre_script_measured_candidates: &[MeasuredCandidate],
    tuning: &DitherTuningValues,
    warning_sink: &mut Option<String>,
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

    // Canonicalize here, at the single point where the effective algorithm is
    // decided, so the renderer and the per-algorithm tuning lookup can never
    // disagree about which algorithm was asked for. The renderer matches
    // canonical names only and silently falls back to Atkinson otherwise, so
    // an un-normalized alias reaching it is a silent wrong-algorithm render.
    let dither = script_dither
        .map(|s| s.to_string())
        .or_else(|| device_config_dither.map(|s| s.to_string()))
        .map(|s| crate::models::normalize_algorithm_name(&s));

    let script_measured = script_colors_actual.map(parse_measured_color_list);
    let mut candidates: Vec<MeasuredCandidate> =
        Vec::with_capacity(1 + pre_script_measured_candidates.len());
    candidates.push((SRC_SCRIPT, script_measured));
    candidates.extend_from_slice(pre_script_measured_candidates);
    let measured = resolve_measured_colors(palette.len(), &candidates);
    *warning_sink = measured.warning;

    RenderParams {
        palette,
        measured_colors: measured.colors,
        measured_source: measured.source,
        dither,
        error_clamp: tuning.error_clamp,
        noise_scale: tuning.noise_scale,
        chroma_clamp: tuning.chroma_clamp,
        strength: tuning.strength,
        gamut: tuning.gamut.clone(),
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

            let cached = CachedContent::new(registration_svg, screen_name, width, height)
                .with_colors(Some(palette));
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
    } else if let Some((name, p)) = config.find_panel_for_board(model_str) {
        // Some devices (e.g. reTerminal E1004) report their panel identity in the
        // `Model` header rather than `Board`; fall back to matching against it.
        (Some(p), Some(format!("model_header:{}", name)))
    } else {
        (None, None)
    };

    // Resolve measured colors: dev override > panel.colors_actual > Measured-Colors header > None
    // Dev overrides are keyed by device config key (not panel name) so each device
    // can be tuned independently even when sharing the same panel profile.
    let measured_colors_header = headers.get_str("Measured-Colors").map(|s| s.to_string());
    let override_colors = if let Some(ref key) = device_entry_key {
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
    // The pre-script chain, as a candidate array in precedence order — the
    // final resolution (after the script runs and the palette length is
    // known) prepends the script's own `colors_actual` in front of this and
    // applies the length rule uniformly across all four, so a mismatch at
    // ANY position falls through to the next rather than only the front.
    let pre_script_measured_candidates: Vec<MeasuredCandidate> = vec![
        (SRC_DEV_OVERRIDE, override_colors_parsed.clone()),
        (SRC_PANEL_ACTUAL, panel_actual_parsed.clone()),
        (SRC_MEASURED_HEADER, header_parsed.clone()),
    ];
    // Pre-script winner, used only to populate `DeviceContext.colors_actual`
    // (what the *script* sees before it runs, when the final palette length
    // isn't known yet — no length check applies here). Derived from
    // `pre_script_measured_candidates` above rather than its own
    // if/else-if chain: two separately-maintained encodings of the same
    // precedence order drift apart silently (see Task 1's `main.rs`
    // finding, of which this was a recurrence) — add a source to the array
    // and forget the if/else-if (or vice versa) and this and the final
    // dithered render quietly stop agreeing on what "measured" means. Not
    // length-checked (unlike the final resolution): this is just "first
    // supplied", the palette length isn't known yet.
    let pre_script_measured_winner = pre_script_measured_candidates
        .iter()
        .find_map(|(s, c)| c.clone().map(|c| (*s, c)));
    let pre_script_measured_source = pre_script_measured_winner
        .as_ref()
        .map_or(SRC_NONE, |(s, _)| *s);
    let measured_colors = pre_script_measured_winner.map(|(_, c)| c);

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
        pre_script_measured_source = pre_script_measured_source,
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

    // Tuning: dev override > device config > panel > None (algorithm defaults)
    let dev_tuning_override: Option<DitherTuningValues> = if let Some(ref key) = device_entry_key {
        dev_overrides.tuning.read().await.get(key).cloned()
    } else {
        None
    };
    let dc_tuning = DitherTuningValues {
        error_clamp: device_config.and_then(|dc| dc.error_clamp),
        noise_scale: device_config.and_then(|dc| dc.noise_scale),
        chroma_clamp: device_config.and_then(|dc| dc.chroma_clamp),
        strength: device_config.and_then(|dc| dc.strength),
        gamut: device_config.map(|dc| dc.gamut.clone()).unwrap_or_default(),
    };

    // Resolve panel dither config for pre-script algorithm
    let panel_dither_config = panel.and_then(|p| p.dither.clone());
    // Pre-script algorithm: dev override > device config > default "atkinson"
    let pre_script_algo = dev_dither_override
        .as_deref()
        .or(device_config_dither.as_deref())
        .unwrap_or("atkinson");
    let panel_tuning = panel_dither_config
        .as_ref()
        .map(|pdc| pdc.resolve_for_algorithm(Some(pre_script_algo)))
        .unwrap_or_default();
    // Pre-script resolved tuning for device context: dc > panel
    let pre_script_tuning = dc_tuning.or(&panel_tuning);

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
        colors_actual: measured_colors.as_deref().map(colors_to_hex_strings),
        dither_algorithm: Some(pre_script_algo.to_string()),
        dither_error_clamp: pre_script_tuning.error_clamp,
        dither_noise_scale: pre_script_tuning.noise_scale,
        dither_chroma_clamp: pre_script_tuning.chroma_clamp,
        dither_strength: pre_script_tuning.strength,
        dither_gamut_knee: pre_script_tuning.gamut.knee,
        dither_gamut_amount: pre_script_tuning.gamut.amount,
        dither_gamut_max_compression: pre_script_tuning.gamut.max_compression,
        refresh_override: None,
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
    let dev_dither = dev_dither_override;
    let dev_tuning = dev_tuning_override;
    let dc_tuning_for_closure = dc_tuning;
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
                        measured_colors = ?measured_colors.as_ref().map(|mc| colors_to_hex_strings(mc)),
                        "Resolving render params for device"
                    );

                    // Dev UI dither override beats everything (including script)
                    let (eff_script_dither, eff_dc_dither) = if dev_dither.is_some() {
                        (None, dev_dither.as_deref())
                    } else {
                        (result.script_dither.as_deref(), dc_dither.as_deref())
                    };

                    // Determine final algorithm for panel tuning resolution
                    let final_algo_str = if dev_dither.is_some() {
                        dev_dither.as_deref()
                    } else {
                        result
                            .script_dither
                            .as_deref()
                            .or(dc_dither.as_deref())
                    };
                    let final_algo_normalized =
                        final_algo_str.map(normalize_algorithm_name);

                    // Re-resolve panel tuning for the final algorithm (may differ from pre-script)
                    let panel_final_tuning = panel_dither_for_closure
                        .as_ref()
                        .map(|pdc| {
                            pdc.resolve_for_algorithm(final_algo_normalized.as_deref())
                        })
                        .unwrap_or_default();

                    let script_tuning = DitherTuningValues {
                        error_clamp: result.script_error_clamp,
                        noise_scale: result.script_noise_scale,
                        chroma_clamp: result.script_chroma_clamp,
                        strength: result.script_strength,
                        gamut: result.script_gamut.clone().unwrap_or_default(),
                    };

                    // Resolve tuning: dev override > script > device config > panel > algorithm defaults
                    let tuning = if let Some(ref dt) = dev_tuning {
                        dt.clone()
                    } else {
                        resolve_tuning(&script_tuning, &dc_tuning_for_closure, &panel_final_tuning)
                    };

                    let mut measured_warning: Option<String> = None;
                    let params = resolve_render_params(
                        result.script_colors.as_deref(),
                        result.script_colors_actual.as_deref(),
                        eff_script_dither,
                        dc_colors.as_deref(),
                        eff_dc_dither,
                        panel_colors_for_chain.as_deref(),
                        &fallback,
                        &pre_script_measured_candidates,
                        &tuning,
                        &mut measured_warning,
                    );
                    if let Some(w) = &measured_warning {
                        tracing::warn!(device = %mac, "{w}");
                    }

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
                                let cached = CachedContent::new(
                                    svg,
                                    result.screen_name.clone(),
                                    width,
                                    height,
                                )
                                .with_colors(Some(params.palette))
                                .with_colors_actual(params.measured_colors)
                                .with_dither(params.dither)
                                .with_tuning(&tuning);
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
        error_clamp: cached.error_clamp,
        chroma_clamp: cached.chroma_clamp,
        noise_scale: cached.noise_scale,
        strength: cached.strength,
        gamut: Some(cached.gamut.resolve()),
    };
    let has_tuning = tuning.error_clamp.is_some()
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
        // Task 7 wires the screen's resolved font config here.
        None,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn first_candidate_wins_when_length_matches() {
        let script = parse_measured_color_list(&["#0A0A0A".to_string(), "#E8E6E0".to_string()]);
        let r = resolve_measured_colors(
            2,
            &[
                (SRC_SCRIPT, Some(script)),
                (SRC_PANEL_ACTUAL, Some(vec![(1, 1, 1), (2, 2, 2)])),
            ],
        );
        assert_eq!(
            r.colors.unwrap(),
            vec![(0x0A, 0x0A, 0x0A), (0xE8, 0xE6, 0xE0)]
        );
        assert_eq!(r.source, SRC_SCRIPT);
        assert!(r.warning.is_none());
    }

    #[test]
    fn falls_back_to_next_candidate_when_first_is_absent() {
        let r = resolve_measured_colors(
            2,
            &[
                (SRC_SCRIPT, None),
                (SRC_PANEL_ACTUAL, Some(vec![(1, 1, 1), (2, 2, 2)])),
            ],
        );
        assert_eq!(r.colors.unwrap(), vec![(1, 1, 1), (2, 2, 2)]);
        assert_eq!(r.source, SRC_PANEL_ACTUAL);
        assert!(r.warning.is_none());
    }

    #[test]
    fn reports_none_when_no_candidates_resolve() {
        let r = resolve_measured_colors(
            4,
            &[
                (SRC_SCRIPT, None),
                (SRC_DEV_OVERRIDE, None),
                (SRC_PANEL_ACTUAL, None),
                (SRC_MEASURED_HEADER, None),
            ],
        );
        assert!(r.colors.is_none());
        assert_eq!(r.source, SRC_NONE);
        assert!(r.warning.is_none());
    }

    #[test]
    fn length_mismatch_at_first_position_falls_through_to_second() {
        let script = parse_measured_color_list(&["#0A0A0A".to_string(), "#E8E6E0".to_string()]);
        let r = resolve_measured_colors(
            4, // official palette has 4 entries, script supplied 2
            &[
                (SRC_SCRIPT, Some(script)),
                (
                    SRC_PANEL_ACTUAL,
                    Some(vec![(1, 1, 1), (2, 2, 2), (3, 3, 3), (4, 4, 4)]),
                ),
            ],
        );
        // Fell through to the next source, did NOT blank the calibration.
        assert_eq!(r.colors.unwrap().len(), 4);
        assert_eq!(r.source, SRC_PANEL_ACTUAL);
        let w = r.warning.expect("a mismatch must be reported");
        assert!(
            w.contains("has 2 usable"),
            "warning must name the mismatched count: {w}"
        );
        assert!(
            w.contains("palette has 4"),
            "warning must name the resolved palette length: {w}"
        );
    }

    #[test]
    fn length_mismatch_at_middle_position_falls_through_to_later_valid_candidate() {
        // script absent, dev_override mismatched, panel.colors_actual valid.
        let r = resolve_measured_colors(
            3,
            &[
                (SRC_SCRIPT, None),
                (SRC_DEV_OVERRIDE, Some(vec![(9, 9, 9), (8, 8, 8)])), // len 2, wrong
                (
                    SRC_PANEL_ACTUAL,
                    Some(vec![(1, 1, 1), (2, 2, 2), (3, 3, 3)]),
                ), // len 3, right
                (
                    SRC_MEASURED_HEADER,
                    Some(vec![(7, 7, 7), (6, 6, 6), (5, 5, 5)]),
                ), // never reached
            ],
        );
        assert_eq!(r.colors.unwrap(), vec![(1, 1, 1), (2, 2, 2), (3, 3, 3)]);
        assert_eq!(r.source, SRC_PANEL_ACTUAL);
        let w = r
            .warning
            .expect("the skipped middle candidate must be reported");
        assert!(
            w.contains(SRC_DEV_OVERRIDE)
                && w.contains("has 2 usable")
                && w.contains("palette has 3"),
            "warning must name the middle candidate that was skipped: {w}"
        );
    }

    #[test]
    fn all_candidates_mismatch_falls_through_to_none_with_accumulated_warnings() {
        let r = resolve_measured_colors(
            3,
            &[
                (SRC_SCRIPT, Some(vec![(1, 1, 1)])), // len 1, wrong
                (SRC_DEV_OVERRIDE, Some(vec![(2, 2, 2), (2, 2, 2)])), // len 2, wrong
            ],
        );
        assert!(r.colors.is_none());
        assert_eq!(r.source, SRC_NONE);
        let w = r.warning.expect("every mismatch must be reported");
        assert!(
            w.contains(SRC_SCRIPT) && w.contains("has 1 usable"),
            "warning must mention the script mismatch: {w}"
        );
        assert!(
            w.contains(SRC_DEV_OVERRIDE) && w.contains("has 2 usable"),
            "warning must mention the dev_override mismatch, not just the first one: {w}"
        );
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
    fn malformed_hex_is_caught_by_the_length_check() {
        // A typo shortens the parsed list; the length rule is what turns
        // that into a diagnostic instead of a silent half-calibration.
        let script =
            parse_measured_color_list(&["#0A0A0A".to_string(), "not-a-colour".to_string()]);
        assert_eq!(
            script.len(),
            1,
            "the malformed entry must have been dropped"
        );
        let r = resolve_measured_colors(2, &[(SRC_SCRIPT, Some(script))]);
        assert!(r.colors.is_none());
        assert_eq!(r.source, SRC_NONE);
        let w = r.warning.expect("a mismatch must be reported");
        assert!(
            w.contains("has 1 usable"),
            "warning must name 1 usable entry: {w}"
        );
        assert!(
            w.contains("palette has 2"),
            "warning must name the palette length: {w}"
        );
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

    // ------------------------------------------------------------------
    // resolve_render_params: the shared plumbing every one of the four
    // call sites (api::display, api::dev, services::screen_store, main)
    // goes through. These pin the two properties a mis-wired call site
    // would silently violate: script always outranks whatever the caller
    // passes as its pre-script chain, and the caller's own chain is walked
    // in the order supplied — not reordered, not collapsed to just the
    // first entry.
    // ------------------------------------------------------------------

    fn default_tuning() -> DitherTuningValues {
        DitherTuningValues::default()
    }

    #[test]
    fn resolve_render_params_prefers_script_colors_actual_over_pre_script_chain() {
        // Distinct, non-guessable RGB triples per source so a wrong-source
        // wiring bug fails the assertion instead of accidentally matching.
        let script_actual = vec!["#111111".to_string(), "#222222".to_string()];
        let pre_script: Vec<MeasuredCandidate> = vec![(
            SRC_PANEL_ACTUAL,
            Some(vec![(0x99, 0x99, 0x99), (0x88, 0x88, 0x88)]),
        )];
        let mut warning = None;
        let params = resolve_render_params(
            None,
            Some(&script_actual),
            None,
            None,
            None,
            None,
            &[(0, 0, 0), (255, 255, 255)],
            &pre_script,
            &default_tuning(),
            &mut warning,
        );
        assert_eq!(
            params.measured_colors.unwrap(),
            vec![(0x11, 0x11, 0x11), (0x22, 0x22, 0x22)],
            "script's colors_actual must win over the caller's pre-script chain"
        );
        assert_eq!(
            params.measured_source, SRC_SCRIPT,
            "measured_source must name the script as the winning layer, not \
             whatever the caller's own pre-script chain resolved to"
        );
        assert!(warning.is_none());
    }

    #[test]
    fn resolve_render_params_walks_pre_script_chain_in_supplied_order() {
        // Mirrors api::display's own three-source chain: dev_override >
        // panel.colors_actual > Measured-Colors header. No script value at
        // all; dev_override is present but the wrong length (mismatch,
        // skipped); panel.colors_actual is right; header is never reached.
        // If the call site swapped panel/header order, this would resolve
        // to the header's value instead — a silent, non-compiling bug this
        // test is built to catch.
        let pre_script: Vec<MeasuredCandidate> = vec![
            (SRC_DEV_OVERRIDE, Some(vec![(0x01, 0x01, 0x01)])), // len 1, wrong
            (
                SRC_PANEL_ACTUAL,
                Some(vec![(0xAA, 0xBB, 0xCC), (0xDD, 0xEE, 0xFF)]),
            ), // len 2, right
            (
                SRC_MEASURED_HEADER,
                Some(vec![(0x44, 0x44, 0x44), (0x55, 0x55, 0x55)]),
            ), // never reached
        ];
        let mut warning = None;
        let params = resolve_render_params(
            None,
            None,
            None,
            None,
            None,
            None,
            &[(0, 0, 0), (255, 255, 255)],
            &pre_script,
            &default_tuning(),
            &mut warning,
        );
        assert_eq!(
            params.measured_colors.unwrap(),
            vec![(0xAA, 0xBB, 0xCC), (0xDD, 0xEE, 0xFF)],
            "must resolve to panel.colors_actual, the second entry in the supplied order"
        );
        let w = warning.expect("the skipped dev_override mismatch must be reported");
        assert!(
            w.contains(SRC_DEV_OVERRIDE),
            "warning must name the skipped source: {w}"
        );
    }

    #[test]
    fn resolve_render_params_script_mismatch_falls_through_to_pre_script_chain() {
        // Script supplies colors_actual but at the wrong length for the
        // resolved (2-entry) palette; the caller's pre-script chain must
        // still be consulted rather than the render losing calibration
        // outright.
        let script_actual = vec!["#010101".to_string()]; // len 1, wrong for a 2-entry palette
        let pre_script: Vec<MeasuredCandidate> = vec![(
            SRC_PANEL_ACTUAL,
            Some(vec![(0x10, 0x20, 0x30), (0x40, 0x50, 0x60)]),
        )];
        let mut warning = None;
        let params = resolve_render_params(
            None,
            Some(&script_actual),
            None,
            None,
            None,
            None,
            &[(0, 0, 0), (255, 255, 255)],
            &pre_script,
            &default_tuning(),
            &mut warning,
        );
        assert_eq!(
            params.measured_colors.unwrap(),
            vec![(0x10, 0x20, 0x30), (0x40, 0x50, 0x60)]
        );
        let w = warning.expect("the script mismatch must be reported");
        assert!(w.contains(SRC_SCRIPT), "warning must name script: {w}");
    }

    #[test]
    fn resolve_render_params_no_candidates_resolve_to_none_without_failing() {
        let mut warning = None;
        let params = resolve_render_params(
            None,
            None,
            None,
            None,
            None,
            None,
            &[(0, 0, 0), (255, 255, 255)],
            &[],
            &default_tuning(),
            &mut warning,
        );
        assert!(params.measured_colors.is_none());
        assert!(warning.is_none());
        // A render must still produce a palette even with no measured colors.
        assert_eq!(params.palette, vec![(0, 0, 0), (255, 255, 255)]);
    }

    #[test]
    fn gamut_follows_the_script_over_device_over_panel_priority() {
        let script = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.4),
                ..Default::default()
            },
            ..Default::default()
        };
        let device = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.7),
                amount: Some(0.5),
                ..Default::default()
            },
            ..Default::default()
        };
        let panel = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.9),
                amount: Some(1.0),
                max_compression: Some(4.0),
            },
            ..Default::default()
        };

        let resolved = resolve_tuning(&script, &device, &panel);
        assert_eq!(resolved.gamut.knee, Some(0.4), "script must win");
        assert_eq!(resolved.gamut.amount, Some(0.5), "device fills the gap");
        assert_eq!(
            resolved.gamut.max_compression,
            Some(4.0),
            "panel fills what neither set"
        );
    }

    #[test]
    fn a_gamut_only_override_counts_as_an_override() {
        // `resolve_effective_tuning` short-circuits when any override field is
        // set. A gamut-only override must not be silently ignored.
        let over = DitherTuningValues {
            gamut: crate::models::GamutTuningValues {
                amount: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let other = DitherTuningValues {
            error_clamp: Some(0.5),
            ..Default::default()
        };
        let resolved = resolve_effective_tuning(&over, &other, &other, &other);
        assert_eq!(resolved.gamut.amount, Some(0.0));
        assert_eq!(
            resolved.error_clamp, None,
            "an explicit override replaces the whole struct"
        );
    }

    #[test]
    fn render_params_carry_gamut_into_the_dither_tuning() {
        let params = RenderParams {
            palette: vec![(0, 0, 0), (255, 255, 255)],
            measured_colors: None,
            measured_source: SRC_NONE,
            dither: None,
            error_clamp: None,
            noise_scale: None,
            chroma_clamp: None,
            strength: None,
            gamut: crate::models::GamutTuningValues {
                knee: Some(0.45),
                ..Default::default()
            },
        };
        let (tuning, has_tuning) = resolve_dither_tuning(&params);
        assert!(has_tuning, "a gamut knob is a tuning override");
        assert_eq!(tuning.gamut.expect("gamut must be set").knee, 0.45);
    }
}
