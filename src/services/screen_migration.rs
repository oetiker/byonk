//! One-time migration of pre-existing installs: older byonk versions copied
//! their builtin screens into `SCREENS_DIR` and presented that directory
//! under the `byonk-builtin` handle. The current design makes `byonk-builtin`
//! a minimal, read-only, embedded-only repo (`default` + `calibration/*`)
//! and gives `SCREENS_DIR` its own handle, `local`. This module repairs an
//! upgraded install: it rewrites the leftover `SCREENS_DIR/byonk-screens.yaml`
//! manifest's `name: byonk-builtin` to `name: local`, and rewrites device
//! refs `byonk-builtin/<x>` to `local/<x>` — but only where `<x>` is actually
//! the *user's own* screen.
//!
//! That last condition is not simply "does `SCREENS_DIR/<x>/meta.yaml` exist":
//! a pre-Task-11 install's `SCREENS_DIR` genuinely contains on-disk copies of
//! `default/meta.yaml` and `calibration/{color,grey}/meta.yaml` (the old
//! `seed_if_configured` copied every embedded builtin screen there; that copy
//! loop was removed in commit `1f05a18`). Those leftover copies are exactly
//! the paths `byonk-builtin/<x>` still refers to today (embedded), so a ref
//! pointing at one of them must be left alone even though the same-named
//! directory exists on disk — rewriting it would permanently pin a device to
//! a frozen, no-longer-maintained copy, and break outright the moment the
//! user cleans out the (now redundant) leftover copy. So "present" is
//! computed as: every `SCREENS_DIR` directory with a `meta.yaml`, *minus*
//! whatever paths the embedded `byonk-builtin` repo itself still serves.
//! Leftover copies of anything else (e.g. old `demo/…`/`example/…` screens)
//! are not served by any embedded repo, so they count as present and do
//! migrate.
//!
//! Called once at startup, after asset seeding and before the server starts
//! serving. A failure here must never prevent the server from starting: every
//! step is independently fallible and warn-logged, mirroring
//! `AssetLoader::seed_if_configured`.

