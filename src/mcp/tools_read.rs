//! Read-context MCP tools: what screens, repos and devices exist, and what a
//! screen's files contain.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{blocking, ok_json, store_failure, ByonkMcp};
use crate::services::DeviceRegistry;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenFileArgs {
    /// Screen reference, `handle/path` — e.g. `local/clock`.
    pub screen_ref: String,
    /// File inside the screen directory — e.g. `script.lua`.
    pub file: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ScreenEntry {
    pub screen_ref: String,
    pub handle: String,
    pub title: String,
    pub description: String,
    /// Engine compatibility requirement from `meta.yaml` (a caret range).
    pub byonk: String,
    /// Whether this screen's repo can be written to. Fork a read-only screen
    /// with `copy_screen` before editing it.
    pub writable: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListScreensOutput {
    pub screens: Vec<ScreenEntry>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FileOutput {
    /// UTF-8 contents. Empty when `binary` is true.
    pub content: String,
    /// Pass back as `if_match` on `write_screen_file` for safe edits.
    pub etag: String,
    pub binary: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RepoEntry {
    pub handle: String,
    /// `embedded` | `git` | `local`
    pub kind: String,
    pub name: String,
    pub screen_count: usize,
    pub writable: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListReposOutput {
    pub repos: Vec<RepoEntry>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeviceEntry {
    pub mac: String,
    pub name: Option<String>,
    pub model: Option<String>,
    pub screen: Option<String>,
    pub last_seen: Option<String>,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListDevicesOutput {
    pub devices: Vec<DeviceEntry>,
}

#[tool_router(router = tools_read_router, vis = "pub")]
impl ByonkMcp {
    /// List every screen this server can resolve, with its repo, title and
    /// whether it is writable.
    #[tool(
        description = "List every screen on this byonk server, with repo handle, title, \
                          compat requirement, file list and whether it can be edited."
    )]
    pub async fn list_screens(&self) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let entries = blocking(move || store.list_screens()).await?;
        ok_json(ListScreensOutput {
            screens: entries
                .into_iter()
                .map(|e| ScreenEntry {
                    screen_ref: e.screen_ref,
                    handle: e.handle,
                    title: e.title,
                    description: e.description,
                    byonk: e.byonk,
                    writable: e.writable,
                    files: e.files,
                })
                .collect(),
        })
    }

    /// Read one file inside a screen.
    #[tool(
        description = "Read one file inside a screen (meta.yaml, script.lua, screen.svg or \
                          any sibling asset). Returns its contents and an etag to pass back \
                          as if_match when writing."
    )]
    pub async fn read_screen_file(
        &self,
        Parameters(args): Parameters<ScreenFileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let contents = match blocking(move || store.read_file(&args.screen_ref, &args.file)).await?
        {
            Ok(c) => c,
            Err(e) => return Ok(store_failure(e)),
        };
        ok_json(FileOutput {
            content: if contents.binary {
                String::new()
            } else {
                String::from_utf8_lossy(&contents.bytes).into_owned()
            },
            etag: contents.etag,
            binary: contents.binary,
        })
    }

    /// List the configured screen repositories.
    #[tool(
        description = "List the screen repositories registered on this server: handle, kind \
                          (embedded/git/local), screen count and writability."
    )]
    pub async fn list_screen_repos(&self) -> Result<CallToolResult, ErrorData> {
        // Build from the live loader so `kind` and `writable` agree with what
        // the write path will actually enforce.
        let manager = self.state.screen_repo_manager.clone();
        let config = self.state.config.clone();
        let repos = blocking(move || {
            let loader = manager.loader();
            let cfg = config.load();

            // Screen counts per handle, from the source of truth.
            let mut counts: std::collections::HashMap<String, usize> = Default::default();
            for s in loader.list_all() {
                *counts.entry(s.handle).or_insert(0) += 1;
            }

            // Union of loader-registered handles and configured screen-repo
            // handles: `loader.list_all()` alone would hide a repo with zero
            // screens (e.g. one an authoring agent just created and hasn't
            // populated yet), leaving it nowhere for the agent to see it is
            // allowed to write. Mirrors `src/api/admin/read.rs::screen_repos`.
            let mut handles: std::collections::BTreeSet<String> =
                loader.handles().into_iter().collect();
            handles.extend(cfg.screen_repos.keys().cloned());

            handles
                .into_iter()
                .filter_map(|handle| {
                    // No source means the manifest failed to load (and the
                    // loader already warned) — nothing to report a kind/name
                    // for, so skip it, same as it not appearing anywhere else.
                    let source = loader.source_for(&handle)?;
                    Some(RepoEntry {
                        screen_count: counts.get(&handle).copied().unwrap_or(0),
                        kind: source.kind().as_str().to_string(),
                        name: source.manifest().name.clone(),
                        writable: source.writable_root().is_some(),
                        handle,
                    })
                })
                .collect::<Vec<_>>()
        })
        .await?;
        ok_json(ListReposOutput { repos })
    }

    /// List known devices.
    #[tool(
        description = "List TRMNL devices this server knows about: MAC, model, assigned \
                          screen, last-seen time, battery and signal strength."
    )]
    pub async fn list_devices(&self) -> Result<CallToolResult, ErrorData> {
        // Mirrors the merge `src/api/admin/read.rs::list_devices` performs
        // (registry telemetry + config mapping).
        //
        // A registry read failure is the registry's problem, not a protocol
        // fault — report it as a tool-level error (visible to the agent),
        // not `Err(ErrorData)` (opaque to the model).
        let seen = match self.state.registry.list_all().await {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "failed to read the device registry: {e}"
                ))]))
            }
        };
        let config = self.state.config.load();
        let devices: Vec<DeviceEntry> = seen
            .iter()
            .map(|d| {
                let mac = d.device_id.to_string();
                let code = d.api_key.registration_code();
                let dc = config
                    .get_device_config(&mac)
                    .or_else(|| config.get_device_config_for_code(&code));
                DeviceEntry {
                    screen: dc.map(|c| c.screen.clone()),
                    name: dc.and_then(|c| c.name.clone()),
                    model: Some(d.model.clone()),
                    last_seen: Some(d.last_seen.to_rfc3339()),
                    battery_voltage: d.battery_voltage,
                    rssi: d.rssi,
                    mac,
                }
            })
            .collect();
        ok_json(ListDevicesOutput { devices })
    }

    /// Non-secret global configuration.
    #[tool(
        description = "Read this server's non-secret global configuration. Tokens and other \
                          credentials are never included."
    )]
    pub async fn get_config(&self) -> Result<CallToolResult, ErrorData> {
        // Delegate to the same redaction the admin API applies — do not
        // hand-roll a second one that could drift and start leaking. Goes
        // through `blocking` like every other tool here: `redacted_config`
        // does synchronous file IO (`read_config_string`).
        let state = self.state.clone();
        match blocking(move || crate::api::admin::read::redacted_config(&state)).await? {
            Ok(v) => ok_json(v),
            Err(msg) => Ok(CallToolResult::error(vec![ContentBlock::text(msg)])),
        }
    }
}
