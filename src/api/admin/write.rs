//! Admin write endpoints: device mappings + global settings.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::api::config_persist::{persist, require_file_config};
use crate::error::ApiError;
use crate::models::config::{
    normalize_algorithm_name, CascadeValues, GamutTuningValues, DITHER_ALGORITHMS,
    RESERVED_DEFAULT_KEY,
};
use crate::models::param_schema::validate_params;
use crate::server::AppState;
use crate::services::config_writer;
use crate::services::screen_repo_loader::BUILTIN_HANDLE;

use super::read::{build_screen_repo_info, ScreenRepoInfo};
use super::require_admin;

// `Serialize` + `Default` exist for the MCP mirror test: serialising a default
// value lists every field, so `ConfigureDeviceArgs` can be checked against the
// struct itself rather than against a hand-kept list. They change nothing at
// runtime (`ToSchema` would need a schema for every nested type, including
// `serde_yaml::Value`).
#[derive(Default, Deserialize, Serialize)]
pub struct DeviceWrite {
    pub key: Option<String>, // required for POST, ignored for PATCH (taken from URL)
    pub screen: Option<String>,
    pub panel: Option<String>,
    pub dither: Option<String>,
    pub colors: Option<String>,
    /// Measured colours for this one display, comma-separated hex. The base
    /// rung: it outranks the panel's calibration and is outranked by a screen
    /// that measures its own.
    pub colors_actual: Option<String>,
    /// Gamut mapping for continuous-tone regions. Base rung.
    ///
    /// Reachable only since the cascade work: it was in `DeviceConfig` but in
    /// neither `DeviceWrite` nor `MANAGED_DEVICE_KEYS`, so it could be
    /// hand-edited into config.yaml and nothing could set it. That is the
    /// silent-overwrite trap, and this field was the proof it bites.
    pub gamut: Option<GamutTuningValues>,
    /// The final rung -- the owner's veto. Merged **field by field**, like
    /// every other rung: setting `final.dither` must not wipe
    /// `final.strength`. A field is removed only through `clear`, which is
    /// why both `final.<field>` and `final` are clearable.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
    pub params: Option<HashMap<String, serde_yaml::Value>>,
    pub refresh: Option<u32>,
    pub name: Option<String>,

    // Dither tuning overrides. A device value wins over the panel's — the
    // render path resolves `dc_tuning.or(&panel_tuning)` — so removing one
    // hands the knob back to the panel block. That is what `clear` is for.
    pub max_error: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,

    // Panel behaviour, passed through to the device in `/api/display`.
    pub temperature_profile: Option<String>,
    pub maximum_compatibility: Option<bool>,
    pub min_png_bytes: Option<u32>,

    /// Settings to remove from the device entry, by name.
    ///
    /// A patch reads an absent field as "leave alone", which is what makes
    /// partial updates safe but also means a setting could be changed and
    /// never taken back. Naming it here deletes it, so the panel default (or
    /// the firmware's) applies again.
    ///
    /// Ignored on add: a device being created has nothing to clear, so a
    /// `clear` there is a mistake and is rejected rather than dropped.
    pub clear: Option<Vec<String>>,
}

/// Guard: global-config registry/settings writes are read-only when byonk runs
/// as an HA add-on. The add-on Options form (`/data/options.json`) is the sole
/// editor for global config; per-device writes and screen repo content-refresh are
/// unaffected.
fn require_writable_global(state: &AppState) -> Result<(), ApiError> {
    if state.addon_mode {
        return Err(ApiError::Conflict(
            "global config is read-only in add-on mode; edit it in the byonk add-on Configuration tab".into(),
        ));
    }
    Ok(())
}

/// In add-on mode, the genuinely-global settings are read-only (edited via the
/// add-on Options form). The operational `registration_enabled` toggle stays live.
fn require_writable_settings(state: &AppState, body: &SettingsWrite) -> Result<(), ApiError> {
    let touches_global = body.auth_mode.is_some() || body.screen_repo_refresh_interval.is_some();
    if state.addon_mode && touches_global {
        return Err(ApiError::Conflict(
            "global config is read-only in add-on mode; edit it in the byonk add-on Configuration tab".into(),
        ));
    }
    Ok(())
}

/// Validate the screen ref resolves to a screen repo screen and that the provided
/// params pass its `meta.yaml` schema. A device `screen` is always a qualified
/// `handle/path` screen repo ref — there is no bare-name / flat-file resolution.
fn validate_screen_and_params(
    state: &AppState,
    screen: &str,
    params: &HashMap<String, serde_yaml::Value>,
) -> Result<(), ApiError> {
    let resolved = state
        .screen_repo_manager
        .loader()
        .resolve(screen)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown screen `{screen}`")))?;
    if let Err(errs) = validate_params(&resolved.meta.params, params) {
        return Err(ApiError::BadRequest(errs.join("; ")));
    }
    Ok(())
}

/// Validate a dither algorithm name, reporting errors with a fully-qualified key name.
///
/// An unrecognised algorithm name does not fail anywhere downstream —
/// `svg_to_png`'s match ends in `_ => DitherAlgorithm::Atkinson` — so without
/// this check a typo silently rendered Atkinson, and the only symptom was on
/// the glass.
fn validate_dither_name(name: &str, key: &str) -> Result<(), ApiError> {
    let canonical = normalize_algorithm_name(name);
    if !DITHER_ALGORITHMS.contains(&canonical.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "unknown dither algorithm `{name}` in `{key}`; known: {}",
            DITHER_ALGORITHMS.join(", ")
        )));
    }
    Ok(())
}

