//! Tests for /api/log endpoint.

mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn test_log_submission_with_entries() {
    let app = TestApp::new();

    let body = r#"{"logs": [{"level": "info", "message": "Device started"}, {"level": "error", "message": "WiFi failed"}]}"#;
    let response = app.post_json("/api/log", &[], body).await;

    common::assert_ok(&response);
    let json: serde_json::Value = response.json();
    assert_eq!(json["status"], 200);
    assert_eq!(json["message"], "Logs received");
}

#[tokio::test]
async fn test_log_submission_empty_logs() {
    let app = TestApp::new();

    let body = r#"{"logs": []}"#;
    let response = app.post_json("/api/log", &[], body).await;

    common::assert_ok(&response);
    let json: serde_json::Value = response.json();
    assert_eq!(json["status"], 200);
}

#[tokio::test]
async fn test_log_submission_missing_logs_field() {
    let app = TestApp::new();

    // logs field defaults to empty array
    let body = r#"{}"#;
    let response = app.post_json("/api/log", &[], body).await;

    common::assert_ok(&response);
}

#[tokio::test]
async fn test_log_submission_invalid_json() {
    let app = TestApp::new();

    let body = r#"not valid json"#;
    let response = app.post_json("/api/log", &[], body).await;

    // Should fail to parse JSON (Axum returns 400 for JSON parse errors)
    common::assert_status(&response, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_log_submission_with_complex_entries() {
    let app = TestApp::new();

    let body = r#"{
        "logs": [
            {"level": "debug", "timestamp": 1234567890, "data": {"key": "value"}},
            {"level": "warn", "message": "Low battery", "voltage": 3.2},
            "simple string log entry",
            42
        ]
    }"#;
    let response = app.post_json("/api/log", &[], body).await;

    common::assert_ok(&response);
}
