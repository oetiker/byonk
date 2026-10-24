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

/// Write `contents` to `tmp`, created with `target`'s mode bits from the start
/// (unix) so the contents are never readable more widely than the target.
fn write_tmp_like(
    target: &std::path::Path,
    tmp: &std::path::Path,
    contents: &str,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if let Ok(meta) = std::fs::metadata(target) {
            opts.mode(meta.permissions().mode() & 0o7777);
        }
    }
    #[cfg(not(unix))]
    let _ = target;
    // A stale tmp from a crash keeps its old mode under `mode()`, so drop it.
    let _ = std::fs::remove_file(tmp);
    let mut f = opts.open(tmp)?;
    f.write_all(contents.as_bytes())
}

pub fn persist(state: &AppState, path: &std::path::Path, new_yaml: String) -> Result<(), ApiError> {
    // Snapshot current contents so we can roll back if the new text fails to reparse.
    let previous = std::fs::read_to_string(path).ok();

    // Atomic write: temp file in same dir, then rename. On unix the temp
    // file is created with the existing file's mode from the start (never
    // wider, even for an instant): config.yaml holds tokens, and an owner who
    // made it private must not find it world-readable after a save. Unix
    // only: elsewhere `permissions()` is the read-only flag, and copying a
    // read-only flag onto the temp file would make the rename fail.
    let tmp = path.with_extension("yaml.tmp");
    let replace = || -> Result<(), ApiError> {
        write_tmp_like(path, &tmp, &new_yaml)
            .map_err(|e| ApiError::Internal(format!("write temp: {e}")))?;
        std::fs::rename(&tmp, path).map_err(|e| ApiError::Internal(format!("rename: {e}")))
    };
    if let Err(e) = replace() {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

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

#[cfg(all(test, unix))]
mod tmp_mode_tests {
    use super::write_tmp_like;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_temp_file_is_created_with_the_targets_mode_never_wider() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.yaml");
        let tmp = dir.path().join("config.yaml.tmp");
        std::fs::write(&target, "a: 1\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        // A stale, world-readable temp must not leak its mode either.
        std::fs::write(&tmp, "stale").unwrap();
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_tmp_like(&target, &tmp, "token: x\n").unwrap();

        let mode = std::fs::metadata(&tmp).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), "token: x\n");
    }
}