/// Validate the device settings a caller actually provided.
///
/// Only the provided fields, never the merged result: a device whose
/// `config.yaml` entry was hand-edited into an invalid state must still accept
/// an unrelated patch, otherwise the API that exists to fix such a device is
/// the one thing that cannot.
///
/// This matters most for `dither`: an unrecognised algorithm name does not
/// fail anywhere downstream — `svg_to_png`'s match ends in `_ =>
/// DitherAlgorithm::Atkinson` — so before this check a typo silently rendered
/// Atkinson, and the only symptom was on the glass.
fn validate_device_settings(state: &AppState, w: &DeviceWrite) -> Result<(), ApiError> {
    if let Some(dither) = &w.dither {
        validate_dither_name(dither, "dither")?;
    }

    if let Some(panel) = &w.panel {
        let config = state.config.load();
        if !config.panels.contains_key(panel) {
            let mut known: Vec<&str> = config.panels.keys().map(|k| k.as_str()).collect();
            known.sort_unstable();
            return Err(ApiError::BadRequest(format!(
                "unknown panel `{panel}`; configured: {}",
                known.join(", ")
            )));
        }
    }

    if let Some(colors) = &w.colors {
        validate_colors(colors, "colors")?;
    }

    // `c` is rejected on purpose: firmware 1.8.14 still has its mapping
    // commented out, so a device reads `c` back as `default` and quietly turns
    // the extra clearing off again. Accepting it would store a setting that
    // does nothing.
    if let Some(profile) = &w.temperature_profile {
        if !matches!(profile.as_str(), "default" | "a" | "b") {
            return Err(ApiError::BadRequest(format!(
                "unknown temperature_profile `{profile}`; known: default, a, b"
            )));
        }
    }

    // Ranges are deliberately left open — these knobs exist to be swept, and a
    // plausible-looking bound would block the experiment that finds the real
    // one. Only values YAML can hold but arithmetic cannot are refused.
    for (key, value) in [
        ("max_error", w.max_error),
        ("noise_scale", w.noise_scale),
        ("chroma_clamp", w.chroma_clamp),
        ("strength", w.strength),
    ] {
        if let Some(v) = value {
            if !v.is_finite() || v < 0.0 {
                return Err(ApiError::BadRequest(format!(
                    "`{key}` must be a finite, non-negative number, got {v}"
                )));
            }
        }
    }

    if let Some(colors) = &w.colors_actual {
        validate_colors(colors, "colors_actual")?;
    }
    if let Some(g) = &w.gamut {
        validate_gamut(g, "gamut")?;
    }
    if let Some(f) = &w.final_rung {
        validate_cascade_values(f, "final.")?;
    }

    Ok(())
}

/// The same checks the base rung gets, applied to a rung written as a block.
/// `prefix` is how the caller names the fields (`final.`), so an error points
/// at the field the request actually contained.
fn validate_cascade_values(v: &CascadeValues, prefix: &str) -> Result<(), ApiError> {
    if let Some(dither) = &v.dither {
        validate_dither_name(dither, &format!("{prefix}dither"))?;
    }
    for (key, colors) in [("colors", &v.colors), ("colors_actual", &v.colors_actual)] {
        if let Some(c) = colors {
            validate_colors(c, &format!("{prefix}{key}"))?;
        }
    }
    for (key, value) in [
        ("max_error", v.max_error),
        ("noise_scale", v.noise_scale),
        ("chroma_clamp", v.chroma_clamp),
        ("strength", v.strength),
    ] {
        if let Some(n) = value {
            if !n.is_finite() || n < 0.0 {
                return Err(ApiError::BadRequest(format!(
                    "`{prefix}{key}` must be a finite, non-negative number, got {n}"
                )));
            }
        }
    }
    validate_gamut(&v.gamut, &format!("{prefix}gamut"))
}

fn validate_gamut(g: &GamutTuningValues, name: &str) -> Result<(), ApiError> {
    for (key, value) in [
        ("knee", g.knee),
        ("amount", g.amount),
        ("max_compression", g.max_compression),
    ] {
        if let Some(n) = value {
            if !n.is_finite() || n < 0.0 {
                return Err(ApiError::BadRequest(format!(
                    "`{name}.{key}` must be a finite, non-negative number, got {n}"
                )));
            }
        }
    }
    Ok(())
}

/// A `colors` override must be a comma-separated list of `#rrggbb` entries.
/// `key` is the fully-qualified name the caller used, so errors point at it.
///
/// Strict where the render path is forgiving: `parse_colors_header` builds the
/// palette with `filter_map`, so it drops the entries it cannot read and
/// carries on with a shorter palette. That is the right behaviour for a header
/// arriving from a device and the wrong one for a value being written to disk.
fn validate_colors(colors: &str, key: &str) -> Result<(), ApiError> {
    let entries: Vec<&str> = colors.split(',').map(str::trim).collect();
    for entry in &entries {
        let hex = entry.strip_prefix('#').unwrap_or(entry);
        let valid = hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit());
        if !valid {
            return Err(ApiError::BadRequest(format!(
                "`{key}` entry `{entry}` is not a #rrggbb color"
            )));
        }
    }
    if entries.len() < 2 {
        return Err(ApiError::BadRequest(format!(
            "`{key}` needs at least two colors to dither between"
        )));
    }
    Ok(())
}

/// Resolve the `clear` list into the set of keys to drop, rejecting names that
/// are not settings and names the same call also sets.
///
/// Setting and clearing one field in a single call has no sensible reading, so
/// it is refused rather than resolved by precedence — a caller that does it is
/// confused about what it wants, and either outcome would surprise it.
fn resolve_clear(w: &DeviceWrite) -> Result<Vec<String>, ApiError> {
    let Some(requested) = &w.clear else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for name in requested {
        let acceptable = CLEARABLE_DEVICE_KEYS.contains(&name.as_str())
            || name.strip_prefix("final.").is_some_and(is_cascade_field);
        if !acceptable {
            let hint = if name == "screen" {
                " (a device must always have a screen; assign a different one instead)"
            } else {
                ""
            };
            return Err(ApiError::BadRequest(format!(
                "cannot clear `{name}`{hint}; clearable: {}, or `final.<field>` for one of: {}",
                CLEARABLE_DEVICE_KEYS.join(", "),
                CASCADE_FIELDS.join(", ")
            )));
        }
        let also_set = match name.as_str() {
            "panel" => w.panel.is_some(),
            "dither" => w.dither.is_some(),
            "colors" => w.colors.is_some(),
            "refresh" => w.refresh.is_some(),
            "name" => w.name.is_some(),
            "params" => w.params.is_some(),
            "max_error" => w.max_error.is_some(),
            "noise_scale" => w.noise_scale.is_some(),
            "chroma_clamp" => w.chroma_clamp.is_some(),
            "strength" => w.strength.is_some(),
            "temperature_profile" => w.temperature_profile.is_some(),
            "maximum_compatibility" => w.maximum_compatibility.is_some(),
            "min_png_bytes" => w.min_png_bytes.is_some(),
            "colors_actual" => w.colors_actual.is_some(),
            "gamut" => w.gamut.is_some(),
            "final" => w.final_rung.is_some(),
            dotted if dotted.starts_with("final.") => w
                .final_rung
                .as_ref()
                .is_some_and(|f| cascade_field_is_set(f, &dotted["final.".len()..])),
            _ => false,
        };
        if also_set {
            return Err(ApiError::BadRequest(format!(
                "`{name}` is both set and cleared in the same request"
            )));
        }
        out.push(name.clone());
    }
    Ok(out)
}

