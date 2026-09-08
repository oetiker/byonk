//! Integration: HA add-on options.json feeds the admin token into the running server.

mod common;

use axum::http::StatusCode;
use byonk::addon_options::{apply_to_config, read_options};
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use common::TestApp;
use std::sync::Arc;

/// `BYONK_DEV_USERNAME`/`BYONK_DEV_PASSWORD` are process-wide env vars.
/// `dev_env_wins_over_config` sets and clears them directly; every other test
/// in this file builds a `TestApp` via `app_with_options`/`TestApp::from_config`,
/// which resolves `DevAuth` (and so reads those same vars) as part of building
/// `AppState` (`create_app_state_with_overrides` in `src/server.rs`). Cargo runs
/// tests in this binary on parallel threads, so without a shared lock the env
/// mutation could still be in effect (or mid-mutation) while another test in
/// this file builds its `AppState`. Every test below takes this guard for its
/// whole body — including the four that never touch the vars themselves, since
/// they are the other side of the race — so it must be an async-aware mutex
/// held across `.await` points.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let _guard = ENV_LOCK.lock().await;
    let app = app_with_options(r#"{"admin_token":"secret","log_level":"info"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
}

#[tokio::test]
async fn options_token_rejects_wrong_bearer() {
    let _guard = ENV_LOCK.lock().await;
    let app = app_with_options(r#"{"admin_token":"secret"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer nope")])
        .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn blank_options_token_keeps_admin_dormant() {
    let _guard = ENV_LOCK.lock().await;
    let app = app_with_options(r#"{"admin_token":"","log_level":"info"}"#);
    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn blank_option_clears_preexisting_config_token() {
    let _guard = ENV_LOCK.lock().await;
    // A token sitting in config (e.g. a user-edited /config/config.yaml) must NOT
    // keep the admin API alive when the add-on option is blank — the add-on option
    // is the single source of truth, so a blank option disables the API.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("options.json");
    std::fs::write(&path, r#"{"admin_token":""}"#).expect("write options");
    let result = read_options(&path);

    let loader = Arc::new(AssetLoader::new(None, None, None));
    let mut config = AppConfig::load_from_assets(&loader).expect("load config");
    config.admin.token = Some("stale-config-token".to_string());
    apply_to_config(&result, &mut config);

    let app = TestApp::from_config(config);
    let resp = app
        .get_with_headers(
            "/api/admin/devices",
            &[("Authorization", "Bearer stale-config-token")],
        )
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dev_env_wins_over_config() {
    let _guard = ENV_LOCK.lock().await;
    std::env::set_var("BYONK_DEV_USERNAME", "envu");
    std::env::set_var("BYONK_DEV_PASSWORD", "envp");
    let auth = byonk::api::dev_auth::DevAuth::resolve(Some("cfgu"), Some("cfgp"));
    std::env::remove_var("BYONK_DEV_USERNAME");
    std::env::remove_var("BYONK_DEV_PASSWORD");
    let c = auth.credentials.unwrap();
    assert_eq!((c.username.as_str(), c.password.as_str()), ("envu", "envp"));
}
