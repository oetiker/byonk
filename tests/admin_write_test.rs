mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

// Device `screen` values are always qualified `handle/path` package refs.
const HELLO: &str = "byonk-builtin/example/hello";
const GREY: &str = "byonk-builtin/calibration/grey";
const GPHOTO: &str = "byonk-builtin/useful/gphoto";
const COLOR: &str = "byonk-builtin/calibration/color";
const TRANSIT: &str = "byonk-builtin/useful/swiss-departure-board";
const UNKNOWN: &str = "byonk-builtin/does-not-exist";

#[tokio::test]
async fn test_write_on_embedded_config_returns_409() {
    let app = TestApp::new_admin("secret"); // embedded-only
    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{HELLO}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_add_device_persists_and_hot_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{HELLO}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK);

    // File updated + comment preserved.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("CC:DD:EE:FF:00:11"));
    assert!(
        on_disk.contains(HELLO),
        "qualified screen ref written to disk"
    );

    // Hot-reload: GET /devices shows it without restart.
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == "CC:DD:EE:FF:00:11")
        .expect("device present after hot reload");
    assert_eq!(
        row["screen"], HELLO,
        "screen reads back as the qualified ref"
    );
}

#[tokio::test]
async fn test_add_unknown_screen_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{UNKNOWN}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_bare_name_screen_is_rejected() {
    // A bare (unqualified) name is no longer resolvable — it must be a `handle/path` ref.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"hello"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_builtin_ref_screen_is_accepted() {
    // A qualified builtin ref that ships in the embedded package must be assignable.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{GPHOTO}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK);
}

#[tokio::test]
async fn test_builtin_ref_as_default_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"default_screen":"{GPHOTO}"}}"#);
    let resp = app.patch_json("/api/admin/settings", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK);
}

#[tokio::test]
async fn test_patch_settings_toggles_registration() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"registration_enabled":false}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("enabled: false"));
}

#[tokio::test]
async fn test_patch_settings_bogus_auth_mode_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app
        .patch_json("/api/admin/settings", &[AUTH], r#"{"auth_mode":"bogus"}"#)
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_patch_settings_unknown_default_screen_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"default_screen":"{UNKNOWN}"}}"#);
    let resp = app.patch_json("/api/admin/settings", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_patch_then_delete_device() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"CC:DD","screen":"{HELLO}"}}"#),
    )
    .await;

    let patch = app
        .patch_json(
            "/api/admin/devices/CC:DD",
            &[AUTH],
            &format!(r#"{{"screen":"{GREY}"}}"#),
        )
        .await;
    assert_eq!(patch.status, StatusCode::OK);

    let del = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::OK);
    let del_again = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del_again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_patch_panel_and_dither_read_back() {
    // Repro for the device-page bug: setting Panel/Dither via PATCH writes to
    // disk but GET /api/admin/devices reported them as null. The device is
    // POSTed into the shipped empty `devices: {}` map (the live scenario).
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .post_json(
            "/api/admin/devices",
            &[AUTH],
            &format!(r#"{{"key":"9C:13:9E:AB:99:D4","screen":"{COLOR}"}}"#),
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let resp = app
        .patch_json(
            "/api/admin/devices/9C:13:9E:AB:99:D4",
            &[AUTH],
            r#"{"panel":"trmnl_x"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let resp = app
        .patch_json(
            "/api/admin/devices/9C:13:9E:AB:99:D4",
            &[AUTH],
            r#"{"dither":"atkinson"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    // Disk has both keys (this part already worked in the field).
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("panel: trmnl_x"),
        "expected 'panel: trmnl_x' on disk:\n{on_disk}"
    );
    assert!(
        on_disk.contains("dither: atkinson"),
        "expected 'dither: atkinson' on disk:\n{on_disk}"
    );

    // The bug: GET /api/admin/devices must read them back, not null.
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "9C:13:9E:AB:99:D4")
        .expect("device row present");
    assert_eq!(row["screen"], COLOR, "screen should read back");
    assert_eq!(row["panel"], "trmnl_x", "panel should read back");
    assert_eq!(row["dither"], "atkinson", "dither should read back");
}

#[tokio::test]
async fn test_patch_panel_and_dither_read_back_for_seen_device() {
    // Same as above, but the device has been SEEN by the registry first (it
    // polled /api/setup). This exercises the first loop of list_devices, which
    // is the path the live HA-onboarded device takes.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    let mac = "9C:13:9E:AB:99:D4";
    app.register_device(mac).await;

    let resp = app
        .post_json(
            "/api/admin/devices",
            &[AUTH],
            &format!(r#"{{"key":"9C:13:9E:AB:99:D4","screen":"{COLOR}"}}"#),
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    app.patch_json(
        "/api/admin/devices/9C:13:9E:AB:99:D4",
        &[AUTH],
        r#"{"panel":"trmnl_x"}"#,
    )
    .await;
    app.patch_json(
        "/api/admin/devices/9C:13:9E:AB:99:D4",
        &[AUTH],
        r#"{"dither":"atkinson"}"#,
    )
    .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == mac)
        .expect("device row present");
    assert_eq!(row["screen"], COLOR, "screen should read back");
    assert_eq!(row["panel"], "trmnl_x", "panel should read back");
    assert_eq!(row["dither"], "atkinson", "dither should read back");
}

#[tokio::test]
async fn test_patch_params_submap_reads_back_with_populated_devices() {
    // Repro for the nested-params write bug: with a populated devices map, the
    // config writer mis-indented the params sub-map so it parsed back empty.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    // Two devices -> populated devices map (exercises the non-empty write path).
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{TRANSIT}"}}"#),
    )
    .await;
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"CC:DD","screen":"{HELLO}"}}"#),
    )
    .await;

    let resp = app
        .patch_json(
            "/api/admin/devices/AA:BB",
            &[AUTH],
            r#"{"params":{"station":"Olten"}}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .expect("device row present");
    assert_eq!(row["params"]["station"], "Olten", "params should read back");
}

