//! Tests for GET /api/admin/devices and admin auth.

mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn test_admin_disabled_returns_404() {
    // Default TestApp has no admin token configured.
    let app = TestApp::new();
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer x")])
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_wrong_token_returns_401() {
    let app = TestApp::new_admin("secret");
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer nope")])
        .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_missing_token_returns_401() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/devices").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_devices_lists_seen_device() {
    let app = TestApp::new_admin("secret");
    // Make a device appear in the registry via the normal setup flow.
    app.register_device("AA:BB:CC:DD:EE:FF").await;

    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let json: serde_json::Value = resp.json();
    let arr = json.as_array().expect("array");
    assert!(arr.iter().any(|d| d["mac"] == "AA:BB:CC:DD:EE:FF"));
}

#[tokio::test]
async fn test_list_devices_flags_reserved_default() {
    let app = TestApp::new_admin("secret");
    // Also register a real (non-reserved) device so we can assert the
    // contrast: a physical device must report reserved:false.
    app.register_device("AA:BB:CC:DD:EE:FF").await;

    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let json: serde_json::Value = resp.json();
    let devices = json.as_array().expect("array");

    let default = devices
        .iter()
        .find(|d| d["key"] == "DEFAULT")
        .expect("DEFAULT device present");
    assert!(default.get("reserved").is_some());
    assert_eq!(default["reserved"], true);

    let seen = devices
        .iter()
        .find(|d| d["mac"] == "AA:BB:CC:DD:EE:FF")
        .expect("seen device present");
    assert_eq!(seen["reserved"], false);
}

#[tokio::test]
async fn test_custom_model_header_is_stored_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    // A reTerminal reports its own model string (not "og"/"x").
    let mac = "9C:13:9E:AB:99:D4";
    let resp = app
        .get_with_headers(
            "/api/setup",
            &[
                ("ID", mac),
                ("FW-Version", "1.0.0"),
                ("Model", "reterminal_e1002"),
            ],
        )
        .await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);

    let listed = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == mac)
        .expect("device row present");
    assert_eq!(row["model"], "reterminal_e1002");
}
