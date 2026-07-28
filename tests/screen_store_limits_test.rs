//! `validate` and `read_file` must refuse oversized files without first
//! loading them into memory, and must distinguish missing / too-large /
//! not-UTF-8 in their reports.

mod common;

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
