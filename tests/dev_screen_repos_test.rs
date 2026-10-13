//! Screen-repo management for the dev UI: `/dev/api/screen-repos`. The routes
//! share their cores with the admin ones, so these tests pin the dev-side
//! contract (response shape, managed handles, add-on mode, no token echo).

mod common;

use axum::http::StatusCode;
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use common::TestApp;
use std::sync::Arc;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");
const BASE: &str = "/dev/api/screen-repos";
// Fails fast (no network) when the background fetch runs.
const REPO: &str = "file:///nonexistent/byonk-test-repo";

fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    TestApp::new_admin_with_file("secret", dir)
}

fn row<'a>(list: &'a serde_json::Value, handle: &str) -> &'a serde_json::Value {
    list["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["handle"] == handle)
        .unwrap_or_else(|| panic!("no repo {handle} in {list}"))
}

#[tokio::test]
async fn list_has_the_shape_the_ui_needs() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app.get_with_headers(BASE, &[AUTH]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["addon_mode"], false);
    let b = row(&body, "byonk-builtin");
    assert_eq!(b["kind"], "builtin");
    assert_eq!(b["managed"], true);
    assert_eq!(b["writable"], false);
    assert_eq!(b["has_token"], false);
    assert_eq!(b["status"], "ready");
}

#[tokio::test]
async fn add_lists_with_token_never_echoed() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    let r = app
        .post_json(
            BASE,
            &[AUTH],
            &format!(
                r#"{{"handle":"weather","repo":"{REPO}","pin":"v1","token":"tok-SECRET-123"}}"#
            ),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(!r.text().contains("tok-SECRET-123"));
    let added: serde_json::Value = r.json();
    assert_eq!(added["handle"], "weather");
    assert_eq!(added["has_token"], true);
    assert_eq!(added["kind"], "git");
    assert_eq!(added["managed"], false);
    assert_eq!(added["pin"], "v1");

    let l = app.get_with_headers(BASE, &[AUTH]).await;
    assert!(!l.text().contains("tok-SECRET-123"), "{}", l.text());
    let list: serde_json::Value = l.json();
    let w = row(&list, "weather");
    assert_eq!(w["repo"], REPO);
    assert_eq!(w["has_token"], true);
    assert_eq!(w["writable"], false);
    // The token is persisted in config.yaml, just never served.
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("tok-SECRET-123"));
}

#[tokio::test]
async fn add_schemeless_url_is_400_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    let before = std::fs::read_to_string(&path).unwrap();
    let r = app
        .post_json(
            BASE,
            &[AUTH],
            r#"{"handle":"weather","repo":"github.com/x/y"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(r.text().contains("scheme"), "{}", r.text());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn add_without_a_repo_is_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json(BASE, &[AUTH], r#"{"handle":"weather"}"#)
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
}