/// The field names a `final.<name>` clear may address -- `CascadeValues`'
/// own fields, and nothing else. `final: {screen: ...}` is a config error at
/// startup; `clear: ["final.screen"]` must be an API error for the same
/// reason, named rather than ignored.
const CASCADE_FIELDS: [&str; 9] = [
    "colors",
    "colors_actual",
    "dither",
    "max_error",
    "noise_scale",
    "chroma_clamp",
    "strength",
    "refresh",
    "gamut",
];

fn is_cascade_field(name: &str) -> bool {
    CASCADE_FIELDS.contains(&name)
}

fn cascade_field_is_set(v: &CascadeValues, field: &str) -> bool {
    match field {
        "colors" => v.colors.is_some(),
        "colors_actual" => v.colors_actual.is_some(),
        "dither" => v.dither.is_some(),
        "max_error" => v.max_error.is_some(),
        "noise_scale" => v.noise_scale.is_some(),
        "chroma_clamp" => v.chroma_clamp.is_some(),
        "strength" => v.strength.is_some(),
        "refresh" => v.refresh.is_some(),
        "gamut" => !v.gamut.is_empty(),
        _ => false,
    }
}

fn clear_cascade_field(v: &mut CascadeValues, field: &str) {
    match field {
        "colors" => v.colors = None,
        "colors_actual" => v.colors_actual = None,
        "dither" => v.dither = None,
        "max_error" => v.max_error = None,
        "noise_scale" => v.noise_scale = None,
        "chroma_clamp" => v.chroma_clamp = None,
        "strength" => v.strength = None,
        "refresh" => v.refresh = None,
        "gamut" => v.gamut = GamutTuningValues::default(),
        // Unreachable: `resolve_clear` rejects anything else.
        _ => {}
    }
}

/// Drop the named settings from an already-merged write.
///
/// A bare name addresses the base rung only; `final.<field>` one field of the
/// final rung and `final` the whole rung.
///
/// Applied after the merge, so a cleared field is not carried over from the
/// existing entry; `device_block` then omits it, and `unmanaged_device_keys`
/// cannot put it back because every clearable key is managed.
fn apply_clear(merged: &mut DeviceWrite, clear: &[String]) {
    for name in clear {
        match name.as_str() {
            "panel" => merged.panel = None,
            "dither" => merged.dither = None,
            "colors" => merged.colors = None,
            "refresh" => merged.refresh = None,
            "name" => merged.name = None,
            "params" => merged.params = None,
            "max_error" => merged.max_error = None,
            "noise_scale" => merged.noise_scale = None,
            "chroma_clamp" => merged.chroma_clamp = None,
            "strength" => merged.strength = None,
            "temperature_profile" => merged.temperature_profile = None,
            "maximum_compatibility" => merged.maximum_compatibility = None,
            "min_png_bytes" => merged.min_png_bytes = None,
            "colors_actual" => merged.colors_actual = None,
            "gamut" => merged.gamut = None,
            // The whole rung, as one key.
            "final" => merged.final_rung = None,
            dotted if dotted.starts_with("final.") => {
                if let Some(f) = merged.final_rung.as_mut() {
                    clear_cascade_field(f, &dotted["final.".len()..]);
                }
                // A rung whose every field was cleared emits nothing anyway
                // (`device_block`), so there is no empty-rung case to handle.
            }
            // Unreachable: `resolve_clear` rejects anything else.
            _ => {}
        }
    }
}

/// Build the YAML mapping for a device block from the provided fields.
/// The device keys `device_block` writes for itself. Anything else found in an
/// existing entry is carried across verbatim by [`unmanaged_device_keys`].
///
/// Kept as a list rather than derived from `DeviceWrite`'s fields because the
/// mapping is not one-to-one: `key` is the entry name, not a key inside it.
const MANAGED_DEVICE_KEYS: [&str; 17] = [
    "screen",
    "panel",
    "dither",
    "colors",
    "refresh",
    "name",
    "params",
    "max_error",
    "noise_scale",
    "chroma_clamp",
    "strength",
    "temperature_profile",
    "maximum_compatibility",
    "min_png_bytes",
    "colors_actual",
    "gamut",
    "final",
];

/// The settings `clear` accepts: every managed key except `screen`, which a
/// device must always have. `error_clamp` is deliberately absent — it is not
/// managed, so it is preserved verbatim and keeps reporting its deprecation.
///
/// A test asserts this stays exactly `MANAGED_DEVICE_KEYS` minus `screen`, so
/// a field added to one list cannot go missing from the other.
const CLEARABLE_DEVICE_KEYS: [&str; 16] = [
    "panel",
    "dither",
    "colors",
    "refresh",
    "name",
    "params",
    "max_error",
    "noise_scale",
    "chroma_clamp",
    "strength",
    "temperature_profile",
    "maximum_compatibility",
    "min_png_bytes",
    "colors_actual",
    "gamut",
    "final",
];

