//! An unknown key in a device's settings is an error (owner ruling E4):
//! byonk will not start on it, and a write cannot slip past it.

mod common;

use axum::http::StatusCode;
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use common::TestApp;
use std::sync::Arc;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");
const VALID: &str = "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
  office:
    screen: byonk-builtin/default
";

fn with_typo() -> String {
    VALID.replacen(
        "byonk-builtin/default\n",
        "byonk-builtin/default\n    dithr: atkinson\n",
        1,
    )
}

#[test]
fn byonk_refuses_to_start_and_names_the_device_and_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, with_typo()).unwrap();
    let loader = Arc::new(AssetLoader::new(None, None, Some(path)));
    let err = AppConfig::load_from_assets(&loader)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("Invalid config.yaml: devices.kitchen: unknown field `dithr`"),
        "{err}"
    );
    let err = byonk::server::load_config_with_addon_overlay(&loader)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("devices.kitchen") && err.contains("dithr"),
        "{err}"
    );
}

#[tokio::test]
async fn a_write_is_refused_with_400_while_config_yaml_holds_an_unknown_device_key() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(VALID, dir.path());
    // Hand-edited while byonk runs: nothing reloads config.yaml on its own.
    std::fs::write(&config_path, with_typo()).unwrap();

    let r = app
        .patch_json("/api/admin/devices/office", &[AUTH], r#"{"name":"Office"}"#)
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(
        r.text().contains("devices.kitchen") && r.text().contains("dithr"),
        "{}",
        r.text()
    );
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        with_typo(),
        "the file is left exactly as the owner wrote it"
    );
    let live = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    assert!(
        !live.text().contains("Office"),
        "the live config did not take the write: {}",
        live.text()
    );
}

#[tokio::test]
async fn another_reload_failure_keeps_its_500() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(VALID, dir.path());
    std::fs::write(&config_path, "devices: [not, a, map").unwrap();
    let r = app
        .patch_json("/api/admin/devices/office", &[AUTH], r#"{"name":"Office"}"#)
        .await;
    assert_eq!(r.status, StatusCode::INTERNAL_SERVER_ERROR, "{}", r.text());
}

#[tokio::test]
async fn a_json_body_with_an_unknown_gamut_key_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_with_config_yaml(VALID, dir.path());
    let ok = app
        .put_json(
            "/dev/api/overrides/devices/kitchen",
            &[AUTH],
            r#"{"gamut":{"knee":0.5}}"#,
        )
        .await;
    assert!(ok.status.is_success(), "{} {}", ok.status, ok.text());
    let r = app
        .put_json(
            "/dev/api/overrides/devices/kitchen",
            &[AUTH],
            r#"{"gamut":{"kne":0.5}}"#,
        )
        .await;
    assert!(r.status.is_client_error(), "{} {}", r.status, r.text());
    assert!(r.text().contains("kne"), "{}", r.text());
}
