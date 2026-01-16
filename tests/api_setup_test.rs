//! Tests for /api/setup endpoint.

mod common;

use axum::http::StatusCode;
use byonk::services::DeviceRegistry;
use common::{fixtures::macs, TestApp};

#[tokio::test]
async fn test_setup_registers_new_device() {
    let app = TestApp::new();

    let headers = [
        ("ID", macs::TEST_DEVICE),
        ("FW-Version", "1.7.1"),
        ("Model", "og"),
    ];

    let response = app.get_with_headers("/api/setup", &headers).await;

    common::assert_ok(&response);
    common::assert_valid_setup_response(&response);

    let json: serde_json::Value = response.json();
    assert_eq!(json["message"], "Device registered successfully");
}

#[tokio::test]
async fn test_setup_returns_existing_device() {
    let app = TestApp::new();

    let headers = [
        ("ID", macs::TEST_DEVICE),
        ("FW-Version", "1.7.1"),
        ("Model", "og"),
    ];

    // First registration
    let response1 = app.get_with_headers("/api/setup", &headers).await;
    common::assert_ok(&response1);
    let json1: serde_json::Value = response1.json();
    let api_key1 = json1["api_key"].as_str().unwrap();

    // Second registration - should return same device
    let response2 = app.get_with_headers("/api/setup", &headers).await;
    common::assert_ok(&response2);
    let json2: serde_json::Value = response2.json();
    let api_key2 = json2["api_key"].as_str().unwrap();

    // API keys should match
    assert_eq!(api_key1, api_key2);
    assert_eq!(json2["message"], "Device already registered");
}

#[tokio::test]
async fn test_setup_missing_id_header() {
    let app = TestApp::new();

    let headers = [("FW-Version", "1.7.1"), ("Model", "og")];

    let response = app.get_with_headers("/api/setup", &headers).await;

    common::assert_status(&response, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = response.json();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Missing required header: ID"));
}

#[tokio::test]
async fn test_setup_defaults_for_optional_headers() {
    let app = TestApp::new();

    // Only ID is required, FW-Version and Model have defaults
    let headers = [("ID", macs::TEST_DEVICE)];

    let response = app.get_with_headers("/api/setup", &headers).await;

    common::assert_ok(&response);
    common::assert_valid_setup_response(&response);
}

#[tokio::test]
async fn test_setup_device_x_model() {
    let app = TestApp::new();

    let headers = [
        ("ID", "AA:BB:CC:DD:EE:FF"),
        ("FW-Version", "2.0.0"),
        ("Model", "x"),
    ];

    let response = app.get_with_headers("/api/setup", &headers).await;

    common::assert_ok(&response);
    common::assert_valid_setup_response(&response);

    // Verify device is stored with correct model
    let device = app
        .registry
        .find_by_id(&byonk::models::DeviceId::new("AA:BB:CC:DD:EE:FF"))
        .await
        .unwrap()
        .expect("Device should be registered");

    assert_eq!(device.model, byonk::models::DeviceModel::X);
}

#[tokio::test]
async fn test_setup_with_trailing_slash() {
    // TRMNL firmware 1.6.9+ sends requests with trailing slashes
    let app = TestApp::new();

    let headers = [
        ("ID", macs::TEST_DEVICE),
        ("FW-Version", "1.6.9"),
        ("Model", "og"),
    ];

    // Request WITH trailing slash should work
    let response = app.get_with_headers("/api/setup/", &headers).await;

    common::assert_ok(&response);
    common::assert_valid_setup_response(&response);
}