/// Every key of an existing device entry that this writer does not model.
///
/// `upsert_device` replaces the whole block, so without this a patch deletes
/// every setting `DeviceWrite` happens not to carry — `temperature_profile`,
/// `maximum_compatibility`, `min_png_bytes` and the dither tuning knobs among
/// them. That failure is silent and only shows up on the glass hours later,
/// which is exactly how it was found. Preserving by exclusion rather than by
/// enumeration means a field added to `DeviceConfig` tomorrow survives without
/// anyone remembering to come back here.
fn unmanaged_device_keys(yaml: &str, key: &str) -> serde_yaml::Mapping {
    let mut out = serde_yaml::Mapping::new();
    let Ok(root) = serde_yaml::from_str::<serde_yaml::Value>(yaml) else {
        return out;
    };
    let Some(block) = root
        .get("devices")
        .and_then(|d| d.get(key))
        .and_then(|d| d.as_mapping())
    else {
        return out;
    };
    for (k, v) in block {
        let managed = k.as_str().is_some_and(|s| MANAGED_DEVICE_KEYS.contains(&s));
        if !managed {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Write an `f32` tuning value as the decimal the caller actually sent.
///
/// `serde_yaml::Value::from(f32)` widens to `f64` first, and the `f64` nearest
/// the `f32` nearest `0.9` is `0.8999999761581421` — which is what lands in
/// `config.yaml`. It reads back as the same `f32`, so nothing renders
/// differently, but the file becomes unreadable and every rewrite churns the
/// diff. `f32::to_string` gives the shortest decimal that round-trips, so
/// parsing that as `f64` keeps `0.9` looking like `0.9`.
fn yaml_f32(v: f32) -> serde_yaml::Value {
    v.to_string()
        .parse::<f64>()
        .map(serde_yaml::Value::from)
        .unwrap_or_else(|_| serde_yaml::Value::from(v))
}

/// One rung's settings as a YAML mapping. Shared by the `final:` block (and
/// by anything else that writes a rung) so `yaml_f32` applies at every rung --
/// a float written one way at one rung and another way at the other churns
/// the diff on every rewrite.
fn cascade_mapping(v: &CascadeValues) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    if let Some(c) = &v.colors {
        m.insert("colors".into(), c.as_str().into());
    }
    if let Some(c) = &v.colors_actual {
        m.insert("colors_actual".into(), c.as_str().into());
    }
    if let Some(d) = &v.dither {
        m.insert("dither".into(), d.as_str().into());
    }
    for (key, value) in [
        ("max_error", v.max_error),
        ("noise_scale", v.noise_scale),
        ("chroma_clamp", v.chroma_clamp),
        ("strength", v.strength),
    ] {
        if let Some(n) = value {
            m.insert(key.into(), yaml_f32(n));
        }
    }
    if let Some(r) = v.refresh.filter(|&r| r > 0) {
        m.insert("refresh".into(), serde_yaml::Value::from(r));
    }
    if let Some(g) = gamut_mapping(&v.gamut) {
        m.insert("gamut".into(), g.into());
    }
    m
}

/// A `GamutTuningValues` as a mapping, or `None` when it sets nothing -- an
/// empty `gamut: {}` in the file would be noise.
fn gamut_mapping(g: &GamutTuningValues) -> Option<serde_yaml::Mapping> {
    if g.is_empty() {
        return None;
    }
    let mut m = serde_yaml::Mapping::new();
    for (key, value) in [
        ("knee", g.knee),
        ("amount", g.amount),
        ("max_compression", g.max_compression),
    ] {
        if let Some(n) = value {
            m.insert(key.into(), yaml_f32(n));
        }
    }
    Some(m)
}

/// Merge the patch's final rung onto the existing one, field by field.
///
/// `body.or(existing)` on the whole `Option` would be whole-block
/// replacement: a patch setting `final.dither` would silently drop
/// `final.strength`. Returns `None` when neither side has a rung, so
/// `device_block` emits no `final:` key at all.
fn merge_final(
    body: Option<&CascadeValues>,
    existing: Option<&CascadeValues>,
) -> Option<CascadeValues> {
    match (body, existing) {
        (None, None) => None,
        (Some(b), None) => Some(b.clone()),
        (None, Some(e)) => Some(e.clone()),
        (Some(b), Some(e)) => Some(CascadeValues {
            colors: b.colors.clone().or_else(|| e.colors.clone()),
            colors_actual: b.colors_actual.clone().or_else(|| e.colors_actual.clone()),
            dither: b.dither.clone().or_else(|| e.dither.clone()),
            max_error: b.max_error.or(e.max_error),
            noise_scale: b.noise_scale.or(e.noise_scale),
            chroma_clamp: b.chroma_clamp.or(e.chroma_clamp),
            strength: b.strength.or(e.strength),
            refresh: b.refresh.or(e.refresh),
            gamut: b.gamut.or(&e.gamut),
        }),
    }
}

fn device_block(w: &DeviceWrite, screen: &str) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    m.insert("screen".into(), screen.into());
    if let Some(p) = &w.panel {
        m.insert("panel".into(), p.as_str().into());
    }
    if let Some(d) = &w.dither {
        m.insert("dither".into(), d.as_str().into());
    }
    if let Some(c) = &w.colors {
        m.insert("colors".into(), c.as_str().into());
    }
    if let Some(r) = w.refresh {
        if r > 0 {
            m.insert("refresh".into(), serde_yaml::Value::from(r));
        }
    }
    if let Some(n) = &w.name {
        if !n.is_empty() {
            m.insert("name".into(), n.as_str().into());
        }
    }
    for (key, value) in [
        ("max_error", w.max_error),
        ("noise_scale", w.noise_scale),
        ("chroma_clamp", w.chroma_clamp),
        ("strength", w.strength),
    ] {
        if let Some(v) = value {
            m.insert(key.into(), yaml_f32(v));
        }
    }
    if let Some(c) = &w.colors_actual {
        m.insert("colors_actual".into(), c.as_str().into());
    }
    if let Some(g) = w.gamut.as_ref().and_then(gamut_mapping) {
        m.insert("gamut".into(), g.into());
    }
    // An empty final rung emits nothing: `final: {}` in the file would read
    // as a veto that vetoes nothing.
    if let Some(f) = &w.final_rung {
        let mapping = cascade_mapping(f);
        if !mapping.is_empty() {
            m.insert("final".into(), mapping.into());
        }
    }
    if let Some(t) = &w.temperature_profile {
        m.insert("temperature_profile".into(), t.as_str().into());
    }
    if let Some(c) = w.maximum_compatibility {
        m.insert("maximum_compatibility".into(), serde_yaml::Value::from(c));
    }
    if let Some(b) = w.min_png_bytes {
        m.insert("min_png_bytes".into(), serde_yaml::Value::from(b));
    }
    if let Some(params) = &w.params {
        let mut pm = serde_yaml::Mapping::new();
        for (k, v) in params {
            pm.insert(k.as_str().into(), v.clone());
        }
        m.insert("params".into(), serde_yaml::Value::Mapping(pm));
    }
    m
}

/// Apply a device *add*: reject a duplicate key, validate the screen and
/// params, and persist. Shared by `POST /api/admin/devices` and the MCP
/// `configure_device` tool's create-on-first-configuration fallback (a device that
/// has only been *seen* by the registry — i.e. it shows up in `list_devices`
/// — has no `config.devices` entry yet, so `apply_device_patch` 404s on it;
/// `configure_device` falls back to this to create the mapping).
pub async fn apply_device_add(
    state: &AppState,
    key: &str,
    body: DeviceWrite,
) -> Result<serde_json::Value, ApiError> {
    let path = require_file_config(state)?;
    let _guard = state.write_lock.lock().await;
    let screen = body
        .screen
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`screen` is required".into()))?;

    // Reject duplicate.
    if state.config.load().devices.contains_key(key) {
        return Err(ApiError::Conflict(format!("device `{key}` already exists")));
    }

    // Any presence, not just a non-empty list: a caller that sends `clear` to
    // a create has misunderstood what it is doing, and accepting the empty
    // case would leave the endpoint with two contracts to explain.
    if body.clear.is_some() {
        return Err(ApiError::BadRequest(
            "`clear` is not valid when creating a device — it has no settings yet".into(),
        ));
    }

    let empty = HashMap::new();
    validate_screen_and_params(state, &screen, body.params.as_ref().unwrap_or(&empty))?;
    validate_device_settings(state, &body)?;

    let block = device_block(&body, &screen);
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_device(&yaml, key, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(state, &path, new_yaml)?;

    Ok(serde_json::json!({ "key": key, "screen": screen }))
}

pub async fn add_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    // Restore the pre-extraction ordering: an embedded/read-only config must
    // still 409 even when the body is missing `key`, exactly as before this
    // was split into a shared core (which re-checks this internally, but
    // only after the key is already resolved).
    require_file_config(&state)?;
    let key = body
        .key
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`key` is required".into()))?;
    Ok(Json(apply_device_add(&state, &key, body).await?))
}

/// Apply a device patch: merge with the existing entry, validate the screen
/// and params, and persist. Shared by `PATCH /api/admin/devices/{key}` and
/// the MCP `configure_device` tool so both enforce identical rules — screen
/// existence, param schema, the config write lock, and rollback on a failed
/// reload.
///
/// Deliberately does NOT call `require_writable_global`: a device mapping is
/// not global config, so it stays writable in add-on mode.
///
/// Note: when `body.screen` changes but `body.params` is `None`, the
/// device's *existing* params carry over unchanged (validated against the
/// new screen's schema) — they are NOT reset to the new screen's defaults.
pub async fn apply_device_patch(
    state: &AppState,
    key: &str,
    body: DeviceWrite,
) -> Result<serde_json::Value, ApiError> {
    let path = require_file_config(state)?;
    validate_device_settings(state, &body)?;
    let clear = resolve_clear(&body)?;
    let _guard = state.write_lock.lock().await;

    // Must already exist.
    let existing = {
        let config = state.config.load();
        config.devices.get(key).cloned().ok_or(ApiError::NotFound)?
    };

    // Merge: start from existing, override provided fields.
    let screen = body.screen.clone().unwrap_or(existing.screen.clone());

    // Params: a screen change replaces params wholesale with whatever the
    // caller provided (or, if the caller provided none, the previous
    // screen's params carry over unchanged — NOT the new screen's defaults;
    // there is no meta.yaml default lookup here). Without a screen change,
    // provided params are merged key-by-key into the existing set, so
    // editing one parameter never drops the others.
    let params = if body.screen.is_none() {
        match &body.params {
            Some(p) => {
                let mut merged = existing.params.clone();
                for (k, v) in p {
                    merged.insert(k.clone(), v.clone());
                }
                merged
            }
            None => existing.params.clone(),
        }
    } else {
        body.params
            .clone()
            .unwrap_or_else(|| existing.params.clone())
    };

    // Every managed key needs a line here. A key listed in
    // `MANAGED_DEVICE_KEYS` but missing from this merge is *deleted* by any
    // patch that omits it — `unmanaged_device_keys` no longer protects it —
    // and the loss is silent until it shows up on the glass.
    let mut merged = DeviceWrite {
        key: Some(key.to_string()),
        screen: Some(screen.clone()),
        panel: body.panel.clone().or(existing.panel.clone()),
        dither: body.dither.clone().or(existing.dither.clone()),
        colors: body.colors.clone().or(existing.colors.clone()),
        colors_actual: body
            .colors_actual
            .clone()
            .or(existing.colors_actual.clone()),
        gamut: Some(body.gamut.clone().unwrap_or_default().or(&existing.gamut)),
        final_rung: merge_final(body.final_rung.as_ref(), existing.final_rung.as_ref()),
        params: Some(params),
        refresh: body.refresh.or(existing.refresh),
        name: body.name.clone().or(existing.name.clone()),
        max_error: body.max_error.or(existing.max_error),
        noise_scale: body.noise_scale.or(existing.noise_scale),
        chroma_clamp: body.chroma_clamp.or(existing.chroma_clamp),
        strength: body.strength.or(existing.strength),
        temperature_profile: body
            .temperature_profile
            .clone()
            .or(existing.temperature_profile.clone()),
        maximum_compatibility: body
            .maximum_compatibility
            .or(existing.maximum_compatibility),
        min_png_bytes: body.min_png_bytes.or(existing.min_png_bytes),
        clear: None,
    };
    apply_clear(&mut merged, &clear);

    let empty = HashMap::new();
    validate_screen_and_params(state, &screen, merged.params.as_ref().unwrap_or(&empty))?;

    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut block = device_block(&merged, &screen);
    // Carry across everything this writer does not model, so a screen
    // assignment cannot quietly strip the device's other settings.
    for (k, v) in unmanaged_device_keys(&yaml, key) {
        block.insert(k, v);
    }
    let new_yaml = config_writer::upsert_device(&yaml, key, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(state, &path, new_yaml)?;

    Ok(serde_json::json!({ "key": key, "screen": screen }))
}

pub async fn patch_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    Ok(Json(apply_device_patch(&state, &key, body).await?))
}

pub async fn delete_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    if key == RESERVED_DEFAULT_KEY {
        return Err(ApiError::Conflict(
            "the reserved DEFAULT device cannot be deleted".into(),
        ));
    }
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = match config_writer::remove_device(&yaml, &key) {
        Ok(y) => y,
        Err(config_writer::ConfigWriteError::NotFound(_)) => return Err(ApiError::NotFound),
        Err(e) => return Err(ApiError::Internal(e.to_string())),
    };
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "deleted": key })))
}

