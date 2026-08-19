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

/// Backup directory name, a sibling of the target inside `custom_components/`.
/// Holds the previous install for the duration of the swap, so it can be put
/// back if the final rename fails.
const BACKUP_NAME: &str = ".byonk-old";

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

/// Remove `dir` if it exists. Used to clear leftover scratch directories from
/// a crashed earlier run; a no-op if there is nothing to clear.
fn clear_if_present(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
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

    // Refusal guard. Ownership is the gate: an existing target must identify
    // itself as byonk's before anything else about it — including its version
    // — is trusted. This must run before the version check below, or a
    // foreign directory that happens to carry our version string would be
    // reported as "already up to date" instead of refused.
    if target.exists() && manifest_field(&target, "domain").as_deref() != Some("byonk") {
        return InstallOutcome::Refused(format!(
            "{} exists but is not byonk's integration; leaving it alone",
            target.display()
        ));
    }

    let installed = manifest_field(&target, "version");

    if installed.as_deref() == Some(ours.as_str()) {
        return InstallOutcome::NotNeeded;
    }

    if !ha_config.is_dir() {
        return InstallOutcome::Failed(format!(
            "Home Assistant config dir {} is not available",
            ha_config.display()
        ));
    }

    // Stage the new copy beside the target, then swap by rename. A crashed
    // earlier run can leave either scratch dir behind; both names are ours,
    // so clearing them is safe.
    let staging = custom_components.join(STAGING_NAME);
    let backup = custom_components.join(BACKUP_NAME);
    if let Err(e) = std::fs::create_dir_all(&custom_components) {
        return InstallOutcome::Failed(format!(
            "could not create {}: {e}",
            custom_components.display()
        ));
    }
    if let Err(e) = clear_if_present(&staging) {
        return InstallOutcome::Failed(format!("could not clear {}: {e}", staging.display()));
    }
    if let Err(e) = clear_if_present(&backup) {
        return InstallOutcome::Failed(format!("could not clear {}: {e}", backup.display()));
    }
    if let Err(e) = copy_dir(src, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return InstallOutcome::Failed(format!("could not stage the integration: {e}"));
    }

    // The swap itself is two renames, each atomic within `custom_components/`.
    // A crash between them leaves the target simply absent rather than
    // half-deleted, so the next start passes the ownership guard above and
    // installs fresh instead of refusing forever. Do not collapse this back
    // into a delete-then-rename: `remove_dir_all` over a multi-file directory
    // is not atomic, and a partial failure there leaves a directory that
    // exists but no longer has byonk's manifest -- refused permanently.
    if target.exists() {
        if let Err(e) = std::fs::rename(&target, &backup) {
            let _ = std::fs::remove_dir_all(&staging);
            return InstallOutcome::Failed(format!(
                "could not set aside the existing {}: {e}",
                target.display()
            ));
        }
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        // Put the original back so the user is not left without any
        // integration at all. Best effort: if this also fails there is
        // nothing more we can safely do here.
        let _ = std::fs::rename(&backup, &target);
        return InstallOutcome::Failed(format!("could not move the integration into place: {e}"));
    }

    // The swap is complete; the old version is no longer needed. A failure to
    // delete it is not a failure of the install -- the new integration is
    // already in place -- so this is best effort and unreported.
    let _ = std::fs::remove_dir_all(&backup);

    InstallOutcome::Installed {
        from: installed,
        to: ours,
    }
}

/// Supervisor's in-container base URL. `BYONK_SUPERVISOR_URL` overrides it.
const DEFAULT_SUPERVISOR_URL: &str = "http://supervisor";

/// Notification id, so a repeated notification replaces the previous one
/// instead of stacking up.
const NOTIFICATION_ID: &str = "byonk_integration";

/// Base URL for Supervisor API calls.
pub fn supervisor_url() -> String {
    std::env::var("BYONK_SUPERVISOR_URL").unwrap_or_else(|_| DEFAULT_SUPERVISOR_URL.to_string())
}

/// Ask the user to restart Home Assistant, through Supervisor's Core API proxy.
///
/// Reachable because the app declares `homeassistant_api: true`; the proxy
/// blocks only `hassio*` paths (`supervisor/api/proxy.py`).
pub async fn notify_restart(
    client: &reqwest::Client,
    token: &str,
    from: Option<&str>,
    to: &str,
) -> anyhow::Result<()> {
    let message = match from {
        Some(previous) => format!(
            "Byonk updated its Home Assistant integration from {previous} to {to}. \
             Restart Home Assistant to load it."
        ),
        None => format!(
            "Byonk installed its Home Assistant integration ({to}). \
             Restart Home Assistant to finish setting up Byonk."
        ),
    };
    client
        .post(format!(
            "{}/core/api/services/persistent_notification/create",
            supervisor_url()
        ))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "notification_id": NOTIFICATION_ID,
            "title": "Byonk",
            "message": message,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Tell Supervisor the byonk service is here, which makes Home Assistant offer
/// a Discovered card for the integration.
///
/// Supervisor stores the message and dedupes on (app, service), so calling this
/// on every start is harmless; if Home Assistant is down the message is kept and
/// replayed when it starts (`supervisor/discovery/__init__.py`).
pub async fn announce_discovery(client: &reqwest::Client, token: &str) -> anyhow::Result<()> {
    client
        .post(format!("{}/discovery", supervisor_url()))
        .bearer_auth(token)
        .json(&serde_json::json!({ "service": "byonk", "config": {} }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
