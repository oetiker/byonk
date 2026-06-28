mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_pending_lists_unregistered_seen_device() {
    let app = TestApp::new_admin("secret");
    // A freshly-set-up device with no config mapping is "pending".
    app.register_device("11:22:33:44:55:66").await;

    let resp = app.get_with_headers("/api/admin/pending", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let arr: serde_json::Value = resp.json();
    let list = arr.as_array().unwrap();
    assert!(list.iter().any(|d| d["mac"] == "11:22:33:44:55:66"));
    assert!(list[0]["registration_code"].as_str().unwrap().len() == 10);
}

#[tokio::test]
async fn test_pending_requires_auth() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/pending").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_pending_excludes_registered_device() {
    let app = TestApp::new_admin("secret");
    // "B4:A9:90:8C:6D:18" is present in the embedded config's devices section,
    // so it counts as registered. Once we trigger the setup flow for that MAC,
    // it must NOT appear in /api/admin/pending.
    let registered_mac = "B4:A9:90:8C:6D:18";
    app.register_device(registered_mac).await;

    let resp = app.get_with_headers("/api/admin/pending", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let arr: serde_json::Value = resp.json();
    let list = arr.as_array().unwrap();
    assert!(
        !list.iter().any(|d| d["mac"] == registered_mac),
        "registered device should not appear in /api/admin/pending"
    );
}

#[tokio::test]
async fn test_config_returns_json_without_token() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app.get_with_headers("/api/admin/config", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();
    assert!(json["screens"].is_object());
    // admin section present but token stripped
    assert!(json["admin"].is_object(), "admin section should be present");
    assert!(
        json["admin"]["token"].is_null(),
        "admin.token must be stripped"
    );
}

#[tokio::test]
async fn test_config_requires_auth() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/config").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}