use crate::assets::{AssetCategory, AssetLoader};
use crate::services::config_writer;
use crate::services::screen_repo_loader::walk_screen_paths;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Result of a `migrate_builtin_overlay_to_local` call.
#[derive(Debug)]
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
/// `local`, preserving all other formatting/comments via `config_writer::replace_scalar`.
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

    match config_writer::replace_scalar(&content, &["name"], "local".into()) {
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

/// Screen directories (relative paths, `/`-separated) that are the *user's
/// own* — present under `screens_dir` (anything containing a `meta.yaml`,
/// nested or not; shares `screen_repo_loader`'s walk so "what counts as a
/// screen" can't drift between the two modules) and NOT a path the embedded
/// `byonk-builtin` repo still serves. Empty if `screens_dir` doesn't exist.
fn present_screen_dirs(screens_dir: &Path) -> HashSet<String> {
    let builtin = builtin_screen_paths();
    walk_screen_paths(screens_dir)
        .into_iter()
        .filter(|p| !builtin.contains(p))
        .collect()
}

/// Screen directory paths (relative, `/`-separated: `"default"`,
/// `"calibration/color"`, `"calibration/grey"`) the embedded `byonk-builtin`
/// repo serves today — i.e. paths a `byonk-builtin/<x>` ref can still
/// legitimately resolve through, regardless of whether a leftover on-disk
/// copy also exists in `SCREENS_DIR`. Derived from `AssetLoader::list_embedded`
/// (the raw embedded file list, unaffected by any `SCREENS_DIR` overlay) —
/// never from a merged/overlaid view, which would defeat the exclusion.
fn builtin_screen_paths() -> HashSet<String> {
    AssetLoader::list_embedded(AssetCategory::Screens)
        .into_iter()
        .filter_map(|p| p.strip_suffix("/meta.yaml").map(str::to_string))
        .collect()
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
        match config_writer::replace_scalar(&current, &["devices", device_key, "screen"], new_ref) {
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
        // idempotent: a second run changes nothing at all, manifest included
        // (not just refs — the manifest already says `local` by now).
        let rep2 = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        assert_eq!(rep2.refs_rewritten, 0);
        assert!(!rep2.manifest_rewritten);
    }

    /// The Critical fix this round: a real upgraded install's `SCREENS_DIR`
    /// contains on-disk copies of `default/meta.yaml` and
    /// `calibration/grey/meta.yaml` (the pre-`1f05a18` seeder copied every
    /// embedded builtin screen there). Refs to those must survive verbatim —
    /// rewriting them would permanently pin devices to a frozen copy that
    /// breaks the moment the user cleans it out — while a ref to the user's
    /// own screen alongside them still migrates.
    #[test]
    fn builtin_default_and_calibration_refs_survive_even_with_leftover_copies_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();

        // Simulate the pre-Task-11 leftover: the old seeder's copy of the
        // embedded builtin screens, still sitting in SCREENS_DIR.
        std::fs::create_dir_all(dir.path().join("default")).unwrap();
        std::fs::write(dir.path().join("default/meta.yaml"), "title: D\n").unwrap();
        std::fs::create_dir_all(dir.path().join("calibration/grey")).unwrap();
        std::fs::write(dir.path().join("calibration/grey/meta.yaml"), "title: G\n").unwrap();

        // The user's own screen, alongside the leftover copies.
        std::fs::create_dir_all(dir.path().join("myclock")).unwrap();
        std::fs::write(dir.path().join("myclock/meta.yaml"), "title: C\n").unwrap();

        let cfg = dir.path().join("config.yaml");
        std::fs::write(
            &cfg,
            "devices:\n  AA:BB:\n    screen: byonk-builtin/default\n  CC:DD:\n    screen: byonk-builtin/calibration/grey\n  EE:FF:\n    screen: byonk-builtin/myclock\n",
        )
        .unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(
            out.contains("byonk-builtin/default"),
            "genuine default ref must survive despite the leftover copy on disk:\n{out}"
        );
        assert!(
            out.contains("byonk-builtin/calibration/grey"),
            "genuine calibration ref must survive despite the leftover copy on disk:\n{out}"
        );
        assert!(
            out.contains("local/myclock"),
            "the user's own screen must still migrate:\n{out}"
        );
        assert_eq!(rep.refs_rewritten, 1);
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

    /// Resolution 2 also names "an unwritable [config]" as a do-nothing case,
    /// not an error. Pre-creates a config with a matching ref, then makes it
    /// unwritable (`chmod 0o400`) so the rewrite is computed in memory but the
    /// final `fs::write` fails. Gated `#[cfg(unix)]` and assumes tests run as
    /// a non-root user (root bypasses the permission bit) — same assumption
    /// already made by `assets.rs`'s
    /// `test_seed_if_configured_examples_cleans_up_after_mid_loop_failure_and_retries_next_start`
    /// and `screen_store.rs`'s existing `#[cfg(unix)]` permission tests.
    #[test]
    #[cfg(unix)]
    fn unwritable_config_is_a_noop_not_an_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("byonk-screens.yaml"),
            "name: byonk-builtin\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("myclock")).unwrap();
        std::fs::write(dir.path().join("myclock/meta.yaml"), "title: C\n").unwrap();

        let cfg = dir.path().join("config.yaml");
        let original = "devices:\n  AA:BB:\n    screen: byonk-builtin/myclock\n";
        std::fs::write(&cfg, original).unwrap();
        std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o400)).unwrap();

        let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));

        // Restore write permission before the tempdir's Drop cleans it up.
        std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert_eq!(
            rep.refs_rewritten, 0,
            "a write failure must be reported as nothing rewritten, not silently claimed"
        );
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(
            out, original,
            "content must be untouched on a write failure"
        );
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
