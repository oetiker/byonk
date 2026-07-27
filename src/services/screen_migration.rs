//! One-time migration of pre-existing installs: older byonk versions copied
//! their builtin screens into `SCREENS_DIR` and presented that directory
//! under the `byonk-builtin` handle. The current design makes `byonk-builtin`
//! a minimal, read-only, embedded-only repo (`default` + `calibration/*`)
//! and gives `SCREENS_DIR` its own handle, `local`. This module repairs an
//! upgraded install: it rewrites the leftover `SCREENS_DIR/byonk-screens.yaml`
//! manifest's `name: byonk-builtin` to `name: local`, and rewrites device
//! refs `byonk-builtin/<x>` to `local/<x>` — but only where `<x>` is actually
//! a screen directory present in `SCREENS_DIR` (a genuine `byonk-builtin/default`
//! or `byonk-builtin/calibration/*` ref lives embedded, is never present on
//! disk, and is left untouched).
//!
//! Called once at startup, after asset seeding and before the server starts
//! serving. A failure here must never prevent the server from starting: every
//! step is independently fallible and warn-logged, mirroring
//! `AssetLoader::seed_if_configured`.

use crate::services::config_writer;
use crate::services::screen_repo_loader::walk_screen_paths;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Result of a `migrate_builtin_overlay_to_local` call.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub manifest_rewritten: bool,
    pub refs_rewritten: usize,
}

/// Migrate a pre-existing install's `SCREENS_DIR` and (optionally) its
/// `config.yaml` from the old `byonk-builtin` overlay model to the new
/// `local` handle. Idempotent: a second call on already-migrated state
/// reports no changes. Never errors — every failure is logged and the
/// migration simply does as much as it safely can.
pub fn migrate_builtin_overlay_to_local(
    screens_dir: &Path,
    config_path: Option<&Path>,
) -> MigrationReport {
    let manifest_rewritten = rewrite_manifest(screens_dir);

    let present = present_screen_dirs(screens_dir);
    let refs_rewritten = match config_path {
        Some(cfg_path) => rewrite_device_refs(cfg_path, &present),
        None => 0,
    };

    let report = MigrationReport {
        manifest_rewritten,
        refs_rewritten,
    };

    if report.manifest_rewritten || report.refs_rewritten > 0 {
        tracing::info!(
            manifest_rewritten = report.manifest_rewritten,
            refs_rewritten = report.refs_rewritten,
            "Migrated pre-existing byonk-builtin overlay screens to the 'local' handle"
        );
    }

    report
}

/// Rewrite `SCREENS_DIR/byonk-screens.yaml`'s `name:` from `byonk-builtin` to
/// `local`, preserving all other formatting/comments via `config_writer::set_scalar`.
/// Returns whether a rewrite happened. Missing/unreadable/malformed manifests,
/// or ones that already say anything other than `byonk-builtin`, are left
/// alone (not an error — just nothing to do).
fn rewrite_manifest(screens_dir: &Path) -> bool {
    let manifest_path = screens_dir.join("byonk-screens.yaml");
    let Ok(content) = fs::read_to_string(&manifest_path) else {
        return false;
    };

    let name = serde_yaml::from_str::<serde_yaml::Value>(&content)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string));
    if name.as_deref() != Some("byonk-builtin") {
        return false;
    }

    match config_writer::set_scalar(&content, &["name"], "local".into()) {
        Ok(updated) => match fs::write(&manifest_path, updated) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    path = %manifest_path.display(),
                    error = %e,
                    "Failed to write migrated byonk-screens.yaml manifest"
                );
                false
            }
        },
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "Failed to rewrite byonk-screens.yaml manifest name during migration"
            );
            false
        }
    }
}

/// Screen directories (relative paths, `/`-separated) present under
/// `screens_dir` — anything containing a `meta.yaml`, nested or not. Empty if
/// `screens_dir` doesn't exist. Shares `screen_repo_loader`'s walk so "what
/// counts as a screen" can't drift between the two modules.
fn present_screen_dirs(screens_dir: &Path) -> HashSet<String> {
    walk_screen_paths(screens_dir).into_iter().collect()
}

