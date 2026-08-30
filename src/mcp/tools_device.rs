//! Device configuration. A device mapping is not global config, so this stays
//! available when byonk runs as a Home Assistant app — matching
//! `PATCH /api/admin/devices/{key}`.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{ok_json, ByonkMcp};
use crate::api::admin::write::{apply_device_add, apply_device_patch, DeviceWrite};
use crate::error::ApiError;
use crate::models::DeviceId;
use crate::services::device_registry::DeviceRegistry;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConfigureDeviceArgs {
    /// Device MAC, or the key it is configured under. `list_devices` reports
    /// the MAC of every device that has actually connected; a device that is
    /// only present in the config and has never connected is configurable too,
    /// but will not appear there.
    pub mac: String,
    /// Screen to show, `handle/path`. Required the first time a device is
    /// configured; afterwards, omit it to leave the device on its screen.
    pub screen_ref: Option<String>,
    /// Panel profile name, as configured under `panels` — `get_config` lists
    /// them. Decides the device's palette and its dither tuning defaults.
    pub panel: Option<String>,
    /// Dither algorithm. Overrides whatever the panel picks. One of
    /// floyd-steinberg, atkinson, atkinson-hybrid, jarvis-judice-ninke,
    /// sierra, sierra-two-row, sierra-lite, stucki, burkes.
    pub dither: Option<String>,
    /// Palette override, comma-separated `#rrggbb` (at least two).
    pub colors: Option<String>,
    /// Parameters for the screen's Lua script, validated against its
    /// `meta.yaml` schema. Merged key-by-key into the device's existing
    /// params, so setting one does not drop the others — unless `screen_ref`
    /// also changes, which replaces them wholesale.
    pub params: Option<HashMap<String, serde_json::Value>>,
    /// Refresh interval in seconds. 0 means "use the screen's own default".
    pub refresh: Option<u32>,
    /// Friendly name for the device.
    pub name: Option<String>,
    /// Cap on accumulated dithering error. Its useful range is around 1.0.
    pub max_error: Option<f32>,
    /// Blue-noise jitter scale.
    pub noise_scale: Option<f32>,
    /// Chroma clamp for dithering.
    pub chroma_clamp: Option<f32>,
    /// Dither strength. 0.0 diffuses nothing, 1.0 is standard.
    pub strength: Option<f32>,
    /// E-ink refresh profile sent to the device: `default`, `a` or `b`. What
    /// it does depends on the panel — on a TRMNL X any non-default value just
    /// buys heavier pre-draw clearing. Needs firmware >= 1.8.4.
    pub temperature_profile: Option<String>,
    /// Ask the firmware to force a full-waveform refresh every update. No
    /// effect on a TRMNL X, which has no partial refresh to switch off.
    pub maximum_compatibility: Option<bool>,
    /// Pad the PNG served to this device to at least this many bytes. A TRMNL
    /// X picks its grayscale table from the file's byte size, using a 38-pass
    /// table above 102400 bytes and a 9-pass one below, so `102401` reaches
    /// the better table without changing a pixel. Pointless on other panels,
    /// whose firmware caps downloads below that threshold anyway.
    pub min_png_bytes: Option<u32>,
    /// Settings to remove from this device, by name — for example
    /// `["noise_scale"]`. Omitting a field means "leave it alone", so this is
    /// the only way to take a setting back and let the panel's value (or the
    /// firmware's default) apply again. A field cannot be set and cleared in
    /// the same call.
    pub clear: Option<Vec<String>>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConfigureDeviceOutput {
    pub key: String,
    pub screen: String,
    /// True when this call created the device's first mapping (it had only
    /// been seen by the registry before); false when it updated an existing
    /// mapping in place.
    pub created: bool,
    /// The settings this call removed, echoed back so the caller can see the
    /// clear took effect.
    pub cleared: Vec<String>,
}

/// Translate the tool's JSON params into the YAML the config writer stores.
///
/// Every JSON scalar, sequence and map has a YAML equivalent, so this cannot
/// fail for anything that arrived as valid JSON in the first place; a value
/// that somehow does not convert is dropped rather than failing the call,
/// leaving `validate_params` to report it as a missing or wrong-typed field.
fn params_to_yaml(
    params: HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_yaml::Value> {
    params
        .into_iter()
        .filter_map(|(k, v)| serde_yaml::to_value(v).ok().map(|v| (k, v)))
        .collect()
}

#[tool_router(router = tools_device_router, vis = "pub")]
impl ByonkMcp {
    #[tool(
        description = "Configure a device: which screen it shows, and how that screen is \
                          rendered for it. A device is accepted if it is already configured \
                          OR the registry has seen it connect; anything else is rejected. \
                          Note those are two different sets: list_devices reports only \
                          devices that have connected, so a device that exists solely in the \
                          config is configurable without appearing there — use get_config to \
                          see those. Every field except mac is optional and an omitted field \
                          is left unchanged, so this is a partial update; use clear to remove \
                          a setting instead of changing it. screen_ref is required only the \
                          first time a device is configured, and the result reports \
                          created: true for that call. Changing screen_ref replaces the \
                          device's params wholesale (existing params carry over unchanged if \
                          you send none — they are not reset to the new screen's defaults); \
                          without a screen change, params are merged key by key."
    )]
    pub async fn configure_device(
        &self,
        Parameters(a): Parameters<ConfigureDeviceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = || DeviceWrite {
            key: None,
            screen: a.screen_ref.clone(),
            panel: a.panel.clone(),
            dither: a.dither.clone(),
            colors: a.colors.clone(),
            params: a.params.clone().map(params_to_yaml),
            refresh: a.refresh,
            name: a.name.clone(),
            max_error: a.max_error,
            noise_scale: a.noise_scale,
            chroma_clamp: a.chroma_clamp,
            strength: a.strength,
            temperature_profile: a.temperature_profile.clone(),
            maximum_compatibility: a.maximum_compatibility,
            min_png_bytes: a.min_png_bytes,
            clear: a.clear.clone(),
        };
        // Resolve the identifier the agent passed (its actual MAC, as
        // `list_devices` reports it, or its config key directly) to the
        // *existing* `config.devices` key, if any — the same resolution
        // `list_devices` performs (MAC, case-insensitively, or registration
        // code). Without this, a device configured by registration code or
        // under a differently-cased MAC would get patched by exact key,
        // miss, and then be re-created under a brand-new MAC-keyed entry —
        // shadowing (not replacing) the original config, which silently
        // drops its name/params/panel/dither/refresh from the effective
        // config (see MUST-FIX 1 in the branch review).
        let seen = self
            .state
            .registry
            .find_by_id(&DeviceId::new(a.mac.clone()))
            .await;
        let seen = match seen {
            Ok(seen) => seen,
            Err(e) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    e.to_string(),
                )]))
            }
        };
        let code = seen.as_ref().map(|d| d.api_key.registration_code());
        let resolved_key = self
            .state
            .config
            .load()
            .resolve_device_key(&a.mac, code.as_deref());

        // A device that has only been *seen* by the registry (it shows up in
        // list_devices) has no `config.devices` entry yet, so nothing
        // resolves. Fall back to creating the mapping — this is the normal
        // first-configuration path for a freshly onboarded device. But only
        // for a mac the registry actually reports: an arbitrary/typo'd mac
        // must not silently persist a phantom device with no MCP tool to
        // remove it. Same "known" notion `list_devices` uses.
        let mut created = false;
        let result = match resolved_key {
            Some(key) => apply_device_patch(&self.state, &key, body()).await,
            None => match seen {
                // Creating needs a screen, and saying so here beats letting
                // `apply_device_add` report a bare "`screen` is required" for
                // a call that never mentioned creating anything.
                Some(_) if a.screen_ref.is_none() => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "this device has connected but is not configured yet, so there is \
                         nothing to update — pass screen_ref to configure it for the first \
                         time."
                            .to_string(),
                    )]))
                }
                Some(_) => {
                    created = true;
                    apply_device_add(&self.state, &a.mac, body()).await
                }
                None => Err(ApiError::NotFound),
            },
        };
        match result {
            Ok(value) => ok_json(ConfigureDeviceOutput {
                key: value["key"].as_str().unwrap_or(&a.mac).to_string(),
                screen: value["screen"].as_str().unwrap_or_default().to_string(),
                created,
                cleared: a.clear.clone().unwrap_or_default(),
            }),
            // Tool-level, not protocol-level: "unknown screen `local/x`" and
            // "device not found" are exactly the messages the agent needs to
            // read and act on.
            Err(ApiError::NotFound) => Ok(CallToolResult::error(vec![ContentBlock::text(
                "no such device — it is neither configured nor has it ever connected. \
                 list_devices shows devices that have connected; get_config shows those \
                 configured but not yet seen."
                    .to_string(),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(
                e.to_string(),
            )])),
        }
    }
}
