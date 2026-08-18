//! Device assignment. A device mapping is not global config, so this stays
//! available when byonk runs as a Home Assistant app — matching
//! `PATCH /api/admin/devices/{key}`.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{ok_json, ByonkMcp};
use crate::api::admin::write::{apply_device_add, apply_device_patch, DeviceWrite};
use crate::error::ApiError;
use crate::models::DeviceId;
use crate::services::device_registry::DeviceRegistry;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssignScreenArgs {
    /// Device MAC, or the key it is configured under. `list_devices` reports
    /// the MAC of every device that has actually connected; a device that is
    /// only present in the config and has never connected is assignable too,
    /// but will not appear there.
    pub mac: String,
    /// Screen reference to assign, `handle/path`.
    pub screen_ref: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AssignScreenOutput {
    pub key: String,
    pub screen: String,
    /// True when this call created the device's first mapping (it had only
    /// been seen by the registry before); false when it updated an existing
    /// mapping in place.
    pub created: bool,
}

#[tool_router(router = tools_device_router, vis = "pub")]
impl ByonkMcp {
    #[tool(
        description = "Assign a device to a screen. The screen must exist. A device is \
                          accepted if it is already configured OR the registry has seen it \
                          connect; anything else is rejected. Note those are two different \
                          sets: list_devices reports only devices that have connected, so a \
                          device that exists solely in the config is assignable without \
                          appearing there — use get_config to see those. If the device already \
                          has a mapping it is updated in place, its existing params carry over \
                          unchanged (revalidated against the new screen's schema) rather than \
                          being reset to the new screen's defaults, and the result reports \
                          created: false. If it had only been seen by the registry, its mapping \
                          is created now and the result reports created: true."
    )]
    pub async fn assign_screen(
        &self,
        Parameters(a): Parameters<AssignScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = || DeviceWrite {
            key: None,
            screen: Some(a.screen_ref.clone()),
            panel: None,
            dither: None,
            colors: None,
            params: None,
            refresh: None,
            name: None,
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
        // first-assignment path for a freshly onboarded device. But only for
        // a mac the registry actually reports: an arbitrary/typo'd mac must
        // not silently persist a phantom device with no MCP tool to remove
        // it. Same "known" notion `list_devices` uses.
        let mut created = false;
        let result = match resolved_key {
            Some(key) => apply_device_patch(&self.state, &key, body()).await,
            None => match seen {
                Some(_) => {
                    created = true;
                    apply_device_add(&self.state, &a.mac, body()).await
                }
                None => Err(ApiError::NotFound),
            },
        };
        match result {
            Ok(value) => ok_json(AssignScreenOutput {
                key: value["key"].as_str().unwrap_or(&a.mac).to_string(),
                screen: value["screen"]
                    .as_str()
                    .unwrap_or(&a.screen_ref)
                    .to_string(),
                created,
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
