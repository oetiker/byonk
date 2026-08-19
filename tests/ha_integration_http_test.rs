//! Tests for the two Supervisor calls the app makes after installing the
//! integration. A tiny local axum server stands in for Supervisor and records
//! what arrived.

use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::post, Json, Router};
use byonk::ha_integration::{announce_discovery, notify_restart, supervisor_url};
use serde_json::Value;

type Captured = Arc<Mutex<Vec<(String, Option<String>, Value)>>>;

/// `BYONK_SUPERVISOR_URL` is a process-wide env var, but every test here needs
/// its own fake Supervisor to be the one that's "live" while it runs. Cargo
/// runs these tests on parallel threads within one process, so without a
/// shared lock one test's URL could still be set when another test's client
/// sends its request. Each test takes this guard for its whole body.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

async fn record(
    State(seen): State<Captured>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<Value>,
) -> &'static str {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    seen.lock()
        .unwrap()
        .push((uri.path().to_string(), auth, body));
    "{}"
}

/// Start a fake Supervisor on a free port. Returns its base URL and the log.
async fn fake_supervisor() -> (String, Captured) {
    let seen: Captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/discovery", post(record))
        .route(
            "/core/api/services/persistent_notification/create",
            post(record),
        )
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), seen)
}

#[tokio::test]
async fn announces_discovery_for_the_byonk_service() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let (url, seen) = fake_supervisor().await;
    // SAFETY: single-threaded test body; the var is read on the next line.
    unsafe { std::env::set_var("BYONK_SUPERVISOR_URL", &url) };
    assert_eq!(supervisor_url(), url);

    let client = reqwest::Client::new();
    announce_discovery(&client, "super-secret").await.unwrap();

    let seen = seen.lock().unwrap();
    let (path, auth, body) = seen.first().expect("one request");
    assert_eq!(path, "/discovery");
    assert_eq!(auth.as_deref(), Some("Bearer super-secret"));
    assert_eq!(body["service"], "byonk");
    assert!(
        body["config"].is_object(),
        "config must be present, may be empty"
    );
}

#[tokio::test]
async fn first_install_notification_says_finish_setup() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let (url, seen) = fake_supervisor().await;
    unsafe { std::env::set_var("BYONK_SUPERVISOR_URL", &url) };

    let client = reqwest::Client::new();
    notify_restart(&client, "tok", None, "0.18.0")
        .await
        .unwrap();

    let seen = seen.lock().unwrap();
    let (path, _, body) = seen.first().expect("one request");
    assert_eq!(path, "/core/api/services/persistent_notification/create");
    assert_eq!(body["notification_id"], "byonk_integration");
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("0.18.0"),
        "message names the version: {message}"
    );
    assert!(
        message.to_lowercase().contains("restart"),
        "message asks for a restart: {message}"
    );
}

#[tokio::test]
async fn update_notification_names_both_versions() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let (url, seen) = fake_supervisor().await;
    unsafe { std::env::set_var("BYONK_SUPERVISOR_URL", &url) };

    let client = reqwest::Client::new();
    notify_restart(&client, "tok", Some("0.18.0"), "0.19.0")
        .await
        .unwrap();

    let seen = seen.lock().unwrap();
    let message = seen.first().unwrap().2["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        message.contains("0.18.0") && message.contains("0.19.0"),
        "{message}"
    );
}

#[tokio::test]
async fn install_and_announce_notifies_once_then_only_announces() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    use byonk::ha_integration::install_and_announce;

    let (url, seen) = fake_supervisor().await;
    let src = tempfile::TempDir::new().unwrap();
    let ha = tempfile::TempDir::new().unwrap();
    std::fs::write(
        src.path().join("manifest.json"),
        r#"{"domain": "byonk", "version": "0.18.0"}"#,
    )
    .unwrap();

    unsafe {
        std::env::set_var("BYONK_SUPERVISOR_URL", &url);
        std::env::set_var("BYONK_INTEGRATION_SRC", src.path());
        std::env::set_var("BYONK_HA_CONFIG_DIR", ha.path());
        std::env::set_var("SUPERVISOR_TOKEN", "tok");
    }

    install_and_announce().await;
    install_and_announce().await;

    let paths: Vec<String> = seen.lock().unwrap().iter().map(|r| r.0.clone()).collect();
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.contains("persistent_notification"))
            .count(),
        1,
        "the restart notification is posted only when something changed: {paths:?}"
    );
    assert_eq!(
        paths.iter().filter(|p| *p == "/discovery").count(),
        2,
        "discovery is announced on every start: {paths:?}"
    );
}
