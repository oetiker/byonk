//! `/dev/api` screen and file routes: integration tests against the real
//! `ScreenStore`-backed router (Task 6).

mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// A `TestApp` whose `local` handle is a writable repo at `dir/local`
/// (empty, no screens yet), with dev auth admitted via the admin bearer
/// token (the simplest way through `require_dev` in tests).
fn app(dir: &std::path::Path) -> TestApp {
    let repo_dir = dir.join("local");
    std::fs::create_dir_all(&repo_dir).expect("create local repo dir");
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: local\ndescription: Test fixture.\nauthor: test\nlicense: MIT\n",
    )
    .expect("write manifest");

    let yaml = format!(
        "admin:\n  token: secret\ndevices:\n  DEFAULT:\n    screen: byonk-builtin/default\nscreen_repos:\n  local:\n    path: {}\n",
        repo_dir.display()
    );
    let (app, _config_path) = TestApp::new_with_config_yaml(&yaml, dir);
    app
}

#[tokio::test]
async fn create_list_read_write_delete_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());

    let r = app
        .post_json(
            "/dev/api/screen",
            &[AUTH],
            r#"{"handle":"local","name":"clock","starter":"minimal"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json::<serde_json::Value>()["ref"], "local/clock");

    let repos = app
        .get_with_headers("/dev/api/repos", &[AUTH])
        .await
        .json::<serde_json::Value>();
    assert!(repos
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["handle"] == "local" && x["writable"] == true));

    let files = app
        .get_with_headers("/dev/api/files?ref=local/clock", &[AUTH])
        .await
        .json::<serde_json::Value>();
    assert_eq!(files[0]["name"], "meta.yaml");
    assert_eq!(files[1]["name"], "script.lua");

    let f = app
        .get_with_headers("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH])
        .await;
    assert_eq!(f.status, StatusCode::OK);
    let etag = f.headers["etag"].to_str().unwrap().to_string();
    assert!(f.text().contains("return"));

    let w = app
        .put_text(
            "/dev/api/file?ref=local/clock&name=script.lua",
            &[AUTH, ("If-Match", &etag)],
            "return { data = {} }\n",
        )
        .await;
    assert_eq!(w.status, StatusCode::NO_CONTENT);
    let new_etag = w.headers["etag"].to_str().unwrap().to_string();
    assert_ne!(new_etag, etag);

    let stale = app
        .put_text(
            "/dev/api/file?ref=local/clock&name=script.lua",
            &[AUTH, ("If-Match", &etag)],
            "x",
        )
        .await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);

    let missing = app
        .put_text(
            "/dev/api/file?ref=local/clock&name=script.lua",
            &[AUTH],
            "x",
        )
        .await;
    assert_eq!(missing.status, StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(
        missing.headers["content-type"].to_str().unwrap(),
        "application/json"
    );
    let missing_body: serde_json::Value = missing.json();
    assert_eq!(missing_body["status"], 428);
    assert!(missing_body["error"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("if-match"));

    let created = app
        .put_text(
            "/dev/api/file?ref=local/clock&name=part.svg",
            &[AUTH, ("If-Match", "*")],
            "<svg/>",
        )
        .await;
    assert_eq!(created.status, StatusCode::NO_CONTENT);
    let dup = app
        .put_text(
            "/dev/api/file?ref=local/clock&name=part.svg",
            &[AUTH, ("If-Match", "*")],
            "<svg/>",
        )
        .await;
    assert_eq!(dup.status, StatusCode::PRECONDITION_FAILED);

    let core = app
        .delete("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH])
        .await;
    assert_eq!(core.status, StatusCode::UNPROCESSABLE_ENTITY);
    let extra = app
        .delete("/dev/api/file?ref=local/clock&name=part.svg", &[AUTH])
        .await;
    assert_eq!(extra.status, StatusCode::NO_CONTENT);

    let gone = app.delete("/dev/api/screen?ref=local/clock", &[AUTH]).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    assert_eq!(
        app.get_with_headers("/dev/api/files?ref=local/clock", &[AUTH])
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn copy_rename_validate_check() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let c = app
        .post_json(
            "/dev/api/screen/copy",
            &[AUTH],
            r#"{"ref":"byonk-builtin/default","to_handle":"local","to_name":"mine"}"#,
        )
        .await;
    assert_eq!(c.status, StatusCode::OK);
    let rn = app
        .post_json(
            "/dev/api/screen/rename",
            &[AUTH],
            r#"{"ref":"local/mine","name":"yours"}"#,
        )
        .await;
    assert_eq!(rn.json::<serde_json::Value>()["ref"], "local/yours");
    let v = app
        .post_json(
            "/dev/api/screen/validate",
            &[AUTH],
            r#"{"ref":"local/yours"}"#,
        )
        .await;
    assert_eq!(v.json::<serde_json::Value>()["ok"], true);
    let ck = app
        .post_json(
            "/dev/api/screen/check",
            &[AUTH],
            r#"{"ref":"local/yours","file":"script.lua","text":"return {\nend"}"#,
        )
        .await;
    let j: serde_json::Value = ck.json();
    assert_eq!(j["ok"], false);
    assert_eq!(j["issues"][0]["line"], 2);
}

#[tokio::test]
async fn read_only_repo_write_is_409_and_binary_read_is_415() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let w = app
        .put_text(
            "/dev/api/file?ref=byonk-builtin/default&name=script.lua",
            &[AUTH, ("If-Match", "*")],
            "x",
        )
        .await;
    assert_eq!(w.status, StatusCode::CONFLICT);
    app.post_json(
        "/dev/api/screen",
        &[AUTH],
        r#"{"handle":"local","name":"b","starter":"minimal"}"#,
    )
    .await;
    std::fs::write(
        dir.path().join("local/b/logo.png"),
        [0x89, 0x50, 0xff, 0xfe],
    )
    .unwrap();
    let r = app
        .get_with_headers("/dev/api/file?ref=local/b&name=logo.png", &[AUTH])
        .await;
    assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

/// R2 (site A): `DELETE /dev/api/file` must refuse a mixed-case match
/// against the defining-files list too, not just an exact one. On a
/// case-insensitive filesystem (macOS APFS, Windows) `Script.lua` and
/// `script.lua` name the same real file, so a request for `Script.lua`
/// must be rejected exactly like `script.lua` is (422) — and the real
/// file must still be there afterwards, not merely the status code.
#[tokio::test]
async fn delete_core_file_is_rejected_case_insensitively_and_file_survives() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());

    let r = app
        .post_json(
            "/dev/api/screen",
            &[AUTH],
            r#"{"handle":"local","name":"clock","starter":"minimal"}"#,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);

    let script_path = dir.path().join("local/clock/script.lua");
    assert!(script_path.exists(), "fixture must have created script.lua");

    let d = app
        .delete("/dev/api/file?ref=local/clock&name=Script.lua", &[AUTH])
        .await;
    assert_eq!(d.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        script_path.exists(),
        "script.lua must survive a case-mismatched delete request for Script.lua"
    );
}

/// R3: `GET /dev/api/file` must send `Cache-Control: no-cache` alongside
/// its `ETag` so the browser always revalidates instead of serving a
/// heuristically-cached, possibly stale, body-and-etag pair (RFC 9111
/// §4.2.2) after an external edit.
#[tokio::test]
async fn read_file_response_carries_no_cache() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    app.post_json(
        "/dev/api/screen",
        &[AUTH],
        r#"{"handle":"local","name":"clock","starter":"minimal"}"#,
    )
    .await;

    let f = app
        .get_with_headers("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH])
        .await;
    assert_eq!(f.status, StatusCode::OK);
    assert_eq!(f.headers["cache-control"].to_str().unwrap(), "no-cache");
}

#[tokio::test]
async fn routes_are_gated() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    assert_eq!(
        app.get("/dev/api/repos").await.status,
        StatusCode::UNAUTHORIZED
    );
}