#[derive(Deserialize)]
pub struct SettingsWrite {
    pub(crate) registration_enabled: Option<bool>,
    pub(crate) auth_mode: Option<String>,
    pub(crate) screen_repo_refresh_interval: Option<u64>,
}

pub async fn patch_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SettingsWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    require_writable_settings(&state, &body)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    // 1. Validate all provided fields before touching yaml.
    if let Some(mode) = &body.auth_mode {
        if mode != "api_key" && mode != "ed25519" {
            return Err(ApiError::BadRequest(
                "auth_mode must be api_key or ed25519".into(),
            ));
        }
    }
    // 2. Apply all mutations.
    let mut yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if let Some(enabled) = body.registration_enabled {
        yaml = config_writer::set_scalar(&yaml, &["registration", "enabled"], enabled.into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(mode) = &body.auth_mode {
        yaml = config_writer::set_scalar(&yaml, &["auth_mode"], mode.as_str().into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(secs) = body.screen_repo_refresh_interval {
        yaml = config_writer::set_scalar(&yaml, &["screen_repo_refresh_interval"], secs.into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // 3. Persist.
    persist(&state, &path, yaml)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ScreenRepoWrite {
    pub handle: Option<String>,
    pub repo: Option<String>,
    pub pin: Option<String>,
    pub token: Option<String>,
}

/// Number of screens currently resolved under `handle` (0 before the first
/// successful fetch, since nothing is registered in the loader yet).
fn screen_repo_screen_count(state: &AppState, handle: &str) -> usize {
    state
        .screen_repo_manager
        .loader()
        .list_all()
        .into_iter()
        .filter(|s| s.handle == handle)
        .count()
}

/// Build a `serde_yaml::Mapping` screen repo block from the given fields, omitting
/// any that are `None` (so e.g. an absent `token` is never written as `null`).
fn screen_repo_block(
    repo: Option<&str>,
    pin: Option<&str>,
    token: Option<&str>,
) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    if let Some(r) = repo {
        m.insert("repo".into(), r.into());
    }
    if let Some(p) = pin {
        m.insert("pin".into(), p.into());
    }
    if let Some(t) = token {
        m.insert("token".into(), t.into());
    }
    m
}

pub async fn add_screen_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ScreenRepoWrite>,
) -> Result<Json<ScreenRepoInfo>, ApiError> {
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let handle = body
        .handle
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`handle` is required".into()))?;

    if handle == BUILTIN_HANDLE {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is the reserved builtin screen repo handle"
        )));
    }
    if state.config.load().screen_repos.contains_key(&handle) {
        return Err(ApiError::Conflict(format!(
            "screen repo `{handle}` already exists"
        )));
    }

    let block = screen_repo_block(
        body.repo.as_deref(),
        body.pin.as_deref(),
        body.token.as_deref(),
    );
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_screen_repo(&yaml, &handle, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    // Fetch asynchronously; the response reflects whatever status exists at
    // this instant (likely no entry yet). The client polls GET /screen-repos for
    // the settled result rather than this handler awaiting the fetch.
    let mgr = state.screen_repo_manager.clone();
    let h = handle.clone();
    tokio::task::spawn_blocking(move || mgr.refresh_one(&h));

    let count = screen_repo_screen_count(&state, &handle);
    let statuses = state.screen_repo_manager.status_snapshot();
    let info = build_screen_repo_info(&state.config.load(), statuses.get(&handle), count, handle);
    Ok(Json(info))
}

pub async fn patch_screen_repo(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ScreenRepoWrite>,
) -> Result<Json<ScreenRepoInfo>, ApiError> {
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    if handle == BUILTIN_HANDLE {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is the reserved builtin screen repo handle"
        )));
    }

    let existing = state
        .config
        .load()
        .screen_repos
        .get(&handle)
        .cloned()
        .ok_or(ApiError::NotFound)?;

    // Overlay only the provided fields; an omitted field (incl. `token`)
    // keeps its existing value.
    let repo = body.repo.clone().or_else(|| existing.repo.clone());
    let pin = body.pin.clone().or_else(|| existing.pin.clone());
    let token = body.token.clone().or_else(|| existing.token.clone());
    let repo_or_pin_changed = (body.repo.is_some() && body.repo != existing.repo)
        || (body.pin.is_some() && body.pin != existing.pin);

    let block = screen_repo_block(repo.as_deref(), pin.as_deref(), token.as_deref());
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_screen_repo(&yaml, &handle, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    if repo_or_pin_changed {
        let mgr = state.screen_repo_manager.clone();
        let h = handle.clone();
        tokio::task::spawn_blocking(move || mgr.refresh_one(&h));
    }

    let count = screen_repo_screen_count(&state, &handle);
    let statuses = state.screen_repo_manager.status_snapshot();
    let info = build_screen_repo_info(&state.config.load(), statuses.get(&handle), count, handle);
    Ok(Json(info))
}

