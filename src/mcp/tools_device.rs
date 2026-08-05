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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssignScreenArgs {
    /// Device MAC (or its config key), as reported by `list_devices`.
    pub mac: String,
    /// Screen reference to assign, `handle/path`.
    pub screen_ref: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AssignScreenOutput {
    pub key: String,
    pub screen: String,
}

#[tool_router(router = tools_device_router, vis = "pub")]
impl ByonkMcp {
    #[tool(
        description = "Assign a device to a screen (use list_devices first to find its mac). \
                          The screen must exist. If the device already has a mapping, it is \
                          updated in place — its existing params carry over unchanged \
                          (revalidated against the new screen's schema), they are NOT reset to \
                          the new screen's defaults. If the device has only been seen (it \
                          appears in list_devices but has never been assigned a screen), \
                          assign_screen creates its mapping now."
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
        // A device that has only been *seen* by the registry (it shows up in
        // list_devices) has no `config.devices` entry yet, so the patch core
        // 404s on it. Fall back to creating the mapping — this is the normal
        // first-assignment path for a freshly onboarded device.
        let result = match apply_device_patch(&self.state, &a.mac, body()).await {
            Err(ApiError::NotFound) => apply_device_add(&self.state, &a.mac, body()).await,
            other => other,
        };
        match result {
            Ok(value) => ok_json(AssignScreenOutput {
                key: value["key"].as_str().unwrap_or(&a.mac).to_string(),
                screen: value["screen"]
                    .as_str()
                    .unwrap_or(&a.screen_ref)
                    .to_string(),
            }),
            // Tool-level, not protocol-level: "unknown screen `local/x`" and
            // "device not found" are exactly the messages the agent needs to
            // read and act on.
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(
                e.to_string(),
            )])),
        }
    }
}
