//! Moving a screen from the dev UI's Edit pane (U9): `POST
//! /dev/api/screen/move` renames a screen inside its repo or moves it into
//! another writable one, and rewrites every device `screen:` in config.yaml
//! that named the old ref -- in the same request. The MCP `rename_screen`
//! tool goes through the same core.

mod common;

use axum::http::StatusCode;
use common::mcp::McpTestClient;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    app: TestApp,
    config: std::path::PathBuf,
}

fn repo(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("byonk-screens.yaml"),
        format!("name: {name}\ndescription: Test fixture.\nauthor: test\nlicense: MIT\n"),
    )
    .unwrap();
    dir
}

/// Scaffold `handle/name` through the dev API, as the UI's New does.
async fn screen(app: &TestApp, handle: &str, name: &str) {
    let r = app
        .post_json(
            "/dev/api/screen",
            &[AUTH],
            &serde_json::json!({ "handle": handle, "name": name }).to_string(),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
}

/// Two writable repos, `local` (holding `clock` and `other`) and `shared`
/// (empty); two devices on `local/clock`, one on `local/other`.
async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let local = repo(&root, "local");
    repo(&root, "shared");
    let yaml = format!(
        "\
admin:
  token: secret
devices:
  DEFAULT:
    screen: byonk-builtin/default
  # the kitchen display
  kitchen:
    screen: local/clock   # keep me
  hall:
    screen: local/clock
  study:
    screen: local/other
screen_repos:
  local:
    path: {}
  shared:
    path: {}
",
        local.display(),
        root.join("shared").display()
    );
    let (app, config) = TestApp::new_with_config_yaml(&yaml, &root);
    screen(&app, "local", "clock").await;
    screen(&app, "local", "other").await;
    Fixture {
        _dir: dir,
        root,
        app,
        config,
    }
}

fn doc(path: &std::path::Path) -> serde_yaml::Value {
    serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

async fn move_screen(
    f: &Fixture,
    from: &str,
    to_handle: &str,
    to_name: &str,
) -> common::app::TestResponse {
    f.app
        .post_json(
            "/dev/api/screen/move",
            &[AUTH],
            &serde_json::json!({ "ref": from, "to_handle": to_handle, "to_name": to_name })
                .to_string(),
        )
        .await
}

async fn refs(f: &Fixture) -> Vec<String> {
    let screens: serde_json::Value = f
        .app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();
    screens["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn a_move_within_its_repo_renames_and_rewrites_the_devices_on_it() {
    let f = fixture().await;
    let r = move_screen(&f, "local/clock", "local", "home/clock").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["ref"], "local/home/clock");
    assert_eq!(body["rewritten"], serde_json::json!(["hall", "kitchen"]));

    assert!(f.root.join("local/home/clock/meta.yaml").is_file());
    assert!(!f.root.join("local/clock").exists());
    let listed = refs(&f).await;
    assert!(
        listed.contains(&"local/home/clock".to_string()),
        "{listed:?}"
    );
    assert!(!listed.contains(&"local/clock".to_string()), "{listed:?}");

    let d = doc(&f.config);
    assert_eq!(d["devices"]["kitchen"]["screen"], "local/home/clock");
    assert_eq!(d["devices"]["hall"]["screen"], "local/home/clock");
    assert_eq!(d["devices"]["study"]["screen"], "local/other");
    assert_eq!(d["devices"]["DEFAULT"]["screen"], "byonk-builtin/default");
    let yaml = std::fs::read_to_string(&f.config).unwrap();
    assert!(yaml.contains("# the kitchen display"), "{yaml}");
    assert!(yaml.contains("# keep me"), "{yaml}");
}

#[tokio::test]
async fn a_move_across_repos_puts_the_files_there_and_removes_them_here() {
    let f = fixture().await;
    // An extra asset travels with the screen.
    let w = f
        .app
        .put_text(
            "/dev/api/file?ref=local/clock&name=logo.txt",
            &[AUTH, ("If-Match", "*")],
            "logo",
        )
        .await;
    assert_eq!(w.status, StatusCode::NO_CONTENT, "{}", w.text());
    let r = move_screen(&f, "local/clock", "shared", "clock").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["ref"], "shared/clock");
    assert_eq!(body["rewritten"], serde_json::json!(["hall", "kitchen"]));

    for file in ["meta.yaml", "script.lua", "screen.svg", "logo.txt"] {
        assert!(f.root.join("shared/clock").join(file).is_file(), "{file}");
    }
    assert!(!f.root.join("local/clock").exists());
    let listed = refs(&f).await;
    assert!(listed.contains(&"shared/clock".to_string()), "{listed:?}");
    assert!(!listed.contains(&"local/clock".to_string()), "{listed:?}");

    let d = doc(&f.config);
    assert_eq!(d["devices"]["kitchen"]["screen"], "shared/clock");
    assert_eq!(d["devices"]["hall"]["screen"], "shared/clock");
    assert_eq!(d["devices"]["study"]["screen"], "local/other");
}

#[tokio::test]
async fn a_move_of_an_unused_screen_leaves_config_yaml_alone() {
    let f = fixture().await;
    screen(&f.app, "local", "unused").await;
    let before = std::fs::read_to_string(&f.config).unwrap();
    let r = move_screen(&f, "local/unused", "local", "unused2").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(
        r.json::<serde_json::Value>()["rewritten"],
        serde_json::json!([])
    );
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), before);
}

