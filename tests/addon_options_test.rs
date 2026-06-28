//! Integration: HA add-on options.json feeds the admin token into the running server.

mod common;

use axum::http::StatusCode;
use byonk::addon_options::{apply_to_config, read_options};
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use common::TestApp;
use std::sync::Arc;

/// Build a TestApp as `run_server` would: read an options.json, apply it to the
/// freshly loaded config, then build the app.
fn app_with_options(json: &str) -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("options.json");
    std::fs::write(&path, json).expect("write options");
    let result = read_options(&path);

    let loader = Arc::new(AssetLoader::new(None, None, None));
    let mut config = AppConfig::load_from_assets(&loader).expect("load config");
    apply_to_config(&result, &mut config);
    TestApp::from_config(config)
}

#[tokio::test]
async fn options_token_activates_admin_api() {
    let app = app_with_options(r#"{"admin_token":"secret","log_level":"info"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
}

#[tokio::test]
async fn options_token_rejects_wrong_bearer() {
    let app = app_with_options(r#"{"admin_token":"secret"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer nope")])
        .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn blank_options_token_keeps_admin_dormant() {
    let app = app_with_options(r#"{"admin_token":"","log_level":"info"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}
