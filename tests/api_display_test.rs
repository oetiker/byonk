//! Tests for /api/display endpoint.

mod common;

use axum::http::StatusCode;
use byonk::services::DeviceRegistry;
use common::{fixtures, fixtures::macs, TestApp};
use ed25519_dalek::{Signer, SigningKey};

#[tokio::test]
async fn test_display_auto_registers_device() {
    // Use app without registration to test auto-registration behavior
    let app = TestApp::new_without_registration();

    // Use display endpoint with a new device (auto-registration)
    let headers = fixtures::display_headers(macs::UNKNOWN_DEVICE, "any-api-key");
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_ok(&response);
    let image_url = common::assert_valid_display_response(&response);
    assert!(!image_url.is_empty(), "Should have image URL");
}

#[tokio::test]
async fn test_display_with_registered_device() {
    let app = TestApp::new();

    // First register device
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    // Now request display
    let headers = fixtures::display_headers(macs::HELLO_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_ok(&response);
    let image_url = common::assert_valid_display_response(&response);
    assert!(image_url.contains("/api/image/"));
    assert!(image_url.ends_with(".png"));
}

#[tokio::test]
async fn test_display_missing_access_token() {
    let app = TestApp::new();

    // Missing Access-Token header
    let headers = [
        ("ID", macs::TEST_DEVICE),
        ("Width", "800"),
        ("Height", "480"),
    ];

    let response = app.get_with_headers("/api/display", &headers).await;

    common::assert_status(&response, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = response.json();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Missing required header: Access-Token"));
}

#[tokio::test]
async fn test_display_missing_id_header() {
    let app = TestApp::new();

    // Missing ID header
    let headers = [
        ("Access-Token", "some-token"),
        ("Width", "800"),
        ("Height", "480"),
    ];

    let response = app.get_with_headers("/api/display", &headers).await;

    common::assert_status(&response, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = response.json();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Missing required header: ID"));
}

#[tokio::test]
async fn test_display_uses_default_dimensions() {
    let app = TestApp::new();

    // Register device first
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    // Request without Width/Height headers (should use defaults 800x480)
    let headers = [
        ("ID", macs::HELLO_DEVICE),
        ("Access-Token", api_key.as_str()),
        ("Host", "localhost:3000"),
    ];

    let response = app.get_with_headers("/api/display", &headers).await;

    common::assert_ok(&response);
    common::assert_valid_display_response(&response);
}

#[tokio::test]
async fn test_display_clamps_excessive_dimensions() {
    let app = TestApp::new();

    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    // Request with excessive dimensions (should be clamped to max)
    let headers = fixtures::display_headers_with_size(macs::HELLO_DEVICE, &api_key, 5000, 5000);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    // Should succeed with clamped dimensions (defaults to 800x480 when invalid)
    common::assert_ok(&response);
    common::assert_valid_display_response(&response);
}

#[tokio::test]
async fn test_display_updates_device_metadata() {
    // TEST_DEVICE is not in config.devices, so use app without registration
    let app = TestApp::new_without_registration();

    let api_key = app.register_device(macs::TEST_DEVICE).await;

    // Request with battery and RSSI
    let headers = [
        ("ID", macs::TEST_DEVICE),
        ("Access-Token", api_key.as_str()),
        ("Width", "800"),
        ("Height", "480"),
        ("Battery-Voltage", "3.95"),
        ("RSSI", "-42"),
        ("Host", "localhost:3000"),
    ];

    let response = app.get_with_headers("/api/display", &headers).await;
    common::assert_ok(&response);

    // Verify device metadata was updated
    let device = app
        .registry
        .find_by_id(&byonk::models::DeviceId::new(macs::TEST_DEVICE))
        .await
        .unwrap()
        .expect("Device should exist");

    assert_eq!(device.battery_voltage, Some(3.95));
    assert_eq!(device.rssi, Some(-42));
}

#[tokio::test]
async fn test_display_with_graytest_screen() {
    let app = TestApp::new();

    let api_key = app.register_device(macs::GRAY_DEVICE).await;

    let headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_ok(&response);
    let json: serde_json::Value = response.json();

    // graytest has default_refresh of 3600
    assert_eq!(json["status"], 0);
    assert!(json["refresh_rate"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_display_content_hash_is_deterministic() {
    let app = TestApp::new();

    let api_key = app.register_device(macs::GRAY_DEVICE).await;

    let headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let headers_ref = fixtures::as_str_pairs(&headers);

    // Make two requests
    let response1 = app.get_with_headers("/api/display", &headers_ref).await;
    let response2 = app.get_with_headers("/api/display", &headers_ref).await;

    let json1: serde_json::Value = response1.json();
    let json2: serde_json::Value = response2.json();

    // For graytest (static content), hash should be the same
    // Note: hello screen has time-dependent content so hashes would differ
    assert_eq!(json1["filename"], json2["filename"]);
}

// --- Ed25519 authentication tests ---

/// Helper to generate Ed25519 signature headers for testing
fn ed25519_headers(
    mac: &str,
    api_key: &str,
    signing_key: &SigningKey,
    timestamp_ms: u64,
) -> Vec<(&'static str, String)> {
    let public_key = signing_key.verifying_key();
    let pk_hex = hex::encode(public_key.as_bytes());

    // Build message: timestamp_ms (8 bytes BE) || public_key (32 bytes)
    let mut message = Vec::with_capacity(40);
    message.extend_from_slice(&timestamp_ms.to_be_bytes());
    message.extend_from_slice(public_key.as_bytes());

    let signature = signing_key.sign(&message);
    let sig_hex = hex::encode(signature.to_bytes());

    vec![
        ("ID", mac.to_string()),
        ("Access-Token", api_key.to_string()),
        ("X-Public-Key", pk_hex),
        ("X-Signature", sig_hex),
        ("X-Timestamp", timestamp_ms.to_string()),
        ("Width", "800".to_string()),
        ("Height", "480".to_string()),
        ("Host", "localhost:3000".to_string()),
    ]
}

#[tokio::test]
async fn test_display_ed25519_valid_signature() {
    let app = TestApp::new();
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;

    let headers = ed25519_headers(macs::HELLO_DEVICE, &api_key, &signing_key, timestamp_ms);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_ok(&response);
    common::assert_valid_display_response(&response);
}

#[tokio::test]
async fn test_display_ed25519_invalid_signature() {
    let app = TestApp::new();
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;

    let mut headers = ed25519_headers(macs::HELLO_DEVICE, &api_key, &signing_key, timestamp_ms);
    // Corrupt the signature
    headers[3] = ("X-Signature", "00".repeat(64));

    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_status(&response, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_display_ed25519_expired_timestamp() {
    let app = TestApp::new();
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    // Timestamp 2 minutes in the past
    let timestamp_ms = (chrono::Utc::now().timestamp_millis() - 120_000) as u64;

    let headers = ed25519_headers(macs::HELLO_DEVICE, &api_key, &signing_key, timestamp_ms);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_status(&response, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_display_fallback_to_api_key() {
    // Without Ed25519 headers, should still work with just Access-Token
    let app = TestApp::new();
    let api_key = app.register_device(macs::HELLO_DEVICE).await;

    let headers = fixtures::display_headers(macs::HELLO_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;

    common::assert_ok(&response);
    common::assert_valid_display_response(&response);
}
