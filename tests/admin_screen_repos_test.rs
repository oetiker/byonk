mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_add_package_persists_and_lists_with_token_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .post_json(
            "/api/admin/screen-repos",
            &[AUTH],
            r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1","token":"secret-token"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());

    // GET /packages shows the new handle with token_set true.
    let listed = app
        .get_with_headers("/api/admin/screen-repos", &[AUTH])
        .await;
    assert_eq!(listed.status, StatusCode::OK);
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["handle"] == "weather")
        .expect("weather package present");
    assert_eq!(row["token_set"], true);
    assert_eq!(row["repo"], "github.com/x/y");
    assert_eq!(row["pin"], "v1");

    // Token is never echoed anywhere, including /config.
    let cfg = app.get_with_headers("/api/admin/config", &[AUTH]).await;
    assert!(
        !cfg.text().contains("secret-token"),
        "token leaked into /config response: {}",
        cfg.text()
    );
}

#[tokio::test]
async fn test_add_package_duplicate_handle_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    let body = r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1"}"#;
    let first = app
        .post_json("/api/admin/screen-repos", &[AUTH], body)
        .await;
    assert_eq!(first.status, StatusCode::OK);

    let second = app
        .post_json("/api/admin/screen-repos", &[AUTH], body)
        .await;
    assert_eq!(second.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_add_package_builtin_handle_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .post_json(
            "/api/admin/screen-repos",
            &[AUTH],
            r#"{"handle":"byonk-builtin","repo":"github.com/x/y"}"#,
        )
        .await;
    assert!(resp.status.is_client_error(), "status: {}", resp.status);
}

#[tokio::test]
async fn test_delete_builtin_package_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .delete("/api/admin/screen-repos/byonk-builtin", &[AUTH])
        .await;
    assert!(resp.status.is_client_error(), "status: {}", resp.status);
}

#[tokio::test]
async fn test_delete_unreferenced_package_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .post_json(
            "/api/admin/screen-repos",
            &[AUTH],
            r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let listed = app
        .get_with_headers("/api/admin/screen-repos", &[AUTH])
        .await;
    let json: serde_json::Value = listed.json();
    assert!(json
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["handle"] == "weather"));

    // No device references `weather/...`: delete succeeds.
    let del = app.delete("/api/admin/screen-repos/weather", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::OK, "body: {}", del.text());

    let listed = app
        .get_with_headers("/api/admin/screen-repos", &[AUTH])
        .await;
    let json: serde_json::Value = listed.json();
    assert!(!json
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["handle"] == "weather"));
}

#[tokio::test]
async fn test_delete_package_referenced_by_device_is_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    // Register the package.
    let resp = app
        .post_json(
            "/api/admin/screen-repos",
            &[AUTH],
            r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    // Directly inject a device referencing `weather/forecast` into the config
    // file (bypassing the devices API, which validates screen resolution
    // against the loader — the package was never actually fetched here).
    let yaml = std::fs::read_to_string(&path).unwrap();
    let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    let mut device = serde_yaml::Mapping::new();
    device.insert("screen".into(), "weather/forecast".into());
    value
        .as_mapping_mut()
        .unwrap()
        .entry("devices".into())
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
        .as_mapping_mut()
        .unwrap()
        .insert(
            "AA:BB:CC:DD:EE:FF".into(),
            serde_yaml::Value::Mapping(device),
        );
    std::fs::write(&path, serde_yaml::to_string(&value).unwrap()).unwrap();

    // Force a reload by hitting a benign write (settings patch with no-op
    // isn't enough since it doesn't touch devices); instead, trigger reload
    // via patch_settings which reloads config after writing. We patch a
    // harmless setting to force `reload_config`.
    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"screen_repo_refresh_interval":1}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());

    let del = app.delete("/api/admin/screen-repos/weather", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::CONFLICT, "body: {}", del.text());
    assert!(
        del.text().contains("AA:BB:CC:DD:EE:FF"),
        "conflict message should name the offending device: {}",
        del.text()
    );
}

