//! `GET /api/admin/devices/{key}/preview` — what a device's panel currently
//! shows, as a PNG.
//!
//! This exists for the Home Assistant integration's device page, where a
//! camera entity displays it while you configure the device. That client
//! pulls a frame every few seconds for as long as somebody is looking, so
//! every request is answered from [`PreviewCache`] unless the render is
//! genuinely stale — see that module for the two rules that decide.
//!
//! The render itself goes through `ScreenStore::render` with the device's
//! params, identity and config layer supplied (`DevicePreview`), rather than
//! a second copy of `handle_display`'s resolution chain. `handle_display` is
//! built around firmware headers — `Colors`, `Board`, `Measured-Colors`,
//! `Width`/`Height` — which a preview request does not have and must not
//! invent; going through the authoring renderer with the device's own
//! configuration is both shorter and the only version that cannot drift out
//! of step with `/dev/render` and the MCP preview.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderName, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::require_admin;
use crate::api::display::{resolve_preview_dimensions, resolve_query_palette};
use crate::error::ApiError;
use crate::models::{config::DeviceConfig, DisplaySpec, DitherTuningValues};
use crate::server::AppState;
use crate::services::screen_store::{DevicePreview, RenderOpts};
use crate::services::DeviceRegistry;

/// Names whether the response was rendered or re-served. Purely diagnostic —
/// it is what makes the cache observable from the outside, both in a test and
/// when staring at a live add-on wondering why a preview will not move.
static CACHE_HEADER: HeaderName = HeaderName::from_static("x-byonk-preview");

#[derive(Deserialize)]
pub struct PreviewQuery {
    /// Present in any form (`?force`, `?force=1`) means re-render regardless
    /// of the cache — the "Refresh preview" button, for when the screen's
    /// *data* moved although its configuration did not. Taken as a
    /// presence flag rather than a parsed bool so `?force=1` works;
    /// `serde_urlencoded` only accepts `true`/`false` for a real bool.
    force: Option<String>,
    /// `off`/`0`/`false`/`no` returns the screen *before* dithering: the
    /// full-colour rasterization of the SVG, with no palette restriction at
    /// all. What the screen would look like on a display that could show it.
    /// Absent or anything else keeps the dithered render, which is what the
    /// panel receives.
    dither: Option<String>,
    /// `off`/`0`/`false`/`no` draws the palette in its *spec* colours — the
    /// values byonk sends to the panel — instead of the measured colours a
    /// calibration says the panel really produces. Absent leaves the normal
    /// rule (measured when a calibration resolved, spec otherwise).
    ///
    /// Has no effect when `dither` is off: an undithered render has no
    /// palette to map.
    measured: Option<String>,
}

/// Read a query parameter as a flag. `None` means the caller did not say.
///
/// Deliberately not `serde`'s bool: these come from a URL a human or a Home
/// Assistant entity builds, where `0`, `off` and `no` are all natural ways to
/// say no and `serde_urlencoded` accepts none of them.
fn flag(value: Option<&String>) -> Option<bool> {
    let v = value?.to_ascii_lowercase();
    Some(!matches!(v.as_str(), "0" | "false" | "off" | "no"))
}