pub async fn delete_screen_repo(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_writable_global(&state)?;
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    if handle == BUILTIN_HANDLE {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is the reserved builtin screen repo handle"
        )));
    }

    // Reject if any device's screen dangles into this screen repo's namespace.
    let prefix = format!("{handle}/");
    let config = state.config.load();
    if let Some((device_key, _)) = config
        .devices
        .iter()
        .find(|(_, d)| d.screen.starts_with(&prefix))
        .map(|(k, d)| (k.clone(), d.screen.clone()))
    {
        return Err(ApiError::Conflict(format!(
            "screen repo `{handle}` is referenced by device `{device_key}`"
        )));
    }
    drop(config);

    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = match config_writer::remove_screen_repo(&yaml, &handle) {
        Ok(y) => y,
        Err(config_writer::ConfigWriteError::NotFound(_)) => return Err(ApiError::NotFound),
        Err(e) => return Err(ApiError::Internal(e.to_string())),
    };
    persist(&state, &path, new_yaml)?;

    // Forget the deleted handle's fetch status so a later re-registration of
    // the same handle doesn't briefly surface the stale resolved_sha/Ready
    // state left over from the deleted screen repo.
    state.screen_repo_manager.forget_status(&handle);
    // Rebuild the hot-swapped loader so the deleted handle's screens stop
    // resolving immediately (in-memory only; not blocking).
    state.screen_repo_manager.rebuild_loader();

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_screen_repo(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ScreenRepoInfo>, ApiError> {
    require_admin(&state, &headers)?;

    if handle != BUILTIN_HANDLE && !state.config.load().screen_repos.contains_key(&handle) {
        return Err(ApiError::NotFound);
    }

    let mgr = state.screen_repo_manager.clone();
    let h = handle.clone();
    tokio::task::spawn_blocking(move || mgr.refresh_one(&h));

    let count = screen_repo_screen_count(&state, &handle);
    let statuses = state.screen_repo_manager.status_snapshot();
    let info = build_screen_repo_info(&state.config.load(), statuses.get(&handle), count, handle);
    Ok(Json(info))
}

pub async fn update_all_screen_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;

    let mgr = state.screen_repo_manager.clone();
    tokio::task::spawn_blocking(move || mgr.refresh_all(true));

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetLoader;
    use crate::models::AppConfig;
    use crate::server::create_app_state_with_config;

    fn state_with_addon_mode(addon_mode: bool) -> AppState {
        let loader = AssetLoader::new(None, None, None);
        let config = AppConfig::load_from_assets(&loader).expect("load embedded config");
        let mut state = create_app_state_with_config(std::sync::Arc::new(loader), config)
            .expect("create app state");
        state.addon_mode = addon_mode;
        state
    }

    const DEVICE_YAML: &str = "\
devices:
  \"1C:DB:D4:66:5B:50\":
    screen: local/gradient-lab
    panel: trmnl_x
    dither: floyd-steinberg
    temperature_profile: a
    maximum_compatibility: true
    min_png_bytes: 102401
    max_error: 0.8
    error_clamp: 0.2
    gamut:
      knee: 0.7
    params:
      station: Olten
";

    /// The bug this guards against cost a live panel its anti-ghosting
    /// settings: `upsert_device` replaces the whole block, so assigning a
    /// screen silently deleted every key `DeviceWrite` does not carry. It
    /// failed on the glass, not in a log.
    ///
    /// `temperature_profile`, `min_png_bytes` and the dither knobs used to be
    /// guarded here. They are managed fields now, kept by the patch merge
    /// instead — `test_patch_preserves_every_tuning_knob` in
    /// `tests/admin_write_test.rs` is what proves they still survive, end to
    /// end. What is left for this door is the deprecated key: `error_clamp`,
    /// whose whole job is to stay on disk and keep reporting itself.
    #[test]
    fn unmanaged_device_keys_are_preserved() {
        let kept = unmanaged_device_keys(DEVICE_YAML, "1C:DB:D4:66:5B:50");
        assert!(
            kept.contains_key(serde_yaml::Value::from("error_clamp")),
            "error_clamp must survive a device patch, kept: {kept:?}"
        );
        assert_eq!(
            kept.get(serde_yaml::Value::from("error_clamp")),
            Some(&serde_yaml::Value::from(0.2)),
            "value must carry across unchanged, not just the key"
        );
    }

    /// `clear` and the managed-key list are two halves of one contract: a key
    /// the writer sets must also be a key the caller can unset, or a setting
    /// becomes a one-way door. `screen` is the sole exception, because a
    /// device without one has nothing to show.
    #[test]
    fn every_managed_key_except_screen_is_clearable() {
        let mut expected: Vec<&str> = MANAGED_DEVICE_KEYS
            .iter()
            .copied()
            .filter(|k| *k != "screen")
            .collect();
        let mut actual: Vec<&str> = CLEARABLE_DEVICE_KEYS.to_vec();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(
            expected, actual,
            "CLEARABLE_DEVICE_KEYS must be MANAGED_DEVICE_KEYS minus `screen`"
        );
    }

    /// Keys the writer sets itself must NOT come back through the side door,
    /// or a patch clearing `name` or `refresh` would be undone by its own
    /// preservation step.
    #[test]
    fn managed_device_keys_are_not_preserved() {
        let kept = unmanaged_device_keys(DEVICE_YAML, "1C:DB:D4:66:5B:50");
        for key in MANAGED_DEVICE_KEYS {
            assert!(
                !kept.contains_key(serde_yaml::Value::from(key)),
                "{key} is written by device_block and must not be carried over"
            );
        }
    }

    fn device_write_for_test() -> DeviceWrite {
        DeviceWrite {
            key: None,
            screen: Some("local/a".into()),
            panel: None,
            dither: None,
            colors: None,
            colors_actual: None,
            gamut: None,
            final_rung: None,
            params: None,
            refresh: None,
            name: None,
            max_error: None,
            noise_scale: None,
            chroma_clamp: None,
            strength: None,
            temperature_profile: None,
            maximum_compatibility: None,
            min_png_bytes: None,
            clear: None,
        }
    }

    fn with_clear(mut w: DeviceWrite, names: &[&str]) -> DeviceWrite {
        w.clear = Some(names.iter().map(|n| n.to_string()).collect());
        w
    }

    /// The failure is silent: the response is 200 and the config keeps the
    /// OLD final block.
    #[test]
    fn a_patch_setting_final_survives_unmanaged_device_keys() {
        let yaml = "devices:\n  AA:\n    screen: local/a\n    final:\n      dither: stucki\n";
        let kept = unmanaged_device_keys(yaml, "AA");
        assert!(
            !kept.contains_key(serde_yaml::Value::from("final")),
            "`final` must be MANAGED, or every patch that sets it is silently \
             overwritten by the block already in the file"
        );
    }

    /// Field by field, like every other rung: setting `final.dither` must not
    /// wipe `final.strength`.
    #[test]
    fn setting_one_final_field_keeps_the_others() {
        let existing = CascadeValues {
            dither: Some("stucki".into()),
            strength: Some(0.4),
            ..Default::default()
        };
        let body = CascadeValues {
            dither: Some("atkinson".into()),
            ..Default::default()
        };
        let m = merge_final(Some(&body), Some(&existing)).expect("a final rung survives");
        assert_eq!(
            m.dither.as_deref(),
            Some("atkinson"),
            "the patch's field wins"
        );
        assert_eq!(m.strength, Some(0.4), "the untouched field must survive");
    }

    /// A bare name clears the BASE rung only. Clearing both would be
    /// indistinguishable from success in the response.
    #[test]
    fn a_bare_clear_name_clears_the_base_rung_only() {
        let mut merged = DeviceWrite {
            dither: Some("sierra".into()),
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        apply_clear(&mut merged, &["dither".to_string()]);
        assert_eq!(merged.dither, None, "the base rung is cleared");
        assert_eq!(
            merged.final_rung.as_ref().and_then(|f| f.dither.as_deref()),
            Some("atkinson"),
            "`clear: [\"dither\"]` must never reach the final rung"
        );
    }

    #[test]
    fn a_dotted_clear_name_clears_that_final_field_only() {
        let mut merged = DeviceWrite {
            dither: Some("sierra".into()),
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                strength: Some(0.4),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        apply_clear(&mut merged, &["final.dither".to_string()]);
        assert_eq!(
            merged.dither.as_deref(),
            Some("sierra"),
            "the base rung stands"
        );
        let f = merged
            .final_rung
            .as_ref()
            .expect("the rung itself survives");
        assert_eq!(f.dither, None);
        assert_eq!(f.strength, Some(0.4), "its other fields survive");
    }

    #[test]
    fn clearing_final_removes_the_whole_rung() {
        let mut merged = DeviceWrite {
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                strength: Some(0.4),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        apply_clear(&mut merged, &["final".to_string()]);
        assert!(merged.final_rung.is_none());
    }

    #[test]
    fn clearing_the_new_base_keys_removes_them() {
        let mut merged = DeviceWrite {
            colors_actual: Some("#000000,#FFFFFF".into()),
            gamut: Some(GamutTuningValues {
                knee: Some(0.8),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        apply_clear(
            &mut merged,
            &["colors_actual".to_string(), "gamut".to_string()],
        );
        assert!(merged.colors_actual.is_none());
        assert!(merged.gamut.is_none());
    }

    #[test]
    fn an_unknown_dotted_clear_name_is_rejected_by_name() {
        for bad in ["final.screen", "final.nonsense", "base.dither"] {
            let err = resolve_clear(&with_clear(device_write_for_test(), &[bad]))
                .expect_err("not a cascade field");
            assert!(format!("{err:?}").contains(bad), "{bad}: {err:?}");
        }
    }

    /// Setting and clearing the same thing in one request is a caller mistake
    /// at every rung, not just the base one.
    #[test]
    fn a_final_field_cannot_be_set_and_cleared_in_one_request() {
        let body = DeviceWrite {
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        resolve_clear(&with_clear(body, &["final.dither"]))
            .expect_err("set and cleared in the same request");
    }

    /// Clearing one final field while setting a different one is fine -- that
    /// is the whole point of field-by-field addressing.
    #[test]
    fn a_final_field_may_be_cleared_while_another_is_set() {
        let body = DeviceWrite {
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        resolve_clear(&with_clear(body, &["final.strength"])).expect("different fields");
    }

    #[test]
    fn device_block_emits_the_final_mapping_and_the_two_new_base_keys() {
        let w = DeviceWrite {
            colors_actual: Some("#1C5240,#FFFFFF".into()),
            gamut: Some(GamutTuningValues {
                knee: Some(0.8),
                ..Default::default()
            }),
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        let block = device_block(&w, "local/a");
        assert_eq!(
            block
                .get(serde_yaml::Value::from("colors_actual"))
                .and_then(|v| v.as_str()),
            Some("#1C5240,#FFFFFF")
        );
        let gamut = block
            .get(serde_yaml::Value::from("gamut"))
            .expect("gamut is emitted");
        assert!(gamut.as_mapping().is_some(), "gamut is a nested mapping");
        let f = block
            .get(serde_yaml::Value::from("final"))
            .expect("final is emitted");
        assert_eq!(
            f.as_mapping()
                .and_then(|m| m.get(serde_yaml::Value::from("dither")))
                .and_then(|v| v.as_str()),
            Some("atkinson")
        );
        // A final rung that sets nothing must not leave `final: {}` behind.
        let empty = DeviceWrite {
            final_rung: Some(CascadeValues::default()),
            ..device_write_for_test()
        };
        assert!(device_block(&empty, "local/a")
            .get(serde_yaml::Value::from("final"))
            .is_none());
    }

    /// An f32 in the final rung (and in a gamut, at either rung) must come
    /// back out of config.yaml as the decimal the caller sent.
    #[test]
    fn f32_values_are_written_as_the_decimal_the_caller_sent_at_every_rung() {
        let w = DeviceWrite {
            gamut: Some(GamutTuningValues {
                knee: Some(0.7),
                ..Default::default()
            }),
            final_rung: Some(CascadeValues {
                strength: Some(0.9),
                gamut: GamutTuningValues {
                    amount: Some(0.3),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        let yaml = serde_yaml::to_string(&device_block(&w, "local/a")).unwrap();
        for expected in ["strength: 0.9", "amount: 0.3", "knee: 0.7"] {
            assert!(yaml.contains(expected), "want `{expected}`, got:\n{yaml}");
        }
        assert!(!yaml.contains("0.8999"), "widened f32 leaked:\n{yaml}");
    }

    /// Every `CascadeValues` field must be addressable as `final.<field>`, or
    /// a final-rung setting becomes a one-way door.
    #[test]
    fn every_cascade_field_is_clearable_in_the_final_rung() {
        for field in CASCADE_FIELDS {
            let name = format!("final.{field}");
            resolve_clear(&with_clear(device_write_for_test(), &[name.as_str()]))
                .unwrap_or_else(|e| panic!("`{name}` must be clearable: {e:?}"));
        }
    }

    /// `CASCADE_FIELDS` is hand-maintained beside `CascadeValues`; a field
    /// added there but not here would be a one-way door. Round-trip through
    /// serde so the struct itself is the witness.
    #[test]
    fn cascade_fields_matches_the_struct() {
        let v = serde_json::to_value(CascadeValues::default()).unwrap();
        let mut from_struct: Vec<&str> =
            v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        let mut listed = CASCADE_FIELDS.to_vec();
        from_struct.sort_unstable();
        listed.sort_unstable();
        assert_eq!(from_struct, listed);
    }

    #[test]
    fn final_rung_values_are_validated_like_the_base_rung() {
        let state = state_with_addon_mode(false);
        for bad in [
            CascadeValues {
                dither: Some("nonsense".into()),
                ..Default::default()
            },
            CascadeValues {
                colors: Some("#00000,#FFFFFF".into()),
                ..Default::default()
            },
            CascadeValues {
                strength: Some(-1.0),
                ..Default::default()
            },
        ] {
            let w = DeviceWrite {
                final_rung: Some(bad),
                ..device_write_for_test()
            };
            assert!(validate_device_settings(&state, &w).is_err());
        }
    }

    #[test]
    fn unmanaged_device_keys_tolerates_missing_device_and_bad_yaml() {
        // Preservation is best-effort: it must never be the thing that fails
        // a write.
        assert!(unmanaged_device_keys(DEVICE_YAML, "NO:SUCH:MAC").is_empty());
        assert!(unmanaged_device_keys("::: not yaml :::", "1C:DB:D4:66:5B:50").is_empty());
        assert!(unmanaged_device_keys("", "1C:DB:D4:66:5B:50").is_empty());
    }

    #[test]
    fn global_writes_rejected_in_addon_mode() {
        let state = state_with_addon_mode(true);
        match require_writable_global(&state) {
            Err(ApiError::Conflict(_)) => {}
            other => panic!("expected Conflict in add-on mode, got {other:?}"),
        }
    }

    #[test]
    fn global_writes_allowed_standalone() {
        let state = state_with_addon_mode(false);
        assert!(require_writable_global(&state).is_ok());
    }

    fn registration_only_body() -> SettingsWrite {
        SettingsWrite {
            registration_enabled: Some(true),
            auth_mode: None,
            screen_repo_refresh_interval: None,
        }
    }

    #[test]
    fn addon_mode_allows_registration_enabled_only() {
        let state = state_with_addon_mode(true);
        let body = registration_only_body();
        assert!(
            require_writable_settings(&state, &body).is_ok(),
            "registration_enabled-only body must stay live in add-on mode"
        );
    }

    #[test]
    fn addon_mode_rejects_global_field() {
        let state = state_with_addon_mode(true);
        let mut body = registration_only_body();
        body.auth_mode = Some("api_key".to_string());
        match require_writable_settings(&state, &body) {
            Err(ApiError::Conflict(_)) => {}
            other => panic!("expected Conflict for global field in add-on mode, got {other:?}"),
        }
    }

    #[test]
    fn standalone_allows_any_settings_body() {
        let state = state_with_addon_mode(false);
        let body = SettingsWrite {
            registration_enabled: Some(false),
            auth_mode: Some("ed25519".to_string()),
            screen_repo_refresh_interval: Some(300),
        };
        assert!(require_writable_settings(&state, &body).is_ok());
    }
}
