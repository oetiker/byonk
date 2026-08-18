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

/// Pins `handle_display`'s pre-script measured-colour candidate array
/// (`src/api/display.rs`, `[SRC_DEV_OVERRIDE, SRC_PANEL_ACTUAL,
/// SRC_MEASURED_HEADER]`) at the real HTTP call site, not just via a
/// hand-built array inside a unit test.
///
/// Note on technique: `/api/display`'s PNG output always uses the OFFICIAL
/// palette as its PLTE (`src/api/display.rs`, `render_png_from_svg(..,
/// false, ..)` — "production always uses official colors"), by design: the
/// device maps palette index -> physical ink regardless of what byonk
/// measured. So which measured-colour candidate won can NOT be observed by
/// decoding PLTE on this path (unlike the authoring path in
/// `src/services/screen_store.rs`, which sets `use_actual = true` and
/// really does put the winning candidate's RGB values in PLTE). The
/// measured colours only affect *dithering*, i.e. which palette index each
/// pixel is assigned — so this test instead holds the panel's
/// `colors_actual` fixed and varies the `Measured-Colors` header between
/// two very different values, then asserts the two renders are
/// byte-identical: if the panel is really winning, the header is inert and
/// can't change the output at all. If `SRC_PANEL_ACTUAL` and
/// `SRC_MEASURED_HEADER` were swapped in the array, the header would win
/// instead and the two renders would differ (the official palette's first
/// and last entries are pure black/white and therefore B&W-forced
/// regardless of source — see `build_eink_palette` — so the two header
/// variants deliberately disagree only on the two non-B&W entries, the
/// ones that variant actually exercises).
#[tokio::test]
async fn test_display_panel_colors_actual_wins_over_measured_colors_header() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = r##"
registration:
  enabled: true
auth_mode: api_key
panels:
  test_panel:
    name: "Test Panel"
    colors: "#000000,#555555,#AAAAAA,#FFFFFF"
    colors_actual: "#000000,#303030,#808080,#FFFFFF"
devices:
  "TE:ST:PL:TE:00:01":
    screen: byonk-builtin/calibration/grey
    panel: test_panel
"##;
    let (app, _config_path) = TestApp::new_with_config_yaml(yaml, dir.path());

    let mac = "TE:ST:PL:TE:00:01";
    let api_key = app.register_device(mac).await;

    async fn fetch_display_png(
        app: &TestApp,
        mac: &str,
        api_key: &str,
        measured_colors_header: &str,
    ) -> Vec<u8> {
        let mut headers = fixtures::display_headers(mac, api_key);
        headers.push(("Measured-Colors", measured_colors_header.to_string()));
        let display_response = app
            .get_with_headers("/api/display", &fixtures::as_str_pairs(&headers))
            .await;
        let image_url = common::assert_valid_display_response(&display_response);
        let path = image_url
            .split("localhost:3000")
            .nth(1)
            .expect("Should have path after host");
        let image_response = app.get(path).await;
        common::assert_png(&image_response);
        image_response.body
    }

    // Two headers, deliberately far apart on the two non-B&W entries
    // (index 1 and 2) — must have zero effect on the output if the panel
    // is correctly winning.
    let png_header_a =
        fetch_display_png(&app, mac, &api_key, "#000000,#707070,#C0C0C0,#FFFFFF").await;
    let png_header_b =
        fetch_display_png(&app, mac, &api_key, "#000000,#101010,#909090,#FFFFFF").await;

    assert_eq!(
        png_header_a, png_header_b,
        "the Measured-Colors header must be inert once panel.colors_actual is \
         present — the two renders differ, which means the header (not the \
         panel) won the measured-colour chain"
    );
}