pub async fn device_preview(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<PreviewQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_admin(&state, &headers)?;

    let config = state.config.load_full();

    // Registry identity. Absent for a configured device that has never
    // polled — that is a normal state (you can configure a device in Home
    // Assistant before it first checks in), not an error.
    let seen = state
        .registry
        .find_by_id(&crate::models::DeviceId::new(&key))
        .await?;
    let registration_code = seen.as_ref().map(|d| d.api_key.registration_code());

    // Same two-step lookup `list_devices` does, so the key Home Assistant
    // holds resolves the same way here as in the listing it came from.
    let device_config = config
        .get_device_config(&key)
        .or_else(|| {
            registration_code
                .as_deref()
                .and_then(|c| config.get_device_config_for_code(c))
        })
        // No device config means no configured screen, so there is nothing to
        // preview. An empty image would be indistinguishable from a screen
        // that renders blank.
        .ok_or(ApiError::NotFound)?;

    let model = seen
        .as_ref()
        .map(|d| d.model.clone())
        .unwrap_or_else(|| "og".to_string());

    // A panel profile may state the geometry; otherwise the model's default
    // applies, exactly as it does for an authoring render.
    let panel = device_config
        .panel
        .as_deref()
        .and_then(|name| config.get_panel(name));

    // View options. These change only how the preview is *drawn* — none of
    // them is written back to the device, so toggling one never alters what
    // the panel shows.
    let dithered = flag(query.dither.as_ref()).unwrap_or(true);
    let measured = flag(query.measured.as_ref()).unwrap_or(true);

    let opts = RenderOpts {
        model: model.clone(),
        width: panel.and_then(|p| p.width),
        height: panel.and_then(|p| p.height),
        panel: device_config.panel.clone(),
        params: device_config.params.clone(),
        device: Some(DevicePreview {
            mac: key.clone(),
            firmware_version: seen.as_ref().map(|d| d.firmware_version.clone()),
            battery_voltage: seen.as_ref().and_then(|d| d.battery_voltage),
            rssi: seen.as_ref().and_then(|d| d.rssi),
            colors: device_config.colors.clone(),
            dither: device_config.dither.clone(),
            tuning: DitherTuningValues {
                error_clamp: device_config.error_clamp,
                noise_scale: device_config.noise_scale,
                chroma_clamp: device_config.chroma_clamp,
                strength: device_config.strength,
                gamut: device_config.gamut.clone(),
            },
            refresh: device_config.refresh,
        }),
        // Undithered means the pre-dither, full-colour PNG, which the
        // renderer produces alongside the normal one.
        include_raw: !dithered,
        // `None` keeps byonk's own rule (measured when calibrated); only the
        // explicit "no" is passed down, since an explicit `true` cannot
        // conjure a calibration that is not there.
        use_actual: (!measured).then_some(false),
        // Deliberately not set: a device's configured dither belongs in the
        // device-config layer above, not in this override slot, or it would
        // beat a screen that picks its own algorithm.
        ..RenderOpts::default()
    };

    let fingerprint = fingerprint(device_config, &model, &opts);
    // The view options select between different images of the same
    // configuration, so they belong in the cache *key*, not the fingerprint.
    // Folding them into the fingerprint would leave one slot per device, and
    // flipping a toggle back and forth would re-render every time.
    let cache_key = format!("{key}#{}{}", u8::from(dithered), u8::from(measured));
    let forced = query.force.is_some();
    let now = chrono::Utc::now();

    if !forced {
        if let Some(png) = state.preview_cache.get(&cache_key, &fingerprint, now) {
            return Ok(png_response(png, "hit"));
        }
    }

    let screen_ref = device_config.screen.clone();
    let (width, height) = (opts.width, opts.height);
    let store = state.screen_store.clone();
    let pipeline = state.content_pipeline.clone();

    // Rendering is CPU-bound and synchronous (Lua, rasterizing, dithering):
    // off the async runtime, or one preview stalls every device poll.
    let rendered = tokio::task::spawn_blocking(move || {
        let result = store.render(&screen_ref, opts);
        if result.error.is_none() {
            // `raw_png` is `None` only if the undithered rasterization itself
            // failed, which `ScreenStore::render` swallows — fall through to
            // the error image rather than serve nothing.
            let chosen = if dithered {
                Some(result.png)
            } else {
                result.raw_png
            };
            if let Some(png) = chosen.filter(|p| !p.is_empty()) {
                return (png, result.refresh_rate);
            }
        }
        let message = match result.error.as_ref() {
            Some(e) => e.message.clone(),
            None => "undithered preview could not be rendered".to_string(),
        };
        // A failed render still owes the device page an image: the error the
        // panel itself would display. A broken-image icon would say only
        // that something went wrong, not what.
        let svg = pipeline.render_error_svg(&message);
        let (w, h) = resolve_preview_dimensions(&model, width, height);
        let spec = DisplaySpec::from_dimensions(w, h).unwrap_or(DisplaySpec::OG);
        let palette = resolve_query_palette(&model, None);
        let png = pipeline
            .render_png_from_svg(
                &svg, spec, &palette, None, false, None, None, None, &mut None,
            )
            .unwrap_or_default();
        // Refresh rate 0: the cache floors it, so a broken screen is retried
        // on a short cycle rather than pinned for the screen's own interval.
        (png, 0)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("preview render panicked: {e}")))?;

    let (png, refresh_rate) = rendered;
    if png.is_empty() {
        return Err(ApiError::Internal("preview produced no image".to_string()));
    }

    state
        .preview_cache
        .store(&cache_key, &fingerprint, png.clone(), refresh_rate, now);

    Ok(png_response(png, "miss"))
}

/// Hash everything the render depends on that this handler can see.
///
/// It cannot see the screen's *source*, so editing a screen's files does not
/// invalidate a preview — the TTL bounds how long that can be wrong, and the
/// refresh button ends it immediately.
///
/// Params are hashed through a `BTreeMap`: `DeviceConfig::params` is a
/// `HashMap`, whose iteration order varies per process, and a fingerprint
/// that varies per process is a cache that never hits.
fn fingerprint(device_config: &DeviceConfig, model: &str, opts: &RenderOpts) -> String {
    let params: BTreeMap<_, _> = device_config.params.iter().collect();
    let params_yaml = serde_yaml::to_string(&params).unwrap_or_default();

    let mut hasher = Sha256::new();
    hasher.update(device_config.screen.as_bytes());
    hasher.update([0]);
    hasher.update(params_yaml.as_bytes());
    hasher.update([0]);
    hasher.update(model.as_bytes());
    hasher.update([0]);
    hasher.update(format!("{:?}x{:?}", opts.width, opts.height).as_bytes());
    hasher.update([0]);
    hasher.update(format!("{:?}", opts.panel).as_bytes());
    hasher.update([0]);
    hasher.update(format!("{:?}", device_config.colors).as_bytes());
    hasher.update([0]);
    hasher.update(format!("{:?}", device_config.dither).as_bytes());
    hasher.update([0]);
    hasher.update(format!("{:?}", device_config.refresh).as_bytes());
    hasher.update([0]);
    hasher.update(
        format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}",
            device_config.error_clamp,
            device_config.noise_scale,
            device_config.chroma_clamp,
            device_config.strength,
            device_config.gamut,
        )
        .as_bytes(),
    );
    hex::encode(hasher.finalize())
}

fn png_response(png: Vec<u8>, cache_state: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CONTENT_LENGTH, png.len().to_string()),
            // The bytes are only valid until the next render; a browser or
            // proxy holding on to a frame would outlive it.
            (header::CACHE_CONTROL, "no-store".to_string()),
            (CACHE_HEADER.clone(), cache_state.to_string()),
        ],
        Bytes::from(png),
    )
        .into_response()
}
