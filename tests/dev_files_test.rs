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
async fn copy_move_validate_check() {
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
            "/dev/api/screen/move",
            &[AUTH],
            r#"{"ref":"local/mine","to_handle":"local","to_name":"yours"}"#,
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

/// The dev UI builds its dither select and its tuning defaults from
/// `/dev/api/screens`. Before this the page carried its own copy of both,
/// and a copy that drifted made the preview render with values the engine
/// never used. Assert the list is complete and that the defaults arrive as
/// numbers, not as the strings the old hard-coded table held.
#[tokio::test]
async fn screens_lists_every_dither_algorithm_with_numeric_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());

    let body = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json::<serde_json::Value>();

    let algos = body["dither_algorithms"].as_array().expect("array");
    let names: Vec<&str> = algos.iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 9, "got {names:?}");
    for expected in [
        "floyd-steinberg",
        "atkinson",
        "atkinson-hybrid",
        "jarvis-judice-ninke",
        "sierra",
        "sierra-two-row",
        "sierra-lite",
        "stucki",
        "burkes",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }

    let fs = algos
        .iter()
        .find(|a| a["name"] == "floyd-steinberg")
        .expect("floyd-steinberg entry");
    assert_eq!(
        fs["defaults"]["noise_scale"].as_f64(),
        Some(8.0),
        "floyd-steinberg noise_scale must be the engine's own default"
    );
    assert_eq!(fs["defaults"]["max_error"].as_f64(), Some(1.0));

    // Sierra Lite is the one algorithm whose noise scale differs from
    // Floyd-Steinberg's, so it proves the value is per-algorithm rather than
    // one constant repeated across the list.
    let sl = algos
        .iter()
        .find(|a| a["name"] == "sierra-lite")
        .expect("sierra-lite entry");
    assert_eq!(sl["defaults"]["noise_scale"].as_f64(), Some(2.5));
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

/// A writable screen repo that exists but holds no screens must still be
/// listed. On a fresh install `local` is exactly that, and if it is missing
/// from `/dev/api/repos` the New and Copy dialogs offer no way to put a
/// screen into the user's own repo — the first screen can only land in
/// `examples`.
#[tokio::test]
async fn empty_writable_repo_is_still_listed() {
    let dir = tempfile::tempdir().unwrap();
    let app = TestApp::new_dev_with_empty_local(dir.path(), "secret");

    let repos = app
        .get_with_headers("/dev/api/repos", &[AUTH])
        .await
        .json::<serde_json::Value>();

    let local = repos
        .as_array()
        .expect("repos is an array")
        .iter()
        .find(|x| x["handle"] == "local");

    assert!(
        local.is_some(),
        "the empty but registered `local` repo is missing from {repos}"
    );
    assert_eq!(
        local.unwrap()["writable"],
        true,
        "`local` must be listed as writable so New/Copy can target it"
    );
}

async fn new_screen(app: &TestApp, name: &str) {
    let r = app
        .post_json(
            "/dev/api/screen",
            &[AUTH],
            &format!(r#"{{"handle":"local","name":"{name}","starter":"minimal"}}"#),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
}

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

#[tokio::test]
async fn raw_get_serves_binary_with_type_and_hardening_headers() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    new_screen(&app, "pic").await;
    std::fs::write(dir.path().join("local/pic/logo.png"), PNG_MAGIC).unwrap();

    let r = app
        .get_with_headers("/dev/api/file/raw?ref=local/pic&name=logo.png", &[AUTH])
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.bytes(), PNG_MAGIC);
    assert_eq!(r.headers["content-type"], "image/png");
    assert_eq!(r.headers["x-content-type-options"], "nosniff");
    assert_eq!(r.headers["content-security-policy"], "sandbox");
    assert_eq!(r.headers["cache-control"], "no-cache");
    assert_eq!(r.headers["etag"].to_str().unwrap().len(), 64);

    // A text file is served too, and svg gets its image type.
    let t = app
        .get_with_headers("/dev/api/file/raw?ref=local/pic&name=script.lua", &[AUTH])
        .await;
    assert_eq!(t.status, StatusCode::OK);
    std::fs::write(dir.path().join("local/pic/a.svg"), "<svg/>").unwrap();
    let v = app
        .get_with_headers("/dev/api/file/raw?ref=local/pic&name=a.svg", &[AUTH])
        .await;
    assert_eq!(v.headers["content-type"], "image/svg+xml");
    std::fs::write(dir.path().join("local/pic/x.bin"), [1u8]).unwrap();
    let u = app
        .get_with_headers("/dev/api/file/raw?ref=local/pic&name=x.bin", &[AUTH])
        .await;
    assert_eq!(u.headers["content-type"], "application/octet-stream");
}

#[tokio::test]
async fn raw_get_missing_is_404_and_traversal_is_400() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    new_screen(&app, "pic").await;
    let m = app
        .get_with_headers("/dev/api/file/raw?ref=local/pic&name=nope.png", &[AUTH])
        .await;
    assert_eq!(m.status, StatusCode::NOT_FOUND);
    let t = app
        .get_with_headers("/dev/api/file/raw?ref=local/pic&name=../x.png", &[AUTH])
        .await;
    assert_eq!(t.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn raw_put_accepts_a_body_over_axums_2mb_default_and_rejects_over_5mib() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    new_screen(&app, "pic").await;

    let big = vec![0xABu8; 4 * 1024 * 1024];
    let w = app
        .put_bytes(
            "/dev/api/file/raw?ref=local/pic&name=big.bin",
            &[AUTH, ("If-Match", "*")],
            big.clone(),
        )
        .await;
    assert_eq!(w.status, StatusCode::NO_CONTENT);
    assert_eq!(
        std::fs::read(dir.path().join("local/pic/big.bin")).unwrap(),
        big
    );

    let huge = vec![0u8; 5 * 1024 * 1024 + 1];
    let w = app
        .put_bytes(
            "/dev/api/file/raw?ref=local/pic&name=huge.bin",
            &[AUTH, ("If-Match", "*")],
            huge,
        )
        .await;
    assert_eq!(w.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!dir.path().join("local/pic/huge.bin").exists());
}

