mod common;

use axum::http::StatusCode;
use byonk::models::{ApiKey, AppConfig};
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
    // The embedded default ships with devices: {} (HA owns them).
    // Build a config that explicitly registers this MAC so the test
    // remains independent of what the embedded default contains.
    let registered_mac = "B4:A9:90:8C:6D:18";
    let yaml = format!(
        "admin:\n  token: secret\nregistration:\n  enabled: true\n\
         screens:\n  default:\n    script: default.lua\n    template: default.svg\n\
         devices:\n  \"{registered_mac}\":\n    screen: default\n"
    );
    let config: AppConfig = serde_yaml::from_str(&yaml).expect("parse test config");
    let app = TestApp::from_config(config);
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
    assert!(json["panels"].is_object());
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

/// Security: package tokens must NOT appear in GET /api/admin/config.
/// `PackageRef.token` is documented "Secret token; redacted in read APIs".
#[tokio::test]
async fn test_config_redacts_package_token() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "admin:\n  token: secret\npackages:\n  mypkg:\n    repo: https://github.com/example/screens\n    pin: v1.0.0\n    token: pkg-secret-abc\n";
    let (app, _) = TestApp::new_with_config_yaml(yaml, dir.path());
    let resp = app.get_with_headers("/api/admin/config", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();

    // The package entry should be present with repo and pin intact.
    let pkg = &json["packages"]["mypkg"];
    assert_eq!(
        pkg["repo"].as_str(),
        Some("https://github.com/example/screens"),
        "packages.mypkg.repo must be present"
    );
    assert_eq!(
        pkg["pin"].as_str(),
        Some("v1.0.0"),
        "packages.mypkg.pin must be present"
    );

    // Token must be absent — not just null, but the key must not be present.
    assert!(
        pkg.get("token").is_none() || pkg["token"].is_null(),
        "packages.mypkg.token must be redacted from GET /api/admin/config"
    );

    // Double-check: the raw response body must not contain the secret.
    let body = resp.text();
    assert!(
        !body.contains("pkg-secret-abc"),
        "raw response must not contain the package token value"
    );

    // admin.token must still be stripped too.
    assert!(
        json["admin"]["token"].is_null(),
        "admin.token must be stripped"
    );
}

/// Bug A regression test: an unregistered Ed25519 device that only hits /api/display
/// must appear in /api/admin/pending with a registration_code matching what is shown
/// on the device screen (derived from the Access-Token, i.e. the identity key).
#[tokio::test]
async fn test_unregistered_device_appears_in_pending_after_display() {
    let app = TestApp::new_admin("secret");

    // Use a MAC that is NOT in config.devices (registration enabled in embedded config).
    // Use a specific Access-Token so we can compute the expected registration code.
    // Since no Ed25519 headers are present, identity_key = Access-Token.
    let mac = "AA:BB:CC:DD:EE:FF";
    let api_key_str = "unregistered-test-identity-key-abc123";
    let headers = [
        ("ID", mac),
        ("Access-Token", api_key_str),
        ("Width", "800"),
        ("Height", "480"),
        ("FW-Version", "1.7.1"),
        ("Model", "og"),
        ("Host", "localhost:3000"),
    ];
    let _resp = app.get_with_headers("/api/display", &headers).await;
    // The device is shown a registration screen; we do not assert its response here.

    // Query /api/admin/pending
    let resp = app.get_with_headers("/api/admin/pending", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let arr: serde_json::Value = resp.json();
    let list = arr.as_array().unwrap();

    // Device must appear in pending after hitting /api/display
    assert!(
        list.iter().any(|d| d["mac"] == mac),
        "Unregistered device must appear in /api/admin/pending after /api/display; got: {list:?}"
    );

    // registration_code must match the code shown on the device screen
    // (derived from Access-Token = identity_key when no Ed25519 headers present)
    let expected_code = ApiKey::new(api_key_str).registration_code();
    let entry = list.iter().find(|d| d["mac"] == mac).unwrap();
    assert_eq!(
        entry["registration_code"].as_str().unwrap(),
        expected_code,
        "registration_code in pending must match the code shown on the device screen"
    );
}
