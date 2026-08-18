//! `validate` and `read_file` must refuse oversized files without first
//! loading them into memory, and must distinguish missing / too-large /
//! not-UTF-8 in their reports.

mod common;

use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::screen_repo_loader::{LocalScreenRepoSource, ReadOutcome, ScreenRepoSource};
use common::store::build_store;

const MAX: usize = 5 * 1024 * 1024;

fn repo_with(dir: &std::path::Path, file: &str, bytes: &[u8]) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("big")).unwrap();
    std::fs::write(
        repo.join("byonk-screens.yaml"),
        "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("big/meta.yaml"),
        "title: Big\ndescription: d\nbyonk: \"0.17\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("big/screen.svg"), "<svg/>\n").unwrap();
    std::fs::write(repo.join(file), bytes).unwrap();
    repo
}

#[test]
fn test_read_limited_reports_too_large_for_oversized_file() {
    let tmp = tempfile::tempdir().unwrap();
    let oversized = vec![b'x'; MAX + 1];
    let repo = repo_with(tmp.path(), "big/script.lua", &oversized);
    let src = LocalScreenRepoSource::load(&repo).unwrap();

    assert!(matches!(
        src.read_limited("big/script.lua", MAX),
        ReadOutcome::TooLarge
    ));
}

#[test]
fn test_read_limited_reports_missing_for_absent_file() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with(tmp.path(), "big/script.lua", b"return {}\n");
    let src = LocalScreenRepoSource::load(&repo).unwrap();

    assert!(matches!(
        src.read_limited("big/nope.lua", MAX),
        ReadOutcome::Missing
    ));
}

#[test]
fn test_read_limited_returns_bytes_under_the_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with(tmp.path(), "big/script.lua", b"return {}\n");
    let src = LocalScreenRepoSource::load(&repo).unwrap();

    match src.read_limited("big/script.lua", MAX) {
        ReadOutcome::Found(b) => assert_eq!(b, b"return {}\n"),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn test_read_limited_respects_the_symlink_guard() {
    // The cap must not become a way around Task 1's escape check.
    #[cfg(unix)]
    {
        let tmp = tempfile::tempdir().unwrap();
        let repo = repo_with(tmp.path(), "big/script.lua", b"return {}\n");
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "TOP SECRET").unwrap();
        std::os::unix::fs::symlink(&secret, repo.join("big/leak.txt")).unwrap();
        let src = LocalScreenRepoSource::load(&repo).unwrap();

        assert!(matches!(
            src.read_limited("big/leak.txt", MAX),
            ReadOutcome::Missing
        ));
    }
}

#[test]
fn test_read_limited_is_stat_first_not_read_first() {
    // Discriminates a stat-first cap (correct) from a read-then-check cap
    // (the trait default) in the one case where they diverge: chmod 000
    // makes `stat` succeed — size is metadata, not content — but `read`
    // fail with EACCES. A stat-first cap sees the size from `metadata()`
    // and returns `TooLarge` without ever attempting the read; a
    // read-then-check cap would attempt the read, get EACCES, and report
    // `Missing` instead. The other tests in this file can't tell the two
    // implementations apart — this one can.
    //
    // `libc` is not a dependency of this crate, so this doesn't guard
    // against running as root (which ignores permission bits, making
    // `read` succeed and changing the expected outcome to `Found`).
    // Assumed not to run as root.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let oversized = vec![b'x'; MAX + 1];
        let repo = repo_with(tmp.path(), "big/script.lua", &oversized);
        let target = repo.join("big/script.lua");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
        let src = LocalScreenRepoSource::load(&repo).unwrap();

        let outcome = src.read_limited("big/script.lua", MAX);

        // Restore permissions before asserting, so a failed assertion
        // doesn't leave an unreadable file behind for anything downstream.
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(
            matches!(outcome, ReadOutcome::TooLarge),
            "expected TooLarge (stat succeeds despite chmod 000, proving the \
             size is checked before any read is attempted); got {outcome:?} \
             — a read-then-check implementation would report Missing (EACCES) instead"
        );
    }
}