#[tokio::test]
async fn test_delete_package_referenced_by_default_device_is_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    // Register the package.
    let resp = app
        .post_json(
            "/api/admin/screen-repos",
            &[AUTH],
            r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    // Point the reserved DEFAULT device at a screen inside the `weather`
    // package's namespace by directly injecting `devices.DEFAULT.screen`
    // into the config file (bypassing `PATCH /devices/DEFAULT`, which
    // validates screen resolution against the loader — the package was
    // never actually fetched here).
    let yaml = std::fs::read_to_string(&path).unwrap();
    let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    let mut device = serde_yaml::Mapping::new();
    device.insert("screen".into(), "weather/forecast".into());
    value
        .as_mapping_mut()
        .unwrap()
        .entry("devices".into())
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
        .as_mapping_mut()
        .unwrap()
        .insert("DEFAULT".into(), serde_yaml::Value::Mapping(device));
    std::fs::write(&path, serde_yaml::to_string(&value).unwrap()).unwrap();

    // Force a reload via a harmless settings patch.
    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"screen_repo_refresh_interval":1}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());

    let del = app.delete("/api/admin/screen-repos/weather", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::CONFLICT, "body: {}", del.text());
    assert!(
        del.text().contains("DEFAULT"),
        "conflict message should name the referencing device: {}",
        del.text()
    );
}

#[tokio::test]
async fn test_delete_missing_package_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app.delete("/api/admin/screen-repos/nope", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_patch_package_preserves_token_and_updates_pin() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/screen-repos",
        &[AUTH],
        r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1","token":"secret-token"}"#,
    )
    .await;

    let patch = app
        .patch_json(
            "/api/admin/screen-repos/weather",
            &[AUTH],
            r#"{"pin":"v2"}"#,
        )
        .await;
    assert_eq!(patch.status, StatusCode::OK, "body: {}", patch.text());

    let listed = app
        .get_with_headers("/api/admin/screen-repos", &[AUTH])
        .await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["handle"] == "weather")
        .expect("weather package present");
    assert_eq!(row["pin"], "v2");
    assert_eq!(row["token_set"], true, "token preserved across patch");

    let cfg = app.get_with_headers("/api/admin/config", &[AUTH]).await;
    assert!(!cfg.text().contains("secret-token"));
}

#[tokio::test]
async fn test_patch_package_builtin_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app
        .patch_json(
            "/api/admin/screen-repos/byonk-builtin",
            &[AUTH],
            r#"{"pin":"v2"}"#,
        )
        .await;
    assert!(resp.status.is_client_error(), "status: {}", resp.status);
}

#[tokio::test]
async fn test_patch_missing_package_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app
        .patch_json("/api/admin/screen-repos/nope", &[AUTH], r#"{"pin":"v2"}"#)
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_missing_package_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app
        .post_json("/api/admin/screen-repos/nope/update", &[AUTH], "")
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_package_and_update_all_return_ok() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/screen-repos",
        &[AUTH],
        r#"{"handle":"weather","repo":"github.com/x/y","pin":"v1"}"#,
    )
    .await;

    let resp = app
        .post_json("/api/admin/screen-repos/weather/update", &[AUTH], "")
        .await;
    assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());

    let resp = app
        .post_json("/api/admin/screen-repos/update", &[AUTH], "")
        .await;
    assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
}

#[tokio::test]
async fn test_packages_list_reports_real_status_for_builtin() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    let listed = app
        .get_with_headers("/api/admin/screen-repos", &[AUTH])
        .await;
    assert_eq!(listed.status, StatusCode::OK);
    let text = listed.text();
    assert!(
        !text.contains("\"token\""),
        "token field leaked into /packages response: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["handle"] == "byonk-builtin")
        .expect("byonk-builtin package present");

    assert_eq!(row["builtin"], true);
    assert_eq!(row["status"], "ready");
    assert_eq!(row["pin_kind"], "embedded");
    assert_eq!(row["token_set"], false);
    assert!(row["resolved_sha"].is_null());
}