/// Rewrite every device's `screen: byonk-builtin/<x>` to `screen: local/<x>`
/// in `config_path`, but only where `<x>` is in `present`. Returns the number
/// of refs actually rewritten (and persisted). A missing/unreadable/malformed
/// config, or a write failure, yields `0` — this is a best-effort fixup, not
/// a hard requirement for startup.
fn rewrite_device_refs(config_path: &Path, present: &HashSet<String>) -> usize {
    let Ok(content) = fs::read_to_string(config_path) else {
        return 0;
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
        tracing::warn!(
            path = %config_path.display(),
            "Failed to parse config.yaml during migration; skipping device ref rewrite"
        );
        return 0;
    };
    let Some(devices) = value.get("devices").and_then(|d| d.as_mapping()) else {
        return 0;
    };

    let mut targets: Vec<(String, String)> = Vec::new();
    for (key, val) in devices {
        let Some(key_str) = key.as_str() else {
            continue;
        };
        let Some(screen_str) = val.get("screen").and_then(|s| s.as_str()) else {
            continue;
        };
        if let Some(suffix) = screen_str.strip_prefix("byonk-builtin/") {
            if present.contains(suffix) {
                targets.push((key_str.to_string(), suffix.to_string()));
            }
        }
    }
    if targets.is_empty() {
        return 0;
    }

    let mut current = content;
    let mut rewritten = 0usize;
    for (device_key, suffix) in &targets {
        let new_ref = serde_yaml::Value::from(format!("local/{suffix}"));
        match config_writer::set_scalar(&current, &["devices", device_key, "screen"], new_ref) {
            Ok(updated) => {
                current = updated;
                rewritten += 1;
            }
            Err(e) => {
                tracing::warn!(
                    device = %device_key,
                    error = %e,
                    "Failed to rewrite device screen ref during migration"
                );
            }
        }
    }

    if rewritten == 0 {
        return 0;
    }

    if let Err(e) = fs::write(config_path, &current) {
        tracing::warn!(
            path = %config_path.display(),
            error = %e,
            "Failed to write migrated config.yaml"
        );
        return 0;
    }

    rewritten
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_rewrites_manifest_and_user_screen_refs_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("myclock")).unwrap();
        std::fs::write(
            dir.path().join("myclock/meta.yaml"),
            "title: C\ndescription: d\nbyonk: \"0.15\"\n",
        )
        .unwrap();
        let cfg = dir.path().join("config.yaml");
        std::fs::write(&cfg,
            "devices:\n  AA:BB:\n    screen: byonk-builtin/myclock\n  CC:DD:\n    screen: byonk-builtin/default\n").unwrap();
        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        let manifest = std::fs::read_to_string(dir.path().join("byonk-screens.yaml")).unwrap();
        assert!(manifest.contains("name: local"));
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("local/myclock"), "user screen ref migrated");
        assert!(
            out.contains("byonk-builtin/default"),
            "genuine builtin ref untouched"
        );
        assert!(rep.refs_rewritten == 1);
        // idempotent
        let rep2 = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        assert_eq!(rep2.refs_rewritten, 0);
    }

    #[test]
    fn nested_user_screen_ref_is_migrated() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("byonk-screens.yaml"), "name: local\n").unwrap();
        std::fs::create_dir_all(dir.path().join("foo/bar")).unwrap();
        std::fs::write(dir.path().join("foo/bar/meta.yaml"), "title: T\n").unwrap();
        let cfg = dir.path().join("config.yaml");
        std::fs::write(
            &cfg,
            "devices:\n  AA:BB:\n    screen: byonk-builtin/foo/bar\n",
        )
        .unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        assert_eq!(rep.refs_rewritten, 1);
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("local/foo/bar"));
    }

    #[test]
    fn ref_whose_target_is_absent_from_screens_dir_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();
        // No `ghost` directory in SCREENS_DIR at all.
        let cfg = dir.path().join("config.yaml");
        std::fs::write(
            &cfg,
            "devices:\n  AA:BB:\n    screen: byonk-builtin/ghost\n",
        )
        .unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        assert_eq!(rep.refs_rewritten, 0);
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("byonk-builtin/ghost"), "stale ref left alone");
        // Manifest is still rewritten independently.
        assert!(rep.manifest_rewritten);
    }

    #[test]
    fn no_config_file_at_all_is_a_noop_for_refs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), None);
        assert_eq!(rep.refs_rewritten, 0);
        assert!(rep.manifest_rewritten);
    }

    #[test]
    fn missing_config_path_on_disk_is_a_noop_never_creates_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();
        let cfg = dir.path().join("does-not-exist.yaml");

        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        assert_eq!(rep.refs_rewritten, 0);
        assert!(!cfg.exists(), "migration must never create a config file");
    }

    #[test]
    fn already_local_manifest_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let original =
            "name: local\ndescription: Your own screens.\nauthor: you\nlicense: UNLICENSED\n";
        std::fs::write(dir.path().join("byonk-screens.yaml"), original).unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), None);
        assert!(!rep.manifest_rewritten);
        let manifest = std::fs::read_to_string(dir.path().join("byonk-screens.yaml")).unwrap();
        assert_eq!(manifest, original);
    }

    #[test]
    fn config_comments_and_formatting_survive_a_ref_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("myclock")).unwrap();
        std::fs::write(dir.path().join("myclock/meta.yaml"), "title: C\n").unwrap();

        let cfg = dir.path().join("config.yaml");
        let original = "\
# my config
registration:
  enabled: true   # inline comment
devices:
  \"AA:BB\":   # my clock
    screen: byonk-builtin/myclock
    params:
      foo: bar
# trailing comment
";
        std::fs::write(&cfg, original).unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        assert_eq!(rep.refs_rewritten, 1);
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("# my config"));
        assert!(out.contains("# inline comment"));
        assert!(out.contains("# my clock"));
        assert!(out.contains("# trailing comment"));
        assert!(out.contains("foo: bar"));
        assert!(out.contains("screen: local/myclock"));
    }
}
