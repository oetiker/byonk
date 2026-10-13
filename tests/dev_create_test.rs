//! Creating a panel and a device from the dev UI's Library, and the list of
//! seen-but-unconfigured devices it offers when adding one.

mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn commented_config() -> &'static str {
    "\
# byonk configuration — hand-written, with comments that must survive.
admin:
  token: secret

panels:
  # The reTerminal E1004. Its greens are hand-tuned; see docs.
  e1004:
    name: reTerminal E1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'

devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
"
}

fn doc(path: &std::path::Path) -> serde_yaml::Value {
    serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn creating_a_panel_adds_its_block_and_keeps_comments() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"bw-test","name":"BW test","colors":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["id"], "bw-test");

    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        yaml.contains("hand-tuned; see docs"),
        "a comment was lost:\n{yaml}"
    );
    let d = doc(&config_path);
    assert_eq!(d["panels"]["bw-test"]["name"], "BW test");
    assert_eq!(d["panels"]["bw-test"]["colors"], "#000000,#FFFFFF");
    assert_eq!(d["panels"]["e1004"]["name"], "reTerminal E1004");

    // The live config carries it: the Library lists it right away.
    let screens: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();
    assert!(
        screens["panels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "bw-test" && p["name"] == "BW test"),
        "{screens}"
    );
}

#[tokio::test]
async fn creating_a_panel_works_without_a_panels_key() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) =
        TestApp::new_with_config_yaml("# top comment\nadmin:\n  token: secret\n", dir.path());
    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"p1","name":"P one","colors":"#000000,#FFFFFF,#FF0000"}"##,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let d = doc(&config_path);
    assert_eq!(d["panels"]["p1"]["name"], "P one");
    assert_eq!(d["panels"]["p1"]["colors"], "#000000,#FFFFFF,#FF0000");
    assert!(std::fs::read_to_string(&config_path)
        .unwrap()
        .contains("# top comment"));
}

#[tokio::test]
async fn creating_a_panel_works_with_an_empty_panels_key() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) =
        TestApp::new_with_config_yaml("admin:\n  token: secret\npanels:\n", dir.path());
    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"p1","name":"P one","colors":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(doc(&config_path)["panels"]["p1"]["name"], "P one");
}

#[tokio::test]
async fn creating_an_existing_panel_is_a_409_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let before = std::fs::read_to_string(&config_path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"e1004","name":"Other","colors":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}

#[tokio::test]
async fn creating_a_panel_validates_id_name_and_colors() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let before = std::fs::read_to_string(&config_path).unwrap();
    for (why, body) in [
        (
            "empty id",
            r##"{"id":"","name":"N","colors":"#000000,#FFFFFF"}"##,
        ),
        (
            "id with a space",
            r##"{"id":"a b","name":"N","colors":"#000000,#FFFFFF"}"##,
        ),
        (
            "id with a dot",
            r##"{"id":"a.b","name":"N","colors":"#000000,#FFFFFF"}"##,
        ),
        (
            "empty name",
            r##"{"id":"x","name":"  ","colors":"#000000,#FFFFFF"}"##,
        ),
        ("one color", r##"{"id":"x","name":"N","colors":"#000000"}"##),
        (
            "bad hex",
            r##"{"id":"x","name":"N","colors":"#000000,#GGGGGG"}"##,
        ),
        ("empty colors", r##"{"id":"x","name":"N","colors":""}"##),
    ] {
        let r = app.post_json("/dev/api/panels", &[AUTH], body).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{why}: {}", r.text());
    }
    // deny_unknown_fields: an unknown key is refused, not ignored.
    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"x","name":"N","colors":"#000000,#FFFFFF","match":"x"}"##,
        )
        .await;
    assert!(r.status.is_client_error(), "{}", r.status);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}

#[tokio::test]
async fn creating_a_panel_can_carry_its_size() {
    // The Library's "+" copies the current panel's size into the
    // placeholder it creates, in the one write.
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"new_panel","name":"New panel","colors":"#000000,#FFFFFF","width":1600,"height":1200}"##,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let d = doc(&config_path);
    assert_eq!(d["panels"]["new_panel"]["width"], 1600);
    assert_eq!(d["panels"]["new_panel"]["height"], 1200);

    let before = std::fs::read_to_string(&config_path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panels",
            &[AUTH],
            r##"{"id":"zero","name":"Z","colors":"#000000,#FFFFFF","width":0}"##,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);

    // Above the cap: a clear 400 naming the limit, and nothing written.
    for size in [r#""width":5000"#, r#""height":4097"#] {
        let body = format!(r##"{{"id":"huge","name":"H","colors":"#000000,#FFFFFF",{size}}}"##);
        let r = app.post_json("/dev/api/panels", &[AUTH], &body).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{size}: {}", r.text());
        assert!(r.text().contains("4096"), "{}", r.text());
    }
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}

#[tokio::test]
async fn creating_a_panel_needs_the_dev_session() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/panels",
            &[],
            r##"{"id":"x","name":"N","colors":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert!(
        r.status == StatusCode::UNAUTHORIZED || r.status.is_redirection(),
        "{}",
        r.status
    );
}

#[tokio::test]
async fn creating_a_device_writes_its_entry() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/devices",
            &[AUTH],
            r#"{"key":"AA:BB:CC:DD:EE:FF","screen":"byonk-builtin/default","name":"Hall"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["key"], "AA:BB:CC:DD:EE:FF");
    let d = doc(&config_path);
    assert_eq!(
        d["devices"]["AA:BB:CC:DD:EE:FF"]["screen"],
        "byonk-builtin/default"
    );
    assert_eq!(d["devices"]["AA:BB:CC:DD:EE:FF"]["name"], "Hall");
    assert!(std::fs::read_to_string(&config_path)
        .unwrap()
        .contains("hand-tuned; see docs"));
}

#[tokio::test]
async fn creating_an_existing_device_is_a_409() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/devices",
            &[AUTH],
            r#"{"key":"kitchen","screen":"byonk-builtin/default"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
}

#[tokio::test]
async fn creating_a_device_needs_key_and_screen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/devices",
            &[AUTH],
            r#"{"screen":"byonk-builtin/default"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    let r = app
        .post_json("/dev/api/devices", &[AUTH], r#"{"key":"hall"}"#)
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
}

#[tokio::test]
async fn pending_devices_matches_the_admin_list() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    app.register_device("11:22:33:44:55:66").await;

    let dev: serde_json::Value = app
        .get_with_headers("/dev/api/pending-devices", &[AUTH])
        .await
        .json();
    let admin: serde_json::Value = app
        .get_with_headers("/api/admin/pending", &[AUTH])
        .await
        .json();
    assert_eq!(dev, admin);
    let list = dev.as_array().unwrap();
    assert!(
        list.iter()
            .any(|d| d["mac"] == "11:22:33:44:55:66" && d["model"].is_string()),
        "{dev}"
    );

    // Once configured, it is no longer pending.
    let r = app
        .post_json(
            "/dev/api/devices",
            &[AUTH],
            r#"{"key":"11:22:33:44:55:66","screen":"byonk-builtin/default"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let dev: serde_json::Value = app
        .get_with_headers("/dev/api/pending-devices", &[AUTH])
        .await
        .json();
    assert!(
        !dev.as_array()
            .unwrap()
            .iter()
            .any(|d| d["mac"] == "11:22:33:44:55:66"),
        "{dev}"
    );
}
