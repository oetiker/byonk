//! Mutating MCP tools. Every one goes through `ScreenStore`, so a read-only
//! target is refused structurally and the refusal names `copy_screen`.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
    ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{blocking, ok_json, store_failure, ByonkMcp};
use crate::services::screen_store::StarterKind;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteFileArgs {
    /// Screen reference, `handle/path`. Must be in a writable repo.
    pub screen_ref: String,
    /// File inside the screen directory.
    pub file: String,
    /// New UTF-8 contents, written atomically.
    pub content: String,
    /// The etag you last read. Omit to force the write; supply it to be told
    /// (`conflict`) when the file changed underneath you.
    #[serde(default)]
    pub if_match: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EtagOutput {
    pub etag: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ScreenRefOutput {
    pub screen_ref: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateScreenArgs {
    /// Writable repo handle — usually `local`.
    pub handle: String,
    /// New screen name, e.g. `clock` or `home/clock`.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CopyScreenArgs {
    /// Source screen, which may be read-only (a builtin or an example).
    pub from_ref: String,
    pub to_handle: String,
    pub to_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameScreenArgs {
    pub screen_ref: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenRefArgs {
    pub screen_ref: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteFileArgs {
    pub screen_ref: String,
    /// Sibling asset to delete. meta.yaml / script.lua / screen.svg define
    /// the screen and cannot be deleted — use `delete_screen` instead.
    pub file: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct OkOutput {
    pub ok: bool,
}

#[tool_router(router = tools_edit_router, vis = "pub")]
impl ByonkMcp {
    #[tool(
        description = "Write one file inside a screen, atomically. Pass if_match with the \
                          etag you read to detect concurrent edits. Only writable repos \
                          accept writes; fork a read-only screen with copy_screen first. \
                          UTF-8 text only: refused if the target already exists and is a \
                          binary asset (not valid UTF-8) — binary assets must be placed by \
                          other means, not over MCP."
    )]
    pub async fn write_screen_file(
        &self,
        Parameters(a): Parameters<WriteFileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome = blocking(move || {
            store.write_file(
                &a.screen_ref,
                &a.file,
                a.content.as_bytes(),
                a.if_match.as_deref(),
            )
        })
        .await?;
        match outcome {
            Ok(etag) => ok_json(EtagOutput { etag }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(
        description = "Scaffold a new screen from the minimal starter (meta.yaml, \
                          script.lua, screen.svg extending the byonk-base-v1 layout)."
    )]
    pub async fn create_screen(
        &self,
        Parameters(a): Parameters<CreateScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome =
            blocking(move || store.create_screen(&a.handle, &a.name, StarterKind::Minimal)).await?;
        match outcome {
            Ok(screen_ref) => ok_json(ScreenRefOutput { screen_ref }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(
        description = "Fork any screen — including read-only builtins and examples — into a \
                          writable repo, copying every file in its directory. This is how you \
                          customize a screen you cannot edit in place."
    )]
    pub async fn copy_screen(
        &self,
        Parameters(a): Parameters<CopyScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome =
            blocking(move || store.copy_screen(&a.from_ref, &a.to_handle, &a.to_name)).await?;
        match outcome {
            Ok(screen_ref) => ok_json(ScreenRefOutput { screen_ref }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(
        description = "Rename a screen within its repo. Devices still pointing at the old \
                          reference will stop resolving — reassign them with assign_screen."
    )]
    pub async fn rename_screen(
        &self,
        Parameters(a): Parameters<RenameScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome = blocking(move || store.rename_screen(&a.screen_ref, &a.new_name)).await?;
        match outcome {
            Ok(screen_ref) => ok_json(ScreenRefOutput { screen_ref }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(
        description = "Delete a screen and every file in its directory. Devices pointing at \
                          it will fall back to the builtin default."
    )]
    pub async fn delete_screen(
        &self,
        Parameters(a): Parameters<ScreenRefArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        match blocking(move || store.delete_screen(&a.screen_ref)).await? {
            Ok(()) => ok_json(OkOutput { ok: true }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(
        description = "Delete one sibling asset from a screen directory. meta.yaml, \
                          script.lua and screen.svg cannot be deleted this way."
    )]
    pub async fn delete_screen_file(
        &self,
        Parameters(a): Parameters<DeleteFileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        match blocking(move || store.delete_file(&a.screen_ref, &a.file)).await? {
            Ok(()) => ok_json(OkOutput { ok: true }),
            Err(e) => Ok(store_failure(e)),
        }
    }
}
