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
use crate::models::{CascadeValues, DeviceId, GamutTuningValues};
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
    /// Measured colours for this one display, comma-separated `#rrggbb` with
    /// the same entry count as the resolved palette. Overrides the panel
    /// profile's calibration for this device only; a screen that measures its
    /// own still outranks it.
    pub colors_actual: Option<String>,
    /// Gamut mapping for continuous-tone regions: `knee` (where compression
    /// begins), `amount` (0 = off, 1 = full) and `max_compression`.
    pub gamut: Option<GamutTuningValues>,
    /// The device owner's **veto**. Anything set here outrules the screen's
    /// own choice — use it when a setting must hold on this display whatever
    /// the script asks for, for example a `refresh` floor on a
    /// battery-powered panel. Settings outside this block are defaults the
    /// screen may override. Takes the same cascade settings as above
    /// (`colors`, `colors_actual`, `dither`, `max_error`, `noise_scale`,
    /// `chroma_clamp`, `strength`, `refresh`, `gamut`) and nothing else.
    /// Merged field by field; remove a field with `clear: ["final.<field>"]`,
    /// or the whole block with `clear: ["final"]`.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
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
    ///
    /// One dot addresses the veto block: `"dither"` clears the device's
    /// default, `"final.dither"` clears its veto, and `"final"` removes the
    /// whole veto block. A bare name never touches `final`.
    pub clear: Option<Vec<String>>,
}