#[tokio::test]
async fn duplicate_handle_is_409() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let body = format!(r#"{{"handle":"weather","repo":"{REPO}"}}"#);
    assert_eq!(
        app.post_json(BASE, &[AUTH], &body).await.status,
        StatusCode::OK
    );
    assert_eq!(
        app.post_json(BASE, &[AUTH], &body).await.status,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn reserved_handles_are_refused_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    let before = std::fs::read_to_string(&path).unwrap();
    for h in ["byonk-builtin", "local", "examples"] {
        let add = app
            .post_json(
                BASE,
                &[AUTH],
                &format!(r#"{{"handle":"{h}","repo":"{REPO}"}}"#),
            )
            .await;
        assert_eq!(add.status, StatusCode::CONFLICT, "add {h}: {}", add.text());
        let patch = app
            .patch_json(&format!("{BASE}/{h}"), &[AUTH], r#"{"pin":"v2"}"#)
            .await;
        assert_eq!(patch.status, StatusCode::CONFLICT, "patch {h}");
        let del = app.delete(&format!("{BASE}/{h}"), &[AUTH]).await;
        assert_eq!(del.status, StatusCode::CONFLICT, "delete {h}");
        let upd = app
            .post_json(&format!("{BASE}/{h}/update"), &[AUTH], "{}")
            .await;
        assert_eq!(upd.status, StatusCode::CONFLICT, "update {h}");
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn patch_overlays_fields_keeps_token_and_validates_url() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    app.post_json(
        BASE,
        &[AUTH],
        &format!(r#"{{"handle":"weather","repo":"{REPO}","pin":"v1","token":"keepme"}}"#),
    )
    .await;

    let r = app
        .patch_json(&format!("{BASE}/weather"), &[AUTH], r#"{"pin":"v2"}"#)
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j: serde_json::Value = r.json();
    assert_eq!(j["pin"], "v2");
    assert_eq!(j["has_token"], true);
    assert!(std::fs::read_to_string(&path).unwrap().contains("keepme"));

    let bad = app
        .patch_json(
            &format!("{BASE}/weather"),
            &[AUTH],
            r#"{"repo":"github.com/x/y"}"#,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.text());

    // No rename: a body handle that differs from the path is refused.
    let rename = app
        .patch_json(
            &format!("{BASE}/weather"),
            &[AUTH],
            r#"{"handle":"other","pin":"v3"}"#,
        )
        .await;
    assert_eq!(rename.status, StatusCode::BAD_REQUEST, "{}", rename.text());

    let missing = app
        .patch_json(&format!("{BASE}/nope"), &[AUTH], r#"{"pin":"v2"}"#)
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_known_ok_unknown_404() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    app.post_json(
        BASE,
        &[AUTH],
        &format!(r#"{{"handle":"weather","repo":"{REPO}"}}"#),
    )
    .await;
    let r = app
        .post_json(&format!("{BASE}/weather/update"), &[AUTH], "{}")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json::<serde_json::Value>()["handle"], "weather");
    let r = app
        .post_json(&format!("{BASE}/nope/update"), &[AUTH], "{}")
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_removes_the_repo_and_unknown_is_404() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    app.post_json(
        BASE,
        &[AUTH],
        &format!(r#"{{"handle":"weather","repo":"{REPO}"}}"#),
    )
    .await;
    let d = app.delete(&format!("{BASE}/weather"), &[AUTH]).await;
    assert_eq!(d.status, StatusCode::OK, "{}", d.text());
    assert_eq!(d.json::<serde_json::Value>()["deleted"], "weather");
    let list: serde_json::Value = app.get_with_headers(BASE, &[AUTH]).await.json();
    assert!(!list["repos"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["handle"] == "weather"));
    assert_eq!(
        app.delete(&format!("{BASE}/weather"), &[AUTH]).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn delete_in_use_is_409_naming_every_device() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        "admin:\n  token: secret\nscreen_repos:\n  weather:\n    repo: {REPO}\ndevices:\n  kitchen:\n    screen: weather/forecast\n  hall:\n    screen: weather/radar\n  office:\n    screen: byonk-builtin/default\n"
    );
    let (app, path) = TestApp::new_with_config_yaml(&yaml, dir.path());
    let before = std::fs::read_to_string(&path).unwrap();
    let d = app.delete(&format!("{BASE}/weather"), &[AUTH]).await;
    assert_eq!(d.status, StatusCode::CONFLICT, "{}", d.text());
    let t = d.text();
    assert!(t.contains("kitchen") && t.contains("hall"), "{t}");
    assert!(!t.contains("office"), "{t}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[tokio::test]
async fn add_on_mode_refuses_writes_but_still_lists_and_updates() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        format!("admin:\n  token: secret\nscreen_repos:\n  weather:\n    repo: {REPO}\n"),
    )
    .unwrap();
    let loader = Arc::new(AssetLoader::new(None, None, Some(config_path.clone())));
    let config = AppConfig::load_from_assets(&loader).unwrap();
    let mut state = create_app_state_with_config(loader, config).unwrap();
    state.addon_mode = true;
    let app = TestApp::from_state(state);
    let before = std::fs::read_to_string(&config_path).unwrap();

    let list: serde_json::Value = app.get_with_headers(BASE, &[AUTH]).await.json();
    assert_eq!(list["addon_mode"], true);
    row(&list, "weather");

    let add = app
        .post_json(
            BASE,
            &[AUTH],
            &format!(r#"{{"handle":"x","repo":"{REPO}"}}"#),
        )
        .await;
    assert_eq!(add.status, StatusCode::CONFLICT);
    assert!(add.text().contains("add-on"), "{}", add.text());
    let patch = app
        .patch_json(&format!("{BASE}/weather"), &[AUTH], r#"{"pin":"v2"}"#)
        .await;
    assert_eq!(patch.status, StatusCode::CONFLICT);
    let del = app.delete(&format!("{BASE}/weather"), &[AUTH]).await;
    assert_eq!(del.status, StatusCode::CONFLICT);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);

    // A content refresh is not a config write: it stays available.
    let upd = app
        .post_json(&format!("{BASE}/weather/update"), &[AUTH], "{}")
        .await;
    assert_eq!(upd.status, StatusCode::OK, "{}", upd.text());
}

#[tokio::test]
async fn managed_repos_report_kind_and_writability() {
    let dir = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", dir.path());
    let list: serde_json::Value = app.get_with_headers(BASE, &[AUTH]).await.json();
    let ex = row(&list, "examples");
    assert_eq!(ex["kind"], "examples");
    assert_eq!(ex["managed"], true);
    assert_eq!(ex["writable"], false);

    let dir = tempfile::tempdir().unwrap();
    let app = TestApp::new_dev_with_empty_local(dir.path(), "secret");
    let list: serde_json::Value = app.get_with_headers(BASE, &[AUTH]).await.json();
    let local = row(&list, "local");
    assert_eq!(local["kind"], "local");
    assert_eq!(local["managed"], true);
    assert_eq!(local["writable"], true);
}

#[tokio::test]
async fn empty_string_clears_token_and_pin_absent_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    app.post_json(
        BASE,
        &[AUTH],
        &format!(r#"{{"handle":"weather","repo":"{REPO}","pin":"v1","token":"tok-xyz"}}"#),
    )
    .await;
    let kept = app
        .patch_json(&format!("{BASE}/weather"), &[AUTH], "{}")
        .await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
    let j: serde_json::Value = kept.json();
    assert_eq!(
        (j["has_token"].clone(), j["pin"].clone()),
        (true.into(), "v1".into())
    );

    let r = app
        .patch_json(&format!("{BASE}/weather"), &[AUTH], r#"{"token":""}"#)
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j: serde_json::Value = r.json();
    assert_eq!(j["has_token"], false);
    assert_eq!(j["pin"], "v1");
    let d: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(d["screen_repos"]["weather"].get("token").is_none());
    assert_eq!(d["screen_repos"]["weather"]["pin"], "v1");

    let r = app
        .patch_json(&format!("{BASE}/weather"), &[AUTH], r#"{"pin":""}"#)
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(r.json::<serde_json::Value>()["pin"].is_null());
    let d: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(d["screen_repos"]["weather"].get("pin").is_none());
    assert_eq!(d["screen_repos"]["weather"]["repo"], REPO);
}

#[tokio::test]
async fn add_with_empty_token_or_pin_stores_neither() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    let r = app
        .post_json(
            BASE,
            &[AUTH],
            &format!(r#"{{"handle":"weather","repo":"{REPO}","pin":"","token":""}}"#),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json::<serde_json::Value>()["has_token"], false);
    let d: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(d["screen_repos"]["weather"].get("token").is_none());
    assert!(d["screen_repos"]["weather"].get("pin").is_none());
}

#[tokio::test]
async fn add_rejects_invalid_handles_and_accepts_a_valid_one() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path());
    let before = std::fs::read_to_string(&path).unwrap();
    for bad in ["", "a/b", "a b", "-x", "Up"] {
        let r = app
            .post_json(
                BASE,
                &[AUTH],
                &format!(r#"{{"handle":"{bad}","repo":"{REPO}"}}"#),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad:?}: {}", r.text());
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    let r = app
        .post_json(
            BASE,
            &[AUTH],
            &format!(r#"{{"handle":"my-repo_2","repo":"{REPO}"}}"#),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
}
