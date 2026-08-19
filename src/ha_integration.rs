//! Installs byonk's Home Assistant integration into the Home Assistant config
//! directory.
//!
//! Byonk ships as a Supervisor app, and the app is the only thing a user
//! installs. The integration half therefore travels inside the app image
//! (`Dockerfile.release` copies `custom_components/byonk` to
//! `/app/custom_components/byonk`) and is written out here, into the config dir
//! Supervisor mounts at `/homeassistant`. Home Assistant reads
//! `custom_components/` only when it starts, so a restart is still needed; the
//! caller asks for one (see `install_and_announce`).
//!
//! Everything here is best effort. A failure logs and lets the server run.

use std::path::{Path, PathBuf};

/// Where Supervisor mounts the Home Assistant config dir for the
/// `homeassistant_config` mapping (`supervisor/docker/const.py`).
const DEFAULT_HA_CONFIG_DIR: &str = "/homeassistant";

/// Where `Dockerfile.release` puts the integration inside the image.
const DEFAULT_INTEGRATION_SRC: &str = "/app/custom_components/byonk";

/// Staging directory name, a sibling of the target inside `custom_components/`.
const STAGING_NAME: &str = ".byonk-new";

/// Home Assistant config dir. `BYONK_HA_CONFIG_DIR` overrides it (tests, and an
/// escape hatch), mirroring `addon_options::options_path`.
pub fn ha_config_dir() -> PathBuf {
    std::env::var("BYONK_HA_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_HA_CONFIG_DIR))
}

/// The integration source inside the image. `BYONK_INTEGRATION_SRC` overrides it.
pub fn integration_src() -> PathBuf {
    std::env::var("BYONK_INTEGRATION_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_INTEGRATION_SRC))
}

/// What `install` did, so the caller can decide whether to tell the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// The installed integration is already this version.
    NotNeeded,
    /// Written. `from` is the version that was there before, if any.
    Installed { from: Option<String>, to: String },
    /// The target exists and is not byonk's. Nothing was touched.
    Refused(String),
    /// Something went wrong. Nothing usable was written.
    Failed(String),
}

/// Read one top-level string field out of a `manifest.json` in `dir`.
fn manifest_field(dir: &Path, field: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get(field)?.as_str().map(str::to_string)
}

/// Copy `src` into `dst` recursively, creating `dst`.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Write the integration from `src` into `<ha_config>/custom_components/byonk`.
///
/// The swap deletes a directory inside the user's Home Assistant config, so it
/// happens only when the target does not exist, or its `manifest.json` names
/// byonk's own domain. Nothing else under `ha_config` is read, walked or
/// removed.
pub fn install(src: &Path, ha_config: &Path) -> InstallOutcome {
    let Some(ours) = manifest_field(src, "version") else {
        return InstallOutcome::Failed(format!("no readable manifest.json in {}", src.display()));
    };

    let custom_components = ha_config.join("custom_components");
    let target = custom_components.join("byonk");
    let installed = manifest_field(&target, "version");

    if installed.as_deref() == Some(ours.as_str()) {
        return InstallOutcome::NotNeeded;
    }

    // Refusal guard. An existing target must identify itself as byonk's.
    if target.exists() && manifest_field(&target, "domain").as_deref() != Some("byonk") {
        return InstallOutcome::Refused(format!(
            "{} exists but is not byonk's integration; leaving it alone",
            target.display()
        ));
    }

    if !ha_config.is_dir() {
        return InstallOutcome::Failed(format!(
            "Home Assistant config dir {} is not available",
            ha_config.display()
        ));
    }

    // Stage the new copy beside the target, then swap. A crashed earlier run can
    // leave staging behind; the name is ours, so clearing it is safe.
    let staging = custom_components.join(STAGING_NAME);
    if let Err(e) = std::fs::create_dir_all(&custom_components) {
        return InstallOutcome::Failed(format!(
            "could not create {}: {e}",
            custom_components.display()
        ));
    }
    if staging.exists() {
        if let Err(e) = std::fs::remove_dir_all(&staging) {
            return InstallOutcome::Failed(format!("could not clear {}: {e}", staging.display()));
        }
    }
    if let Err(e) = copy_dir(src, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return InstallOutcome::Failed(format!("could not stage the integration: {e}"));
    }
    if target.exists() {
        if let Err(e) = std::fs::remove_dir_all(&target) {
            let _ = std::fs::remove_dir_all(&staging);
            return InstallOutcome::Failed(format!("could not replace {}: {e}", target.display()));
        }
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        let _ = std::fs::remove_dir_all(&staging);
        return InstallOutcome::Failed(format!("could not move the integration into place: {e}"));
    }

    InstallOutcome::Installed {
        from: installed,
        to: ours,
    }
}