impl ConfigureDeviceArgs {
    /// The `DeviceWrite` this call means.
    ///
    /// A named method rather than a closure inside `configure_device`: the
    /// conversion is the half of the mirror that drifts (`gamut` reached
    /// `DeviceConfig` and never reached here), and a closure cannot be tested.
    pub fn to_device_write(&self) -> DeviceWrite {
        DeviceWrite {
            key: None,
            screen: self.screen_ref.clone(),
            panel: self.panel.clone(),
            dither: self.dither.clone(),
            colors: self.colors.clone(),
            colors_actual: self.colors_actual.clone(),
            gamut: self.gamut.clone(),
            final_rung: self.final_rung.clone(),
            params: self.params.clone().map(params_to_yaml),
            refresh: self.refresh,
            name: self.name.clone(),
            max_error: self.max_error,
            noise_scale: self.noise_scale,
            chroma_clamp: self.chroma_clamp,
            strength: self.strength,
            temperature_profile: self.temperature_profile.clone(),
            maximum_compatibility: self.maximum_compatibility,
            min_png_bytes: self.min_png_bytes,
            clear: self.clear.clone(),
        }
    }
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
        let body = || a.to_device_write();
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
            Ok(value) => {
                // Both write paths always report the screen, so this is
                // unreachable — but defaulting it would answer "" to the one
                // question the caller asked, and "no screen" is a claim an
                // agent would act on. `key` can fall back to the mac because
                // that is a true answer; there is no true answer here.
                let Some(screen) = value["screen"].as_str() else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "the device was written, but byonk's own result carried no screen — \
                         this is a bug; check the device with get_config."
                            .to_string(),
                    )]));
                };
                ok_json(ConfigureDeviceOutput {
                    key: value["key"].as_str().unwrap_or(&a.mac).to_string(),
                    screen: screen.to_string(),
                    created,
                    cleared: a.clear.clone().unwrap_or_default(),
                })
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The settable fields of `DeviceWrite`, as the agent-facing names, read
    /// off the struct itself. `key` is excluded: it is the entry name, not a
    /// setting, and the tool takes `mac` instead. `screen` is `screen_ref`
    /// here.
    fn device_write_fields() -> std::collections::BTreeSet<String> {
        let v = serde_json::to_value(DeviceWrite::default()).expect("DeviceWrite serialises");
        v.as_object()
            .expect("a struct serialises to an object")
            .keys()
            .filter(|k| *k != "key")
            .map(|k| {
                if k == "screen" {
                    "screen_ref".to_string()
                } else {
                    k.clone()
                }
            })
            .collect()
    }

    /// `ConfigureDeviceArgs` is a hand-written mirror of `DeviceWrite`. Only a
    /// test keeps the two in step: `gamut` went missing from this mirror for a
    /// whole release because nothing checked.
    ///
    /// `DeviceWrite` is the witness: its field set comes from serialising a
    /// default value, not from a list a human must remember to edit. Adding a
    /// field there (and writing `None` for it in `to_device_write`) therefore
    /// fails here until the tool gains it too.
    ///
    /// The nested config types are embedded directly (not re-mirrored), so
    /// they follow the config model by construction; the test pins that the
    /// schema really exposes their fields.
    #[test]
    fn the_mcp_mirror_carries_every_device_write_field() {
        let schema = schemars::schema_for!(ConfigureDeviceArgs);
        let json = serde_json::to_value(&schema).expect("schema serialises");
        let props: std::collections::BTreeSet<String> = json["properties"]
            .as_object()
            .expect("an object schema")
            .keys()
            .cloned()
            .collect();
        let want = device_write_fields();
        // `mac` is the tool's address for the device; DeviceWrite has no such
        // field (its `key` is excluded from `want` for the same reason).
        let mut have = props.clone();
        have.remove("mac");

        let missing: Vec<_> = want.difference(&have).collect();
        let extra: Vec<_> = have.difference(&want).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "ConfigureDeviceArgs and DeviceWrite disagree.\n  \
             DeviceWrite fields missing from ConfigureDeviceArgs (add them, and map them in to_device_write): {missing:?}\n  \
             ConfigureDeviceArgs fields DeviceWrite cannot take: {extra:?}"
        );

        // The embedded types expose their own fields through $defs.
        let defs = json["$defs"].as_object().expect("nested types in $defs");
        let fields_of = |name: &str| -> Vec<String> {
            defs[name]["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("`{name}` has no properties in the schema"))
                .keys()
                .cloned()
                .collect()
        };
        for f in ["knee", "amount", "max_compression"] {
            assert!(
                fields_of("GamutTuningValues").contains(&f.to_string()),
                "{f}"
            );
        }
        for f in [
            "colors",
            "colors_actual",
            "dither",
            "max_error",
            "noise_scale",
            "chroma_clamp",
            "strength",
            "refresh",
            "gamut",
        ] {
            assert!(fields_of("CascadeValues").contains(&f.to_string()), "{f}");
        }
    }

    /// Carrying a field in the schema is not enough: the conversion must
    /// actually pass it on. Every field is populated (no `..` shorthand, so a
    /// new `ConfigureDeviceArgs` field breaks this literal), then every
    /// `DeviceWrite` field must come out non-null.
    #[test]
    fn the_mcp_mirror_passes_every_field_to_device_write() {
        let a = ConfigureDeviceArgs {
            mac: "AA:BB:CC:DD:EE:FF".into(),
            screen_ref: Some("builtin/clock".into()),
            panel: Some("p".into()),
            dither: Some("sierra".into()),
            colors: Some("#000000,#FFFFFF".into()),
            colors_actual: Some("#1C5240,#FFFFFF".into()),
            gamut: Some(GamutTuningValues {
                knee: Some(0.8),
                ..Default::default()
            }),
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                ..Default::default()
            }),
            params: Some(HashMap::from([("k".to_string(), serde_json::json!(1))])),
            refresh: Some(60),
            name: Some("n".into()),
            max_error: Some(1.0),
            noise_scale: Some(2.0),
            chroma_clamp: Some(3.0),
            strength: Some(0.5),
            temperature_profile: Some("a".into()),
            maximum_compatibility: Some(true),
            min_png_bytes: Some(102401),
            clear: Some(vec!["final.strength".into()]),
        };
        let w = a.to_device_write();
        let v = serde_json::to_value(&w).unwrap();
        for (field, value) in v.as_object().unwrap() {
            if field == "key" {
                continue; // the entry name comes from `mac`, not from the body
            }
            assert!(
                !value.is_null(),
                "`{field}` stays unset in DeviceWrite although the tool call set it"
            );
        }
        assert_eq!(w.colors_actual.as_deref(), Some("#1C5240,#FFFFFF"));
        assert_eq!(w.gamut.and_then(|g| g.knee), Some(0.8));
        assert_eq!(
            w.final_rung.and_then(|f| f.dither),
            Some("atkinson".to_string()),
            "the conversion must pass the final rung, not drop it"
        );
        assert_eq!(w.clear, Some(vec!["final.strength".to_string()]));
    }

    /// `final` arrives under that name on the wire, not `final_rung`.
    #[test]
    fn the_final_block_is_read_from_the_wire_name() {
        let a: ConfigureDeviceArgs = serde_json::from_value(serde_json::json!({
            "mac": "AA:BB:CC:DD:EE:FF",
            "final": { "refresh": 900 },
            "gamut": { "amount": 0.5 },
        }))
        .unwrap();
        assert_eq!(a.final_rung.and_then(|f| f.refresh), Some(900));
        assert_eq!(a.gamut.and_then(|g| g.amount), Some(0.5));
    }
}
