//! Tests for /api/image/{hash} endpoint.

mod common;

use axum::http::StatusCode;
use common::{fixtures, fixtures::macs, TestApp};

#[tokio::test]
async fn test_image_retrieval() {
    let app = TestApp::new();

    // First get display to generate content
    let api_key = app.register_device(macs::GRAY_DEVICE).await;
    let headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let display_response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;
    let image_url = common::assert_valid_display_response(&display_response);

    // Extract path from URL (e.g., http://localhost:3000/api/image/abc123.png -> /api/image/abc123.png)
    let path = image_url
        .split("localhost:3000")
        .nth(1)
        .expect("Should have path after host");

    // Fetch the image
    let image_response = app.get(path).await;

    common::assert_png(&image_response);
    assert!(
        image_response.body.len() > 100,
        "PNG should have reasonable size"
    );
}

#[tokio::test]
async fn test_image_not_found() {
    let app = TestApp::new();

    let response = app.get("/api/image/nonexistent123.png").await;

    common::assert_status(&response, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_image_without_png_extension() {
    let app = TestApp::new();

    // Generate content first
    let api_key = app.register_device(macs::GRAY_DEVICE).await;
    let headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let display_response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;
    let json: serde_json::Value = display_response.json();
    let hash = json["filename"].as_str().unwrap();

    // Fetch without .png extension (should still work - extension is stripped)
    let response = app.get(&format!("/api/image/{}", hash)).await;

    common::assert_png(&response);
}

#[tokio::test]
async fn test_image_content_type_header() {
    let app = TestApp::new();

    let api_key = app.register_device(macs::GRAY_DEVICE).await;
    let headers = fixtures::display_headers(macs::GRAY_DEVICE, &api_key);
    let display_response = app
        .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
        .await;
    let image_url = common::assert_valid_display_response(&display_response);
    let path = image_url.split("localhost:3000").nth(1).unwrap();

    let image_response = app.get(path).await;

    let content_type = image_response
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("image/png"));

    let content_length = image_response
        .headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    assert_eq!(content_length, Some(image_response.body.len()));
}

#[tokio::test]
async fn test_image_different_screen_sizes() {
    let app = TestApp::new();

    let api_key = app.register_device(macs::GRAY_DEVICE).await;

    // Request with different sizes and verify both work
    for (width, height) in [(800, 480), (1872, 1404)] {
        let headers =
            fixtures::display_headers_with_size(macs::GRAY_DEVICE, &api_key, width, height);
        let display_response = app
            .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
            .await;
        let image_url = common::assert_valid_display_response(&display_response);
        let path = image_url.split("localhost:3000").nth(1).unwrap();

        let image_response = app.get(path).await;
        common::assert_png(&image_response);
    }
}
