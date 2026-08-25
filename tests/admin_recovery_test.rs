//! Tests for the panel-recovery admin endpoints.
//!
//! A device can be addressed by two names: the MAC that `/api/admin/devices`
//! reports once it has checked in, and the `config.yaml` key it is listed
//! under, which may be a registration code. Both reach the same device, so a
//! run started under one name has to be visible and cancellable under the other.

mod common;

use common::TestApp;

const MAC: &str = "AA:BB:CC:DD:EE:02";
const API_KEY: &str = "recovery-endpoints-resolve-either-name";
const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// An app whose config lists `MAC`'s device under its registration code, with
/// the device already in the registry. Returns the app and that config key.
async fn app_with_code_configured_device() -> (TestApp, String) {
    use byonk::assets::AssetLoader;
    use byonk::models::{ApiKey, AppConfig, Device, DeviceId};
    use byonk::services::DeviceRegistry;

    let code = ApiKey::new(API_KEY).registration_code();
    let config_key = format!("{}-{}", &code[..5], &code[5..]);

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

    let mut device = Device::new(DeviceId::new(MAC), "og".to_string(), "1.7.1".to_string());
    device.api_key = ApiKey::new(API_KEY);
    app.registry.upsert(device).await.expect("upsert device");

    (app, config_key)
}

#[tokio::test]
async fn a_run_started_under_the_config_key_is_reported_under_the_mac() {
    let (app, config_key) = app_with_code_configured_device().await;

    app.post_json(
        &format!("/api/admin/devices/{config_key}/recover"),
        &[AUTH],
        r#"{"wipes": 3}"#,
    )
    .await;

    let status: serde_json::Value = app
        .get_with_headers(&format!("/api/admin/devices/{MAC}/recover"), &[AUTH])
        .await
        .json();

    assert_eq!(
        status["active"], true,
        "the MAC is what /api/admin/devices reports; a run must be visible under it"
    );
    assert_eq!(status["total"], 3);
}

#[tokio::test]
async fn a_run_started_under_the_config_key_is_cancelled_by_the_mac() {
    let (app, config_key) = app_with_code_configured_device().await;

    app.post_json(
        &format!("/api/admin/devices/{config_key}/recover"),
        &[AUTH],
        r#"{"wipes": 3}"#,
    )
    .await;

    app.delete(&format!("/api/admin/devices/{MAC}/recover"), &[AUTH])
        .await;

    let status: serde_json::Value = app
        .get_with_headers(&format!("/api/admin/devices/{config_key}/recover"), &[AUTH])
        .await
        .json();

    assert_eq!(
        status["active"], false,
        "cancelling under either name must end the run, not silently do nothing"
    );
}

/// Before a device has ever checked in there is no MAC to resolve to, so the
/// session is filed under the code the operator typed. Asking about it with the
/// other written form of the same code must still find it.
#[tokio::test]
async fn a_run_for_an_unseen_device_is_found_under_either_spelling_of_its_code() {
    use byonk::assets::AssetLoader;
    use byonk::models::{ApiKey, AppConfig};

    let code = ApiKey::new(API_KEY).registration_code();
    let hyphenated = format!("{}-{}", &code[..5], &code[5..]);

    let loader = AssetLoader::new(None, None, None);
    let mut config = AppConfig::load_from_assets(&loader).expect("load embedded config");
    config.admin.token = Some("secret".to_string());
    let device_config = config
        .devices
        .get("DEFAULT")
        .expect("embedded config has a reserved DEFAULT device")
        .clone();
    config.devices.insert(hyphenated.clone(), device_config);

    // Deliberately no registry entry: the device has never checked in.
    let app = TestApp::from_config(config);

    app.post_json(
        &format!("/api/admin/devices/{code}/recover"),
        &[AUTH],
        r#"{"wipes": 3}"#,
    )
    .await;

    let status: serde_json::Value = app
        .get_with_headers(&format!("/api/admin/devices/{hyphenated}/recover"), &[AUTH])
        .await
        .json();

    assert_eq!(
        status["active"], true,
        "hyphenation must not hide a run that is in progress"
    );
}
