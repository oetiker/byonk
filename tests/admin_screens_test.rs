mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// Find a screen by its `handle/path` ref across all package groups.
fn find_screen<'a>(json: &'a serde_json::Value, r#ref: &str) -> Option<&'a serde_json::Value> {
    json["screen_repos"]
        .as_array()?
        .iter()
        .flat_map(|p| p["screens"].as_array().into_iter().flatten())
        .find(|s| s["ref"] == r#ref)
}

#[tokio::test]
async fn test_screens_grouped_includes_builtin_with_titles() {
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();

    let packages = json["screen_repos"].as_array().expect("packages array");
    let builtin = packages
        .iter()
        .find(|p| p["handle"] == "byonk-builtin")
        .expect("byonk-builtin package present");

    // Package-level metadata comes from the manifest.
    assert_eq!(builtin["name"], "byonk-builtin");
    assert!(builtin["license"].is_string());

    let screens = builtin["screens"].as_array().expect("screens array");
    assert!(!screens.is_empty(), "builtin ships screens");
    // Every builtin screen is a qualified ref with a non-empty title.
    for s in screens {
        let r#ref = s["ref"].as_str().expect("ref is a string");
        assert!(
            r#ref.starts_with("byonk-builtin/"),
            "ref must be qualified: {ref}"
        );
        assert!(
            !s["title"].as_str().unwrap_or("").is_empty(),
            "title must be non-empty for {ref}"
        );
        assert!(
            s["byonk"].is_string(),
            "byonk requirement present for {ref}"
        );
    }

    // A known builtin screen is present by its qualified ref. (`hello` moved to
    // the `examples` embed in Task 10 and is no longer part of `byonk-builtin`.)
    let default_screen =
        find_screen(&json, "byonk-builtin/default").expect("default screen present");
    assert_eq!(default_screen["title"], "Default");

    // Panels + dither algorithms are still surfaced alongside packages.
    assert!(json["panels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "trmnl_og"));
    assert!(json["dither_algorithms"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d == "atkinson"));
}

#[tokio::test]
async fn test_screens_unauthorized() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/screens").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

// `test_gphoto_screen_exposes_its_params` and
// `test_swiss_departure_board_has_station_param` used to live here, asserting
// that `gphoto`'s `album_url` and `swiss-departure-board`'s `station`
// meta.yaml params surface through this *admin HTTP listing*. Both screens
// moved to the `examples` embed in Task 10. Deliberately deferred (not
// impossible to write today): this is HTTP-level coverage of a registered
// screen repo, so re-adding it now would require standing up a temp-dir
// screen repo fixture (`TestApp::new_admin_with_screens`,
// `tests/common/app.rs`) just to exercise a repo that Task 11 is about to
// register for real. Re-adding it against the real `examples` handle once
// Task 11 lands is less work and tests the actual wiring, so it's deferred
// there rather than duplicated with a throwaway fixture now. `meta.yaml`
// parsing for these two screens (params present, correctly typed) is
// covered directly, without any repo handle, in
// `tests/screen_schemas_test.rs`'s `test_gphoto_params` /
// `test_transit_params`; the generic `params:` → `ParamSchema` parsing
// mechanism itself is covered independently of any bundled screen in
// `src/models/screen_meta.rs`'s unit tests.

#[tokio::test]
async fn test_packages_lists_builtin_with_redaction() {
    let app = TestApp::new_admin("secret");
    let resp = app
        .get_with_headers("/api/admin/screen-repos", &[AUTH])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();

    let packages = json.as_array().expect("packages array");
    let builtin = packages
        .iter()
        .find(|p| p["handle"] == "byonk-builtin")
        .expect("byonk-builtin package present");

    assert_eq!(builtin["builtin"], true, "builtin flag set");
    assert_eq!(
        builtin["token_set"], false,
        "no token configured for builtin"
    );
    assert_eq!(builtin["status"], "ready");
    assert!(
        builtin["screen_count"].as_u64().unwrap_or(0) > 0,
        "builtin reports its screen count"
    );

    // The secret token is never serialized under any key.
    assert!(
        builtin.get("token").is_none(),
        "token must never be present in the response"
    );
}

#[tokio::test]
async fn test_packages_unauthorized() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/screen-repos").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}
