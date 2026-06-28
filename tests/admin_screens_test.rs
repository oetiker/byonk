mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_screens_lists_screens_and_enums() {
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();

    let screens = json["screens"].as_array().unwrap();
    assert!(screens.iter().any(|s| s["name"] == "transit"));
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
#[ignore] // un-ignored in Task 12 once transit.lua has @params
async fn test_transit_has_station_param_after_headers_added() {
    // After Task 12 adds @params headers, transit exposes a `station` param.
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    let json: serde_json::Value = resp.json();
    let transit = json["screens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "transit")
        .unwrap();
    let params = transit["params"].as_array().unwrap();
    assert!(params.iter().any(|p| p["name"] == "station"));
}
