//! Moving (or renaming) a screen and pointing config.yaml at its new ref.
//!
//! One core shared by the dev UI's `POST /dev/api/screen/move` and the MCP
//! `rename_screen` tool, so both leave the same config behind: every device
//! whose `screen:` named the old ref names the new one, in the same request.

use crate::error::ApiError;
use crate::server::AppState;
use crate::services::screen_store::StoreError;

/// A successful move: the new ref, and the device keys whose `screen:` was
/// rewritten (sorted).
#[derive(Debug)]
pub struct ScreenMoved {
    pub screen_ref: String,
    pub rewritten: Vec<String>,
}

/// Why a move failed: the store refused it (nothing moved), or config.yaml
/// could not follow (the move was undone).
#[derive(Debug)]
pub enum MoveError {
    Store(StoreError),
    Config(ApiError),
}

/// Move `from_ref` to `to_handle/to_name` and rewrite the devices on it.
///
/// Holds the config write lock for the whole operation, so no config write
/// slips in between choosing the devices and rewriting them. A config that
/// cannot be written (embedded, or the patch fails to reload) undoes the
/// move: the screen and config.yaml never disagree.
pub async fn apply_screen_move(
    state: &AppState,
    from_ref: &str,
    to_handle: &str,
    to_name: &str,
) -> Result<ScreenMoved, MoveError> {
    let _guard = state.write_lock.lock().await;
    let mut users: Vec<String> = state
        .config
        .load()
        .devices
        .iter()
        .filter(|(_, d)| d.screen == from_ref)
        .map(|(k, _)| k.clone())
        .collect();
    users.sort_unstable();
    // Refused before anything moves: a device on this screen whose config
    // cannot be rewritten would be left pointing at nothing.
    let config_path = if users.is_empty() {
        None
    } else {
        Some(crate::api::config_persist::require_file_config(state).map_err(MoveError::Config)?)
    };

    let store = state.screen_store.clone();
    let (from, handle, name) = (
        from_ref.to_owned(),
        to_handle.to_owned(),
        to_name.to_owned(),
    );
    let screen_ref = blocking(move || store.move_screen(&from, &handle, &name))
        .await?
        .map_err(MoveError::Store)?;

    if let Some(path) = config_path {
        if let Err(e) = rewrite_refs(state, &path, &users, &screen_ref) {
            // Put the screen back where the config still points.
            let store = state.screen_store.clone();
            let (back, (handle, path)) = (screen_ref.clone(), split(from_ref));
            let _ = blocking(move || store.move_screen(&back, &handle, &path)).await;
            return Err(MoveError::Config(e));
        }
    }
    Ok(ScreenMoved {
        screen_ref,
        rewritten: users,
    })
}

fn rewrite_refs(
    state: &AppState,
    path: &std::path::Path,
    users: &[String],
    new_ref: &str,
) -> Result<(), ApiError> {
    let mut yaml = std::fs::read_to_string(path)
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;
    for device in users {
        yaml = crate::services::config_writer::set_path(
            &yaml,
            &["devices", device.as_str(), "screen"],
            &new_ref.into(),
        )
        .map_err(|e| ApiError::Internal(format!("patch config: {e}")))?;
    }
    crate::api::config_persist::persist(state, path, yaml)
}

fn split(screen_ref: &str) -> (String, String) {
    let (h, p) = screen_ref.split_once('/').unwrap_or((screen_ref, ""));
    (h.to_owned(), p.to_owned())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, MoveError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| MoveError::Config(ApiError::Internal(e.to_string())))
}
