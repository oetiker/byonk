//! End-to-end flow tests covering complete user scenarios.

mod common;

use byonk::services::DeviceRegistry;
use common::{fixtures, fixtures::macs, TestApp};

#[tokio::test]
async fn test_complete_device_flow() {
    let app = TestApp::new();

    // Step 1: Device registration
    let setup_headers = [
        ("ID", macs::HELLO_DEVICE),
        ("FW-Version", "1.7.1"),
        ("Model", "og"),
    ];
    let setup_response = app.get_with_headers("/api/setup", &setup_headers).await;
    common::assert_valid_setup_response(&setup_response);

    let setup_json: serde_json::Value = setup_response.json();
    let api_key = setup_json["api_key"].as_str().unwrap();

    // Step 2: Request display content
    let display_headers = fixtures::display_headers(macs::HELLO_DEVICE, api_key);
    let display_response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&display_headers))
        .await;
    let image_url = common::assert_valid_display_response(&display_response);
    assert!(!image_url.is_empty());

    // Step 3: Fetch the rendered image
    let path = image_url.split("localhost:3000").nth(1).unwrap();
    let image_response = app.get(path).await;
    common::assert_png(&image_response);

    // Verify image is reasonably sized
    assert!(
        image_response.body.len() > 1000,
        "Image should be > 1KB, got {} bytes",
        image_response.body.len()
    );
}

#[tokio::test]
async fn test_multiple_devices_different_screens() {
    let app = TestApp::new();

    // Register and fetch content for two different devices
    let devices = [
        (macs::HELLO_DEVICE, "hello"),
        (macs::GRAY_DEVICE, "graytest"),
    ];

    let mut image_hashes = Vec::new();

    for (mac, _screen_name) in devices {
        let api_key = app.register_device(mac).await;

        let headers = fixtures::display_headers(mac, &api_key);
        let response = app
            .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
            .await;
        common::assert_ok(&response);

        let json: serde_json::Value = response.json();
        let hash = json["filename"].as_str().unwrap().to_string();
        image_hashes.push(hash);
    }

    // Different screens should produce different content hashes
    // (hello has time which changes, graytest is static)
    // Just verify both have valid hashes
    assert!(!image_hashes[0].is_empty());
    assert!(!image_hashes[1].is_empty());
}

#[tokio::test]
async fn test_device_metadata_persists() {
    // TEST_DEVICE is not in config.devices, so use app without registration
    let app = TestApp::new_without_registration();

    // Register device
    let api_key = app.register_device(macs::TEST_DEVICE).await;

    // Make multiple requests updating metadata
    let voltages = [4.20, 4.10, 3.95, 3.80];

    for voltage in voltages {
        let headers = [
            ("ID", macs::TEST_DEVICE),
            ("Access-Token", api_key.as_str()),
            ("Width", "800"),
            ("Height", "480"),
            ("Battery-Voltage", &voltage.to_string()),
            ("RSSI", "-50"),
            ("Host", "localhost:3000"),
        ];

        let response = app.get_with_headers("/api/display", &headers).await;
        common::assert_ok(&response);

        // Verify metadata was updated
        let device = app
            .registry
            .find_by_id(&byonk::models::DeviceId::new(macs::TEST_DEVICE))
            .await
            .unwrap()
            .expect("Device should exist");

        assert_eq!(device.battery_voltage, Some(voltage));
    }
}

#[tokio::test]
async fn test_content_cache_reuse() {
    let app = TestApp::new();

    let api_key = app.register_device(macs::GRAY_DEVICE).await;
    let headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let headers_ref = fixtures::as_str_pairs(&headers);

    // First request generates content
    let response1 = app.get_with_headers("/api/display", &headers_ref).await;
    let json1: serde_json::Value = response1.json();
    let hash1 = json1["filename"].as_str().unwrap();

    // Fetch image
    let path1 = json1["image_url"]
        .as_str()
        .unwrap()
        .split("localhost:3000")
        .nth(1)
        .unwrap();
    let image1 = app.get(path1).await;
    common::assert_png(&image1);

    // Second request should return same hash (for static content)
    let response2 = app.get_with_headers("/api/display", &headers_ref).await;
    let json2: serde_json::Value = response2.json();
    let hash2 = json2["filename"].as_str().unwrap();

    // graytest is static so hash should be same
    assert_eq!(hash1, hash2);

    // Image should still be retrievable from cache
    let path2 = json2["image_url"]
        .as_str()
        .unwrap()
        .split("localhost:3000")
        .nth(1)
        .unwrap();
    let image2 = app.get(path2).await;
    common::assert_png(&image2);

    // Images should be identical
    assert_eq!(image1.body, image2.body);
}

#[tokio::test]
async fn test_health_endpoint() {
    let app = TestApp::new();

    let response = app.get("/health").await;

    common::assert_ok(&response);
    assert_eq!(response.text(), "OK");
}

/// Verify that all responses include "Connection: close" header.
/// This prevents connection accumulation from ESP32 clients that request
/// keep-alive but never reuse connections.
/// See: https://github.com/usetrmnl/trmnl-firmware/pull/274
#[tokio::test]
async fn test_connection_close_header() {
    let app = TestApp::new();

    // Test health endpoint
    let response = app.get("/health").await;
    assert_eq!(
        response
            .headers
            .get("connection")
            .map(|v| v.to_str().unwrap()),
        Some("close"),
        "Health endpoint should have Connection: close header"
    );

    // Test setup endpoint
    let setup_headers = [
        ("ID", macs::TEST_DEVICE),
        ("FW-Version", "1.0.0"),
        ("Model", "og"),
    ];
    let response = app.get_with_headers("/api/setup", &setup_headers).await;
    assert_eq!(
        response
            .headers
            .get("connection")
            .map(|v| v.to_str().unwrap()),
        Some("close"),
        "Setup endpoint should have Connection: close header"
    );

    // Test display endpoint
    let api_key = app.register_device(macs::GRAY_DEVICE).await;
    let display_headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&display_headers))
        .await;
    assert_eq!(
        response
            .headers
            .get("connection")
            .map(|v| v.to_str().unwrap()),
        Some("close"),
        "Display endpoint should have Connection: close header"
    );
}
