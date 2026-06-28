mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_write_on_embedded_config_returns_409() {
    let app = TestApp::new_admin("secret"); // embedded-only
    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"hello"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_add_device_persists_and_hot_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"hello"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::OK);

    // File updated + comment preserved.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("CC:DD:EE:FF:00:11"));

    // Hot-reload: GET /devices shows it without restart.
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    assert!(json
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["mac"] == "CC:DD:EE:FF:00:11"));
}

#[tokio::test]
async fn test_add_unknown_screen_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"does-not-exist"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
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
    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"default_screen":"does-not-exist"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_patch_then_delete_device() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        r#"{"key":"CC:DD","screen":"hello"}"#,
    )
    .await;

    let patch = app
        .patch_json(
            "/api/admin/devices/CC:DD",
            &[AUTH],
            r#"{"screen":"graytest"}"#,
        )
        .await;
    assert_eq!(patch.status, StatusCode::OK);

    let del = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::OK);
    let del_again = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del_again.status, StatusCode::NOT_FOUND);
}
