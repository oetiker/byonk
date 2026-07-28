//! A screen repo containing a symlink that points outside its root must not
//! serve the target's contents. Unix-only: symlink creation on Windows needs
//! elevated privileges and byonk targets Linux/macOS.
#![cfg(unix)]

use byonk::assets::AssetLoader;
use byonk::services::screen_repo_loader::{LocalScreenRepoSource, ScreenRepoSource};

/// Build a minimal writable repo at `root` with one real screen, plus a
/// symlink `leak.txt` pointing at `secret` outside the repo.
fn fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("hello")).unwrap();
    std::fs::write(
        repo.join("byonk-screens.yaml"),
        "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("hello/meta.yaml"),
        "title: Hello\ndescription: d\nbyonk: \"0.17\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("hello/script.lua"), "return {}\n").unwrap();
    std::fs::write(repo.join("hello/screen.svg"), "<svg/>\n").unwrap();

    let secret = dir.join("secret.txt");
    std::fs::write(&secret, "TOP SECRET").unwrap();
    std::os::unix::fs::symlink(&secret, repo.join("leak.txt")).unwrap();
    std::os::unix::fs::symlink(&secret, repo.join("hello/leak.txt")).unwrap();
    repo
}

#[test]
fn test_symlink_escaping_repo_root_is_not_readable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture(tmp.path());
    let src = LocalScreenRepoSource::load(&repo).expect("load repo");

    assert!(
        src.read("leak.txt").is_none(),
        "symlink at the repo root leaked its target"
    );
    assert!(
        src.read("hello/leak.txt").is_none(),
        "symlink inside a screen dir leaked its target"
    );
}

#[test]
fn test_real_files_still_read() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture(tmp.path());
    let src = LocalScreenRepoSource::load(&repo).expect("load repo");

    let bytes = src.read("hello/script.lua").expect("real file must read");
    assert_eq!(bytes, b"return {}\n");
}

#[test]
fn test_symlink_staying_inside_repo_still_reads() {
    // Escape is the thing being blocked — an internal symlink is legitimate
    // (e.g. a shared asset linked into two screens) and must keep working.
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture(tmp.path());
    std::os::unix::fs::symlink(repo.join("hello/script.lua"), repo.join("hello/alias.lua"))
        .unwrap();
    let src = LocalScreenRepoSource::load(&repo).expect("load repo");

    assert_eq!(
        src.read("hello/alias.lua")
            .expect("internal symlink must read"),
        b"return {}\n"
    );
}

#[test]
fn test_symlink_escaping_screens_dir_overlay_falls_through_to_embedded() {
    // `AssetLoader::read_screen`'s `SCREENS_DIR` overlay is a separate,
    // hand-written guard (not `read_within`) because it returns
    // `io::Result<Cow<...>>` rather than `Option<Vec<u8>>`. Prove it
    // independently: a symlink planted at a path that also exists embedded
    // (`default/script.lua`) must not leak its target — the read must fall
    // through to the embedded content instead.
    let tmp = tempfile::tempdir().unwrap();
    let screens_dir = tmp.path().join("screens");
    std::fs::create_dir_all(screens_dir.join("default")).unwrap();

    let secret = tmp.path().join("secret.txt");
    std::fs::write(&secret, "TOP SECRET").unwrap();
    std::os::unix::fs::symlink(&secret, screens_dir.join("default/script.lua")).unwrap();

    let loader = AssetLoader::new(Some(screens_dir), None, None);
    let rel = std::path::Path::new("default/script.lua");

    let bytes = loader.read_screen(rel).expect("read_screen must succeed");
    assert_ne!(
        &*bytes, b"TOP SECRET",
        "symlink in SCREENS_DIR overlay leaked its target"
    );

    let embedded = loader
        .read_screen_embedded_only(rel)
        .expect("embedded default/script.lua must exist");
    assert_eq!(
        &*bytes, &*embedded,
        "refused overlay read must fall through to the embedded screen"
    );
}
