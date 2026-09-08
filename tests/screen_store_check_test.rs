//! `ScreenStore::check_text` (draft validation, no on-disk write required),
//! `ScreenStore::list_files`, and the line-number extraction (`error_line`)
//! that both `check_text` and `validate` share via `check_one`.

mod common;
use byonk::services::screen_store::{error_line, Severity};
use common::store::build_store;

#[test]
fn error_line_parses_lua_and_tera_forms() {
    assert_eq!(
        error_line(r#"syntax error: [string "script.lua"]:6: '}' expected"#),
        Some(6)
    );
    assert_eq!(
        error_line("Failed to parse 'screen.svg'\n --> 3:5\n  |"),
        Some(3)
    );
    assert_eq!(error_line("no numbers here"), None);
}

#[test]
fn check_text_reports_lua_syntax_error_with_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    let r = store.check_text("local/clock", "script.lua", "return {\n  a = 1\nend\n");
    assert!(!r.ok);
    assert_eq!(r.issues[0].location, "script.lua");
    assert_eq!(r.issues[0].severity, Severity::Error);
    assert_eq!(r.issues[0].line, Some(3));
}

#[test]
fn check_text_ok_for_valid_lua_and_unknown_kind() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    assert!(
        store
            .check_text("local/clock", "script.lua", "return {}")
            .ok
    );
    assert!(store.check_text("local/clock", "notes.txt", "anything").ok);
}

#[test]
fn check_text_reports_tera_error_with_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    let r = store.check_text("local/clock", "screen.svg", "<svg>\n{% if %}\n</svg>");
    assert!(!r.ok);
    assert_eq!(r.issues[0].location, "screen.svg");
    assert_eq!(r.issues[0].line, Some(2));
}

#[test]
fn check_text_and_validate_agree() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    store
        .write_file("local/clock", "script.lua", b"return {\nend\n", None)
        .unwrap();
    let v = store.validate("local/clock");
    let c = store.check_text("local/clock", "script.lua", "return {\nend\n");
    assert_eq!(v.issues.len(), 1);
    assert_eq!(v.issues[0].message, c.issues[0].message);
    assert_eq!(v.issues[0].line, c.issues[0].line);
}

#[test]
fn list_files_marks_text_and_binary() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    store
        .write_file("local/clock", "extra.svg", b"<svg/>", None)
        .unwrap();
    std::fs::write(
        dir.path().join("local/clock/logo.png"),
        [0x89, b'P', b'N', b'G', 0xff, 0xfe],
    )
    .unwrap();
    let files = store.list_files("local/clock").unwrap();
    let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "meta.yaml",
            "script.lua",
            "screen.svg",
            "extra.svg",
            "logo.png"
        ]
    );
    assert!(!files.iter().find(|f| f.name == "logo.png").unwrap().text);
    assert!(files.iter().find(|f| f.name == "extra.svg").unwrap().text);
    assert_eq!(
        files.iter().find(|f| f.name == "extra.svg").unwrap().size,
        Some(6)
    );
}

#[test]
fn list_files_unknown_ref_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    assert!(matches!(
        store.list_files("local/nope"),
        Err(byonk::services::screen_store::StoreError::NotFound)
    ));
}