#[test]
fn test_validate_reports_size_cap_for_screens_dir_overlay_file() {
    // `EmbeddedBuiltinSource::read` deliberately consults the `SCREENS_DIR`
    // filesystem overlay — a user-writable directory (Samba share, HA
    // `/config/screens`) — so `byonk-builtin` is not exempt from the same
    // unbounded-read risk as the disk-backed screen repos above.
    //
    // A plain oversized file here is NOT enough to discriminate the fix: a
    // read-then-check default still arrives at the correct `TooLarge`
    // outcome (it just wastefully reads the whole file first — a property
    // this test can't observe from the outside). So, same trick as
    // `test_read_limited_is_stat_first_not_read_first`: chmod 000 makes
    // `stat` succeed (size is metadata) but a full `read` fail (EACCES). A
    // stat-first `read_limited` sees the size and reports `TooLarge`; a
    // read-then-check implementation attempts the read, gets EACCES, folds
    // that into "not found" — reproducing this task's very first bug
    // (finding A), just reached through the overlay this time instead of a
    // disk-backed `ScreenRepoSource`.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let screens_dir = tmp.path().join("screens");
        std::fs::create_dir_all(screens_dir.join("default")).unwrap();
        let script = screens_dir.join("default/script.lua");
        std::fs::write(&script, vec![b'x'; MAX + 1]).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o000)).unwrap();
        let config_path = tmp.path().join("config.yaml");
        std::fs::write(
            &config_path,
            "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n",
        )
        .unwrap();

        let asset_loader = Arc::new(AssetLoader::new(Some(screens_dir), None, Some(config_path)));
        let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        let state = create_app_state_with_config(asset_loader, config).expect("create app state");

        let report = state.screen_store.validate("byonk-builtin/default");

        // Restore permissions before any assertion can panic.
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();

        let issue = report
            .issues
            .iter()
            .find(|i| i.location == "script.lua")
            .expect("script.lua must be flagged");
        assert!(
            issue.message.contains("exceeds"),
            "expected the size-cap message (stat succeeds despite chmod 000); \
             got: {}",
            issue.message
        );
        assert!(
            !issue.message.contains("not found")
                && !issue.message.to_lowercase().contains("syntax"),
            "an oversized overlay file must not be reported as missing or as a \
             Lua syntax error, got: {}",
            issue.message
        );
    }
}

#[test]
fn test_validate_reports_oversized_file_distinctly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["big"]);
    // Overwrite the scaffolded script with an oversized one.
    std::fs::write(tmp.path().join("local/big/script.lua"), vec![b'x'; MAX + 1]).unwrap();

    let report = store.validate("local/big");

    assert!(!report.ok, "an oversized script must fail validation");
    let issue = report
        .issues
        .iter()
        .find(|i| i.location == "script.lua")
        .expect("script.lua must be flagged");
    assert!(
        issue.message.contains("exceeds"),
        "must name the size cap, got: {}",
        issue.message
    );
    assert!(
        !issue.message.contains("not found"),
        "an oversized file must not be reported as missing, got: {}",
        issue.message
    );
}

#[test]
fn test_validate_reports_non_utf8_distinctly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["big"]);
    // Invalid UTF-8: lone continuation bytes.
    std::fs::write(
        tmp.path().join("local/big/script.lua"),
        [0x80u8, 0x80, 0x80],
    )
    .unwrap();

    let report = store.validate("local/big");

    let issue = report
        .issues
        .iter()
        .find(|i| i.location == "script.lua")
        .expect("script.lua must be flagged");
    assert!(
        issue.message.contains("UTF-8"),
        "a non-UTF-8 file must say so rather than 'file not found', got: {}",
        issue.message
    );
}
