//! `list_screens` reports writability structurally, and `delete_file`
//! refuses to strip a screen of the three files that define it.

mod common;

use byonk::services::screen_store::StoreError;
use common::store::{build_store, build_store_with_readonly_local};

#[test]
fn test_list_screens_marks_builtin_read_only_and_local_writable() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);

    let all = store.list_screens();

    let builtin = all
        .iter()
        .find(|e| e.screen_ref == "byonk-builtin/default")
        .expect("builtin default must be listed");
    assert!(!builtin.writable, "byonk-builtin must never be writable");

    let local = all
        .iter()
        .find(|e| e.screen_ref == "local/clock")
        .expect("local/clock must be listed");
    assert!(local.writable, "a local repo screen must be writable");
    assert!(local.files.iter().any(|f| f == "script.lua"));
}

/// Writability is a structural property of the resolved source
/// (`writable_root().is_some()`), not of the handle's name — a buggy
/// `writable = (handle == "local")` shortcut would pass every other test in
/// this file, since they only ever see `local` as an actually-writable
/// handle. `RESERVED_HANDLES` (which would otherwise stop a `local` entry
/// from being redefined) is enforced only in the HA add-on's options parsing,
/// not by `ScreenRepoManager` — a plain `config.yaml` really can register
/// `local` against a read-only (git-fetched) source. Pin the correct
/// behaviour down for that case too.
#[test]
fn test_list_screens_marks_a_readonly_local_handle_not_writable() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store_with_readonly_local(tmp.path());

    let all = store.list_screens();

    let builtin = all
        .iter()
        .find(|e| e.screen_ref == "byonk-builtin/default")
        .expect("builtin default must still be listed");
    assert!(!builtin.writable);

    // No screens exist under this read-only `local` fixture, but the write
    // path itself must still refuse — list_screens has nothing to assert on
    // otherwise, since a repo with zero screens lists no `local/...` entries.
    let err = store
        .write_file("local/anything", "meta.yaml", b"x", None)
        .unwrap_err();
    assert!(
        matches!(err, StoreError::ReadOnly { .. }),
        "a `local` handle resolving to a read-only source must refuse writes: {err:?}"
    );
}

#[test]
fn test_delete_file_removes_a_sibling_asset() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);
    store
        .write_file("local/clock", "notes.txt", b"scratch", None)
        .unwrap();

    store.delete_file("local/clock", "notes.txt").unwrap();

    assert!(matches!(
        store.read_file("local/clock", "notes.txt"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn test_delete_file_refuses_the_three_defining_files() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);

    for f in ["meta.yaml", "script.lua", "screen.svg"] {
        let err = store.delete_file("local/clock", f);
        assert!(
            err.is_err(),
            "deleting {f} must be refused — it defines the screen"
        );
    }
    // …and the screen is still intact afterwards.
    assert!(store.read_file("local/clock", "meta.yaml").is_ok());
}

#[test]
fn test_delete_file_on_a_read_only_handle_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);

    match store.delete_file("byonk-builtin/default", "script.lua") {
        Err(StoreError::ReadOnly { copy_hint }) => {
            assert!(copy_hint.contains("copy_screen"));
        }
        other => panic!("expected ReadOnly, got {other:?}"),
    }
}