#[tokio::test]
async fn a_move_into_or_out_of_a_read_only_repo_is_refused_and_changes_nothing() {
    let f = fixture().await;
    let before = std::fs::read_to_string(&f.config).unwrap();

    let r = move_screen(&f, "local/clock", "byonk-builtin", "clock").await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert!(r.text().contains("read-only"), "{}", r.text());
    assert!(f.root.join("local/clock/meta.yaml").is_file());

    let r = move_screen(&f, "byonk-builtin/default", "local", "default").await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert!(!f.root.join("local/default").exists());

    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), before);
}

#[tokio::test]
async fn a_move_onto_an_existing_screen_is_a_409_and_changes_nothing() {
    let f = fixture().await;
    screen(&f.app, "shared", "clock").await;
    let before = std::fs::read_to_string(&f.config).unwrap();

    let r = move_screen(&f, "local/clock", "local", "other").await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert!(r.text().contains("local/other"), "{}", r.text());

    let r = move_screen(&f, "local/clock", "shared", "clock").await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert!(r.text().contains("shared/clock"), "{}", r.text());

    assert!(f.root.join("local/clock/script.lua").is_file());
    assert!(f.root.join("local/other/script.lua").is_file());
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), before);
}

#[tokio::test]
async fn an_unknown_screen_is_a_404() {
    let f = fixture().await;
    let r = move_screen(&f, "local/nope", "shared", "nope").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text());
    assert!(!f.root.join("shared/nope").exists());
}

/// The delete half fails after the copy landed: the copy is removed again
/// and the source is untouched -- never both, never neither.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_delete_after_the_copy_rolls_the_copy_back() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture().await;
    let before = std::fs::read_to_string(&f.config).unwrap();
    let local = f.root.join("local");
    // The source repo's directory refuses to drop an entry.
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o555)).unwrap();
    let r = move_screen(&f, "local/clock", "shared", "clock").await;
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(r.status, StatusCode::INTERNAL_SERVER_ERROR, "{}", r.text());
    assert!(
        !f.root.join("shared/clock").exists(),
        "the copy was left behind"
    );
    for file in ["meta.yaml", "script.lua", "screen.svg"] {
        assert!(local.join("clock").join(file).is_file(), "{file} lost");
    }
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), before);
    let listed = refs(&f).await;
    assert!(listed.contains(&"local/clock".to_string()), "{listed:?}");
    assert!(!listed.contains(&"shared/clock".to_string()), "{listed:?}");
}

#[tokio::test]
async fn the_old_rename_route_is_gone() {
    let f = fixture().await;
    let r = f
        .app
        .post_json(
            "/dev/api/screen/rename",
            &[AUTH],
            r#"{"ref":"local/clock","name":"x"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text());
    assert!(f.root.join("local/clock").is_dir());
}

#[tokio::test]
async fn the_mcp_rename_rewrites_the_device_refs_too() {
    let f = fixture().await;
    let client = McpTestClient::new(&f.app, Some("secret"));
    client.initialize().await;
    let result = client
        .call_tool(
            "rename_screen",
            serde_json::json!({ "screen_ref": "local/clock", "new_path": "wall" }),
        )
        .await;
    assert_ne!(result["isError"], true, "{result}");
    let out = &result["structuredContent"];
    assert_eq!(out["screen_ref"], "local/wall", "{result}");
    assert_eq!(
        out["rewritten_devices"],
        serde_json::json!(["hall", "kitchen"]),
        "{result}"
    );
    let d = doc(&f.config);
    assert_eq!(d["devices"]["kitchen"]["screen"], "local/wall");
    assert_eq!(d["devices"]["hall"]["screen"], "local/wall");
    assert_eq!(d["devices"]["study"]["screen"], "local/other");
    assert!(f.root.join("local/wall/meta.yaml").is_file());
}

/// config.yaml cannot be rewritten after the files moved: the move is undone,
/// so the screen is back where every device still points.
#[cfg(unix)]
#[tokio::test]
async fn a_config_that_cannot_be_written_undoes_the_move() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture().await;
    let before = std::fs::read_to_string(&f.config).unwrap();
    // config.yaml's directory refuses the temp file `persist` writes; the
    // repos below it stay writable.
    std::fs::set_permissions(&f.root, std::fs::Permissions::from_mode(0o555)).unwrap();
    let same = move_screen(&f, "local/clock", "local", "wall").await;
    let across = move_screen(&f, "local/clock", "shared", "clock").await;
    std::fs::set_permissions(&f.root, std::fs::Permissions::from_mode(0o755)).unwrap();

    for r in [&same, &across] {
        assert_eq!(r.status, StatusCode::INTERNAL_SERVER_ERROR, "{}", r.text());
    }
    assert!(f.root.join("local/clock/meta.yaml").is_file());
    assert!(!f.root.join("local/wall").exists());
    assert!(!f.root.join("shared/clock").exists());
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), before);
    let listed = refs(&f).await;
    assert!(listed.contains(&"local/clock".to_string()), "{listed:?}");
}

/// Task U24: moving a nested screen out leaves no empty directories behind
/// in the source repo -- up to, but never including, the repo root.
#[tokio::test]
async fn a_move_removes_the_empty_parents_it_leaves_behind() {
    let f = fixture().await;
    screen(&f.app, "local", "nest/deep/clock").await;
    screen(&f.app, "local", "keep/sibling").await;
    screen(&f.app, "local", "keep/moving").await;

    let r = move_screen(&f, "local/nest/deep/clock", "shared", "clock").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(!f.root.join("local/nest").exists(), "empty parents remain");
    assert!(f.root.join("local/byonk-screens.yaml").is_file());

    // A parent that still holds another screen stays.
    let r = move_screen(&f, "local/keep/moving", "local", "moved").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(f.root.join("local/keep/sibling/meta.yaml").is_file());
    // (That the walk never removes the repo root, even an empty one, is
    // `prune_empty_parents_stops_at_the_root` in screen_store.rs.)
}
