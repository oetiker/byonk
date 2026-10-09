//! Atomically replace `config.yaml` and swap the result into the live config.
//!
//! Shared by the admin write handlers and by the dev UI's colour-tuning save.
//! Kept in one place because the roll-back is the interesting part: a patch
//! that produces YAML byonk cannot reparse must leave the file as it was.

use crate::error::ApiError;
use crate::server::{reload_config, AppState};

/// Guard: writes require a file-backed config.
pub fn require_file_config(state: &AppState) -> Result<std::path::PathBuf, ApiError> {
    state
        .asset_loader
        .config_path()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| ApiError::Conflict("config is embedded/read-only; set CONFIG_FILE".into()))
}

pub fn persist(state: &AppState, path: &std::path::Path, new_yaml: String) -> Result<(), ApiError> {
    // Snapshot current contents so we can roll back if the new text fails to reparse.
    let previous = std::fs::read_to_string(path).ok();

    // Atomic write: temp file in same dir, then rename.
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, &new_yaml).map_err(|e| ApiError::Internal(format!("write temp: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| ApiError::Internal(format!("rename: {e}")))?;

    // Reload into the live config; on failure, roll the file back.
    if let Err(e) = reload_config(state) {
        if let Some(prev) = previous {
            let _ = std::fs::write(path, prev);
        }
        let msg = format!("reload failed, rolled back: {e}");
        // The owner's hand-edited file holds a key byonk refuses: their file is
        // wrong, not byonk.
        if e.downcast_ref::<crate::models::config::UnknownDeviceKey>()
            .is_some()
        {
            return Err(ApiError::BadRequest(msg));
        }
        return Err(ApiError::Internal(msg));
    }
    Ok(())
}
