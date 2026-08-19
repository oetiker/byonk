//! Tests for writing byonk's HA integration into the Home Assistant config dir.
//!
//! This code deletes a directory inside the user's Home Assistant configuration,
//! so the refusal cases matter as much as the happy path.

use std::path::Path;

use byonk::ha_integration::{install, InstallOutcome};
use tempfile::TempDir;

/// Build a fake image-side integration source: manifest plus one extra file,
/// so a test can tell a real copy from a manifest-only one.
fn make_src(dir: &Path, version: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(r#"{{"domain": "byonk", "name": "Byonk", "version": "{version}"}}"#),
    )
    .unwrap();
    std::fs::write(dir.join("__init__.py"), "# byonk\n").unwrap();
    std::fs::create_dir_all(dir.join("brand")).unwrap();
    std::fs::write(dir.join("brand/icon.png"), b"not-really-a-png").unwrap();
}

fn target_of(ha_config: &Path) -> std::path::PathBuf {
    ha_config.join("custom_components").join("byonk")
}

#[test]
fn writes_the_integration_when_none_is_installed() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(&outcome, InstallOutcome::Installed { from: None, to } if to == "0.18.0"),
        "got {outcome:?}"
    );
    let target = target_of(ha.path());
    assert!(target.join("__init__.py").is_file());
    assert!(
        target.join("brand/icon.png").is_file(),
        "subdirectories are copied"
    );
}

#[test]
fn does_nothing_when_the_installed_version_matches() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");
    install(src_dir.path(), ha.path());

    // A file the copy would not have produced; it must survive a no-op.
    let marker = target_of(ha.path()).join("__pycache__marker");
    std::fs::write(&marker, "x").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(outcome, InstallOutcome::NotNeeded),
        "got {outcome:?}"
    );
    assert!(marker.is_file(), "a no-op must not touch the directory");
}

#[test]
fn replaces_the_integration_when_the_version_differs() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");
    install(src_dir.path(), ha.path());
    let stale = target_of(ha.path()).join("gone.py");
    std::fs::write(&stale, "old").unwrap();

    make_src(src_dir.path(), "0.19.0");
    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(&outcome, InstallOutcome::Installed { from: Some(f), to } if f == "0.18.0" && to == "0.19.0"),
        "got {outcome:?}"
    );
    assert!(
        !stale.exists(),
        "a replaced install must not keep stale files"
    );
    let manifest = std::fs::read_to_string(target_of(ha.path()).join("manifest.json")).unwrap();
    assert!(manifest.contains("0.19.0"));
}

#[test]
fn refuses_a_directory_that_is_not_byonks() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let target = target_of(ha.path());
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("manifest.json"),
        r#"{"domain": "something_else", "version": "1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(target.join("precious.py"), "keep me").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(outcome, InstallOutcome::Refused(_)),
        "got {outcome:?}"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("precious.py")).unwrap(),
        "keep me",
        "a refusal must change nothing"
    );
}

#[test]
fn refuses_a_foreign_directory_even_when_its_version_string_matches_ours() {
    // Ownership must be checked before the version short-circuit. If the
    // version check ran first, a foreign directory that happens to carry our
    // version string would compare equal and be reported as "already up to
    // date" — silently leaving a directory that isn't byonk's in place,
    // without ever warning the user it's standing in the way.
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let target = target_of(ha.path());
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("manifest.json"),
        r#"{"domain": "something_else", "version": "0.18.0"}"#,
    )
    .unwrap();
    std::fs::write(target.join("precious.py"), "keep me").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(outcome, InstallOutcome::Refused(_)),
        "got {outcome:?}"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("precious.py")).unwrap(),
        "keep me",
        "a refusal must change nothing"
    );
}

#[test]
fn refuses_a_directory_with_no_manifest_at_all() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let target = target_of(ha.path());
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("random.txt"), "who knows").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(outcome, InstallOutcome::Refused(_)),
        "got {outcome:?}"
    );
    assert!(target.join("random.txt").is_file());
}

#[test]
fn reports_failure_when_the_config_dir_is_absent() {
    let src_dir = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let outcome = install(src_dir.path(), Path::new("/no/such/ha/config"));

    assert!(
        matches!(outcome, InstallOutcome::Failed(_)),
        "got {outcome:?}"
    );
}

#[test]
fn reports_failure_when_the_image_has_no_integration() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(outcome, InstallOutcome::Failed(_)),
        "got {outcome:?}"
    );
    assert!(!target_of(ha.path()).exists());
}

#[test]
fn clears_a_leftover_staging_dir_from_a_crashed_run() {
    let src_dir = TempDir::new().unwrap();
    let ha = TempDir::new().unwrap();
    make_src(src_dir.path(), "0.18.0");

    let staging = ha.path().join("custom_components").join(".byonk-new");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("junk.py"), "half-written").unwrap();

    let outcome = install(src_dir.path(), ha.path());

    assert!(
        matches!(outcome, InstallOutcome::Installed { .. }),
        "got {outcome:?}"
    );
    assert!(!staging.exists(), "staging is consumed by the swap");
    assert!(!target_of(ha.path()).join("junk.py").exists());
}
