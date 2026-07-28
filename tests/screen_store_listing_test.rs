//! `list_screens` reports writability structurally, and `delete_file`
//! refuses to strip a screen of the three files that define it.

mod common;

use byonk::services::screen_store::StoreError;
use common::store::build_store;

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
