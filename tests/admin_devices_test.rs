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

/// A device may be configured under its registration code instead of its MAC.
/// It is still one device and must be listed once.
///
/// The seen-devices pass reports `key: mac`; the configured-devices pass then
/// has to recognise that the same device is already covered. Comparing config
/// keys against MACs alone misses it, and the extra row is not harmless: its
/// `key` is the registration code, and admin actions taken from it address a
/// different identifier than the one the device is known by.
#[tokio::test]
async fn a_device_configured_by_registration_code_is_listed_once() {
    use byonk::assets::AssetLoader;
    use byonk::models::{ApiKey, AppConfig, Device, DeviceId};
    use byonk::services::DeviceRegistry;

    let api_key = "listed-once-under-its-registration-code";
    let code = ApiKey::new(api_key).registration_code();
    let config_key = format!("{}-{}", &code[..5], &code[5..]);
    let mac = "AA:BB:CC:DD:EE:01";

    let loader = AssetLoader::new(None, None, None);
    let mut config = AppConfig::load_from_assets(&loader).expect("load embedded config");
    config.admin.token = Some("secret".to_string());
    let device_config = config
        .devices
        .get("DEFAULT")
        .expect("embedded config has a reserved DEFAULT device")
        .clone();
    config.devices.insert(config_key.clone(), device_config);

    let app = TestApp::from_config(config);

    let mut device = Device::new(DeviceId::new(mac), "og".to_string(), "1.7.1".to_string());
    device.api_key = ApiKey::new(api_key);
    app.registry.upsert(device).await.expect("upsert device");

    let listed: Vec<serde_json::Value> = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await
        .json();

    let rows: Vec<&serde_json::Value> = listed
        .iter()
        .filter(|d| d["key"] == mac || d["key"] == config_key.as_str())
        .collect();

    assert_eq!(rows.len(), 1, "one device, one row; got {rows:#?}");
    assert_eq!(
        rows[0]["key"], mac,
        "the MAC is the key the admin API reports for a device that has checked in"
    );
}
