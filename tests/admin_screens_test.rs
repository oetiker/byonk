mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// Find a screen by its `handle/path` ref across all package groups.
fn find_screen<'a>(json: &'a serde_json::Value, r#ref: &str) -> Option<&'a serde_json::Value> {
    json["packages"]
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

    let packages = json["packages"].as_array().expect("packages array");
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

    // A known builtin screen is present by its qualified ref.
    let hello = find_screen(&json, "byonk-builtin/example/hello").expect("hello screen present");
    assert_eq!(hello["title"], "Hello World");

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

#[tokio::test]
async fn test_gphoto_screen_exposes_its_params() {
    // The gphoto screen declares params in its meta.yaml; they surface on the ref.
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    let json: serde_json::Value = resp.json();

    let gphoto = find_screen(&json, "byonk-builtin/useful/gphoto").expect("gphoto screen present");
    let params = gphoto["params"].as_array().expect("params array");
    assert!(
        params.iter().any(|p| p["name"] == "album_url"),
        "gphoto exposes its album_url param"
    );
}

#[tokio::test]
async fn test_swiss_departure_board_has_station_param() {
    // meta.yaml-declared params reach the admin listing under the qualified ref.
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    let json: serde_json::Value = resp.json();

    let transit = find_screen(&json, "byonk-builtin/useful/swiss-departure-board")
        .expect("swiss-departure-board screen present");
    let params = transit["params"].as_array().unwrap();
    assert!(params.iter().any(|p| p["name"] == "station"));
}

#[tokio::test]
async fn test_packages_lists_builtin_with_redaction() {
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/packages", &[AUTH]).await;
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
    let resp = app.get("/api/admin/packages").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}
