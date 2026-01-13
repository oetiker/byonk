//! Assertion helpers for tests.

use axum::http::StatusCode;
use pretty_assertions::assert_eq;

use super::app::TestResponse;

/// Assert response has expected status code
pub fn assert_status(response: &TestResponse, expected: StatusCode) {
    assert_eq!(
        response.status, expected,
        "Expected status {}, got {}. Body: {}",
        expected,
        response.status,
        response.text()
    );
}

/// Assert response is OK (200)
pub fn assert_ok(response: &TestResponse) {
    assert_status(response, StatusCode::OK);
}

/// Assert response is a valid PNG image
pub fn assert_png(response: &TestResponse) {
    assert_ok(response);
    assert!(
        response.is_png(),
        "Expected PNG image, got {} bytes starting with {:?}",
        response.body.len(),
        &response.body[..8.min(response.body.len())]
    );

    // Check Content-Type header
    let content_type = response
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        content_type,
        Some("image/png"),
        "Expected Content-Type: image/png"
    );
}

/// Assert JSON response has expected status field
pub fn assert_json_status(response: &TestResponse, expected_status: u16) {
    let json: serde_json::Value = response.json();
    assert_eq!(
        json["status"].as_u64(),
        Some(expected_status as u64),
        "Expected JSON status {}, got {:?}. Full response: {}",
        expected_status,
        json["status"],
        serde_json::to_string_pretty(&json).unwrap()
    );
}

/// Assert setup response is valid
pub fn assert_valid_setup_response(response: &TestResponse) {
    assert_ok(response);
    let json: serde_json::Value = response.json();

    assert_eq!(json["status"], 200);
    assert!(json["api_key"].is_string(), "Expected api_key to be a string");
    assert!(
        json["friendly_id"].is_string(),
        "Expected friendly_id to be a string"
    );

    // API key should be 24 characters
    let api_key = json["api_key"].as_str().unwrap();
    assert_eq!(api_key.len(), 24, "API key should be 24 characters");
}

/// Assert display response is valid and has an image URL
pub fn assert_valid_display_response(response: &TestResponse) -> String {
    assert_ok(response);
    let json: serde_json::Value = response.json();

    // TRMNL expects status=0 for success
    assert_eq!(json["status"], 0, "Expected status=0 for success");
    assert!(json["refresh_rate"].is_u64(), "Expected refresh_rate");
    assert!(json["filename"].is_string(), "Expected filename");

    // Extract image URL if present
    if let Some(url) = json["image_url"].as_str() {
        url.to_string()
    } else {
        // skip_update case - no image URL
        String::new()
    }
}

/// Assert display response indicates skip_update
pub fn assert_skip_update_response(response: &TestResponse) {
    assert_ok(response);
    let json: serde_json::Value = response.json();

    assert_eq!(json["status"], 0);
    assert!(json["image_url"].is_null(), "Expected no image_url for skip_update");
    assert_eq!(json["filename"], "unchanged");
}