#[tokio::test]
async fn test_patch_params_merge_preserves_other_keys() {
    // Editing one param (no screen change) must merge, not replace: a second
    // single-key PATCH must not drop the first key.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{TRANSIT}"}}"#),
    )
    .await;

    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        r#"{"params":{"station":"Olten"}}"#,
    )
    .await;
    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        r#"{"params":{"limit":5}}"#,
    )
    .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(
        row["params"]["station"], "Olten",
        "first key preserved after 2nd patch"
    );
    assert_eq!(row["params"]["limit"], 5, "second key present");
}

#[tokio::test]
async fn test_patch_with_screen_change_replaces_params() {
    // A screen change replaces params wholesale (new screen's defaults).
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{HELLO}"}}"#),
    )
    .await;
    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        r#"{"params":{"old":"x"}}"#,
    )
    .await;

    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        &format!(r#"{{"screen":"{GREY}","params":{{"new":"y"}}}}"#),
    )
    .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(
        row["params"].get("old"),
        None,
        "old key dropped on screen change"
    );
    assert_eq!(row["params"]["new"], "y", "new key present");
}

#[tokio::test]
async fn test_patch_settings_registration_screen_persists() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            &format!(r#"{{"registration_screen":"{TRANSIT}"}}"#),
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains(&format!("screen: {TRANSIT}")),
        "expected 'screen: {TRANSIT}' in:\n{on_disk}"
    );
}

#[tokio::test]
async fn test_patch_settings_registration_screen_empty_is_builtin_sentinel() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"registration_screen":""}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("screen: ''") || on_disk.contains("screen: \"\""),
        "expected empty screen sentinel in:\n{on_disk}"
    );
}

#[tokio::test]
async fn test_patch_settings_unknown_registration_screen_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            &format!(r#"{{"registration_screen":"{UNKNOWN}"}}"#),
        )
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_patch_name_reads_back_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{HELLO}"}}"#),
    )
    .await;
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"name":"Kitchen"}"#)
        .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["name"], "Kitchen");

    // Empty string clears it.
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"name":""}"#)
        .await;
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["name"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_patch_refresh_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{HELLO}"}}"#),
    )
    .await;
    let resp = app
        .patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"refresh":600}"#)
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["refresh"], 600);

    // 0 clears the override.
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"refresh":0}"#)
        .await;
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["refresh"], serde_json::Value::Null);
}