#[tokio::test]
async fn raw_put_if_match_rules_and_binary_replace() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    new_screen(&app, "pic").await;
    let url = "/dev/api/file/raw?ref=local/pic&name=logo.png";

    // Missing If-Match -> 428; non-ASCII -> 400.
    assert_eq!(
        app.put_bytes(url, &[AUTH], PNG_MAGIC.to_vec()).await.status,
        StatusCode::PRECONDITION_REQUIRED
    );

    // `*` creates, then 412s on the existing file; bytes are verbatim.
    let c = app
        .put_bytes(url, &[AUTH, ("If-Match", "*")], PNG_MAGIC.to_vec())
        .await;
    assert_eq!(c.status, StatusCode::NO_CONTENT);
    let etag = c.headers["etag"].to_str().unwrap().to_string();
    assert_eq!(
        std::fs::read(dir.path().join("local/pic/logo.png")).unwrap(),
        PNG_MAGIC
    );
    assert_eq!(
        app.put_bytes(url, &[AUTH, ("If-Match", "*")], vec![1])
            .await
            .status,
        StatusCode::PRECONDITION_FAILED
    );

    // A wrong etag 412s; the right one replaces the (binary) file.
    assert_eq!(
        app.put_bytes(url, &[AUTH, ("If-Match", "deadbeef")], vec![2])
            .await
            .status,
        StatusCode::PRECONDITION_FAILED
    );
    let r = app
        .put_bytes(url, &[AUTH, ("If-Match", &etag)], vec![0xFF, 0xFE, 0x00])
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_ne!(r.headers["etag"].to_str().unwrap(), etag);
    assert_eq!(
        std::fs::read(dir.path().join("local/pic/logo.png")).unwrap(),
        vec![0xFF, 0xFE, 0x00]
    );

    // The text route still refuses to overwrite a binary asset.
    let t = app
        .put_text(
            "/dev/api/file?ref=local/pic&name=logo.png",
            &[AUTH, ("If-Match", "*")],
            "x",
        )
        .await;
    assert_eq!(t.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn raw_put_refuses_core_files_and_read_only_repos() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    new_screen(&app, "pic").await;
    let before = std::fs::read(dir.path().join("local/pic/script.lua")).unwrap();
    let w = app
        .put_bytes(
            "/dev/api/file/raw?ref=local/pic&name=Script.lua",
            &[AUTH, ("If-Match", "*")],
            b"x".to_vec(),
        )
        .await;
    assert_eq!(w.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        std::fs::read(dir.path().join("local/pic/script.lua")).unwrap(),
        before
    );

    let ro = app
        .put_bytes(
            "/dev/api/file/raw?ref=byonk-builtin/default&name=a.png",
            &[AUTH, ("If-Match", "*")],
            vec![1],
        )
        .await;
    assert_eq!(ro.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn rename_route_status_codes() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    new_screen(&app, "pic").await;
    std::fs::write(dir.path().join("local/pic/a.png"), PNG_MAGIC).unwrap();
    std::fs::write(dir.path().join("local/pic/b.png"), [1u8]).unwrap();
    let rename =
        |from: &str, to: &str, r: &str| format!(r#"{{"ref":"{r}","from":"{from}","to":"{to}"}}"#);
    let url = "/dev/api/file/rename";

    let ok = app
        .post_json(url, &[AUTH], &rename("a.png", "img/c.png", "local/pic"))
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);
    assert_eq!(
        std::fs::read(dir.path().join("local/pic/img/c.png")).unwrap(),
        PNG_MAGIC
    );
    assert!(!dir.path().join("local/pic/a.png").exists());

    let exists = app
        .post_json(url, &[AUTH], &rename("b.png", "img/c.png", "local/pic"))
        .await;
    assert_eq!(exists.status, StatusCode::PRECONDITION_FAILED);
    let missing = app
        .post_json(url, &[AUTH], &rename("zzz.png", "q.png", "local/pic"))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let core_from = app
        .post_json(url, &[AUTH], &rename("script.lua", "s.lua", "local/pic"))
        .await;
    assert_eq!(core_from.status, StatusCode::UNPROCESSABLE_ENTITY);
    let core_to = app
        .post_json(url, &[AUTH], &rename("b.png", "meta.yaml", "local/pic"))
        .await;
    assert_eq!(core_to.status, StatusCode::UNPROCESSABLE_ENTITY);
    let trav = app
        .post_json(url, &[AUTH], &rename("b.png", "../x.png", "local/pic"))
        .await;
    assert_eq!(trav.status, StatusCode::BAD_REQUEST);
    let ro = app
        .post_json(url, &[AUTH], &rename("a", "b", "byonk-builtin/default"))
        .await;
    assert_eq!(ro.status, StatusCode::CONFLICT);
    let no_auth = app
        .post_json(url, &[], &rename("a", "b", "local/pic"))
        .await;
    assert_eq!(no_auth.status, StatusCode::UNAUTHORIZED);
}
