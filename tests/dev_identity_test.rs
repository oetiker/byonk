//! Editing a panel's and a device's identity from the dev UI's Edit pane:
//! the panel Save carries name/match/size/colors and may rename the panel;
//! a device PATCH may rename the device; both can be deleted.

mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");
const BW: &str = "#000000,#FFFFFF";
const SIX: &str = "#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00";

const CONFIG: &str = "\
# byonk configuration — hand-written, with comments that must survive.
admin:
  token: secret

panels:
  # The reTerminal E1004. Its greens are hand-tuned; see docs.
  e1004:
    name: reTerminal E1004   # the six-colour one
    match: reterminal_e1004
    width: 1600
    height: 1200
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  # The original TRMNL.
  og:
    name: TRMNL OG
    colors: '#000000,#FFFFFF'
  spare:
    name: Spare
    colors: '#000000,#FFFFFF'

devices:
  # the kitchen display
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
  hall:
    screen: byonk-builtin/default
    panel: e1004
  study:
    screen: byonk-builtin/default
    panel: og
";

fn doc(path: &std::path::Path) -> serde_yaml::Value {
    serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn keys(path: &std::path::Path, section: &str) -> Vec<String> {
    doc(path)[section]
        .as_mapping()
        .unwrap()
        .keys()
        .map(|k| k.as_str().unwrap().to_string())
        .collect()
}

fn app() -> (tempfile::TempDir, TestApp, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_with_config_yaml(CONFIG, dir.path());
    (dir, app, path)
}

async fn overrides(app: &TestApp) -> serde_json::Value {
    app.get_with_headers("/dev/api/overrides", &[AUTH])
        .await
        .json()
}

fn live_keys(o: &serde_json::Value, list: &str) -> Vec<String> {
    o[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].as_str().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------- PanelInfo

#[tokio::test]
async fn panel_info_carries_match() {
    let (_d, app, _p) = app();
    let screens: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();
    let panels = screens["panels"].as_array().unwrap();
    let e1004 = panels.iter().find(|p| p["id"] == "e1004").unwrap();
    assert_eq!(e1004["match"], "reterminal_e1004", "{e1004}");
    let og = panels.iter().find(|p| p["id"] == "og").unwrap();
    assert!(og["match"].is_null(), "{og}");
}

// ------------------------------------------------------- panel Save identity

#[tokio::test]
async fn panel_save_writes_the_identity_fields_and_removes_absent_ones() {
    let (_d, app, path) = app();
    // og gains match/width/height and a new name.
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            &format!(
                r#"{{"name":"TRMNL OG v2","match":"trmnl_og","width":800,"height":480,"colors":"{BW}"}}"#
            ),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["id"], "og");
    let d = doc(&path);
    let og = &d["panels"]["og"];
    assert_eq!(og["name"], "TRMNL OG v2");
    assert_eq!(og["match"], "trmnl_og");
    assert_eq!(og["width"].as_u64(), Some(800));
    assert_eq!(og["height"].as_u64(), Some(480));
    assert_eq!(og["colors"], BW);

    // e1004 drops match/width/height by omitting them; its comments stay.
    let r = app
        .post_json(
            "/dev/api/panels/e1004/save",
            &[AUTH],
            &format!(r#"{{"name":"reTerminal E1004","colors":"{SIX}"}}"#),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let yaml = std::fs::read_to_string(&path).unwrap();
    let d = doc(&path);
    let e = &d["panels"]["e1004"];
    assert!(e.get("match").is_none(), "{yaml}");
    assert!(e.get("width").is_none(), "{yaml}");
    assert!(e.get("height").is_none(), "{yaml}");
    assert_eq!(e["name"], "reTerminal E1004");
    for c in [
        "# The reTerminal E1004. Its greens are hand-tuned; see docs.",
        "name: reTerminal E1004   # the six-colour one",
        "# The original TRMNL.",
        "# byonk configuration",
    ] {
        assert!(yaml.contains(c), "lost {c:?}:\n{yaml}");
    }
    assert_eq!(keys(&path, "panels"), vec!["e1004", "og", "spare"]);

    // The live config follows.
    let screens: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();
    let og = screens["panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "og")
        .unwrap()
        .clone();
    assert_eq!(og["match"], "trmnl_og");
    assert_eq!(og["width"], 800);
}

#[tokio::test]
async fn panel_save_validates_name_colors_and_size() {
    let (_d, app, path) = app();
    let before = std::fs::read_to_string(&path).unwrap();
    for body in [
        format!(r#"{{"name":"  ","colors":"{BW}"}}"#),
        r#"{"name":"OG","colors":""}"#.to_string(),
        r#"{"name":"OG","colors":"not-a-colour"}"#.to_string(),
        format!(r#"{{"name":"OG","colors":"{BW}","width":0}}"#),
        format!(r#"{{"name":"OG","colors":"{BW}","height":0}}"#),
        format!(r#"{{"name":"OG","colors":"{BW}","width":5000}}"#),
        format!(r#"{{"name":"OG","colors":"{BW}","height":4097}}"#),
        format!(r#"{{"id":"bad id","name":"OG","colors":"{BW}"}}"#),
    ] {
        let r = app
            .post_json("/dev/api/panels/og/save", &[AUTH], &body)
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}: {}", r.text());
    }
    // name and colors are required fields.
    let r = app
        .post_json("/dev/api/panels/og/save", &[AUTH], "{}")
        .await;
    assert!(r.status.is_client_error(), "{}", r.status);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

// ------------------------------------------------------------- panel rename

#[tokio::test]
async fn renaming_a_panel_renames_its_key_in_place_and_rewrites_device_refs() {
    let (_d, app, path) = app();
    let r = app
        .put_json(
            "/dev/api/overrides/panels/e1004",
            &[AUTH],
            r#"{"strength":0.5}"#,
        )
        .await;
    assert!(r.status.is_success());

    let r = app
        .post_json(
            "/dev/api/panels/e1004/save",
            &[AUTH],
            &format!(
                r#"{{"id":"e1004-v2","name":"reTerminal E1004","match":"reterminal_e1004","width":1600,"height":1200,"colors":"{SIX}"}}"#
            ),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["id"], "e1004-v2");

    let yaml = std::fs::read_to_string(&path).unwrap();
    // Same position, same comments, same lines -- only the key changed.
    assert_eq!(keys(&path, "panels"), vec!["e1004-v2", "og", "spare"]);
    assert!(
        yaml.contains(
            "  # The reTerminal E1004. Its greens are hand-tuned; see docs.\n  e1004-v2:\n    name: reTerminal E1004   # the six-colour one\n"
        ),
        "{yaml}"
    );
    let d = doc(&path);
    assert_eq!(d["devices"]["kitchen"]["panel"], "e1004-v2");
    assert_eq!(d["devices"]["hall"]["panel"], "e1004-v2");
    assert_eq!(d["devices"]["study"]["panel"], "og");
    assert!(yaml.contains("# the kitchen display"), "{yaml}");
    assert!(!yaml.contains("panel: e1004\n"), "{yaml}");

    let o = overrides(&app).await;
    assert!(live_keys(&o, "panels").is_empty(), "{o}");
}

#[tokio::test]
async fn renaming_a_panel_onto_an_existing_id_is_a_409_and_changes_nothing() {
    let (_d, app, path) = app();
    let before = std::fs::read_to_string(&path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panels/e1004/save",
            &[AUTH],
            &format!(r#"{{"id":"og","name":"X","colors":"{SIX}"}}"#),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn an_id_equal_to_the_path_panel_is_no_rename() {
    let (_d, app, path) = app();
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            &format!(r#"{{"id":"og","name":"TRMNL OG","colors":"{BW}"}}"#),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(keys(&path, "panels"), vec!["e1004", "og", "spare"]);
}

// ------------------------------------------------------------- panel delete

#[tokio::test]
async fn deleting_an_unknown_panel_is_a_404() {
    let (_d, app, _p) = app();
    let r = app.delete("/dev/api/panels/nosuch", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_panel_in_use_is_a_409_naming_its_devices() {
    let (_d, app, path) = app();
    let before = std::fs::read_to_string(&path).unwrap();
    let r = app.delete("/dev/api/panels/e1004", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let text = r.text();
    assert!(
        text.contains("hall") && text.contains("kitchen"),
        "the message must name the devices: {text}"
    );
    assert!(!text.contains("study"), "{text}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn deleting_an_unused_panel_removes_its_block_and_override() {
    let (_d, app, path) = app();
    app.put_json(
        "/dev/api/overrides/panels/spare",
        &[AUTH],
        r#"{"strength":0.5}"#,
    )
    .await;
    let r = app.delete("/dev/api/panels/spare", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(keys(&path, "panels"), vec!["e1004", "og"]);
    let yaml = std::fs::read_to_string(&path).unwrap();
    assert!(yaml.contains("# The original TRMNL."), "{yaml}");
    let o = overrides(&app).await;
    assert!(live_keys(&o, "panels").is_empty(), "{o}");
}

// ------------------------------------------------------------ device rename

#[tokio::test]
async fn renaming_a_device_in_a_patch_renames_in_place_and_applies_the_patch() {
    let (_d, app, path) = app();
    app.put_json(
        "/dev/api/overrides/devices/kitchen",
        &[AUTH],
        r#"{"strength":0.5}"#,
    )
    .await;
    let r = app
        .put_json("/dev/api/tuning-session/kitchen", &[AUTH], "")
        .await;
    assert!(r.status.is_success(), "{}", r.status);

    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"key":"pantry","name":"Pantry"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["key"], "pantry");

    let yaml = std::fs::read_to_string(&path).unwrap();
    assert_eq!(keys(&path, "devices"), vec!["pantry", "hall", "study"]);
    assert!(
        yaml.contains("  # the kitchen display\n  pantry:\n    screen: byonk-builtin/default\n"),
        "{yaml}"
    );
    let d = doc(&path);
    assert_eq!(d["devices"]["pantry"]["name"], "Pantry");
    assert_eq!(d["devices"]["pantry"]["panel"], "e1004");

    let o = overrides(&app).await;
    assert!(live_keys(&o, "devices").is_empty(), "{o}");
    assert!(live_keys(&o, "tuning_sessions").is_empty(), "{o}");
}

#[tokio::test]
async fn renaming_a_device_onto_an_existing_key_is_a_409_and_changes_nothing() {
    let (_d, app, path) = app();
    let before = std::fs::read_to_string(&path).unwrap();
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"key":"hall","name":"Pantry"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn a_rename_with_a_failing_patch_writes_neither() {
    let (_d, app, path) = app();
    let before = std::fs::read_to_string(&path).unwrap();
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"key":"pantry","screen":"no/such-screen"}"#,
        )
        .await;
    assert!(r.status.is_client_error(), "{} {}", r.status, r.text());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn a_device_rename_validates_the_new_key() {
    let (_d, app, path) = app();
    let before = std::fs::read_to_string(&path).unwrap();
    for key in ["", "  ", "pan try"] {
        let r = app
            .patch_json(
                "/dev/api/devices/kitchen",
                &[AUTH],
                &format!(r#"{{"key":"{key}"}}"#),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{key:?}: {}", r.text());
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn the_admin_patch_ignores_a_key_in_its_body() {
    let (_d, app, path) = app();
    let r = app
        .patch_json(
            "/api/admin/devices/kitchen",
            &[AUTH],
            r#"{"key":"pantry","name":"Kitchen"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(keys(&path, "devices"), vec!["kitchen", "hall", "study"]);
    assert_eq!(doc(&path)["devices"]["kitchen"]["name"], "Kitchen");
}

// ------------------------------------------------------------ device delete

#[tokio::test]
async fn deleting_a_device_removes_it_its_override_and_its_session() {
    let (_d, app, path) = app();
    app.put_json(
        "/dev/api/overrides/devices/kitchen",
        &[AUTH],
        r#"{"strength":0.5}"#,
    )
    .await;
    app.put_json("/dev/api/tuning-session/kitchen", &[AUTH], "")
        .await;

    let r = app.delete("/dev/api/devices/kitchen", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(keys(&path, "devices"), vec!["hall", "study"]);
    let o = overrides(&app).await;
    assert!(live_keys(&o, "devices").is_empty(), "{o}");
    assert!(live_keys(&o, "tuning_sessions").is_empty(), "{o}");

    let r = app.delete("/dev/api/devices/kitchen", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_default_device_cannot_be_deleted_from_the_dev_ui() {
    let (_d, app, _p) = app();
    let r = app.delete("/dev/api/devices/DEFAULT", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
}
