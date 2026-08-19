//! `GET /api/admin/devices/{key}/preview` — the rendered PNG the Home
//! Assistant device page shows.
//!
//! The cache's own rules (fingerprint match, TTL, the refresh-rate floor,
//! eviction) are unit-tested in `services::preview_cache`, where time is
//! injected and nothing has to sleep. What is tested here is the HTTP surface
//! that sits on top of it: auth, key resolution, that a render actually comes
//! back as a PNG, that a failing screen still yields an image, and — via the
//! `X-Byonk-Preview` response header — that the handler consults the cache,
//! re-renders when the device's configuration moves, and honours `?force`.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// An admin app whose reserved DEFAULT device renders `screen`. Static
/// calibration screens are used throughout so a render is reproducible —
/// the embedded default screen paints the wall clock.
fn app_with_default_screen(screen: &str) -> TestApp {
    let asset_loader = Arc::new(AssetLoader::new(None, None, None));
    let mut config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    config.admin.token = Some("secret".to_string());
    config
        .devices
        .get_mut("DEFAULT")
        .expect("embedded config has a reserved DEFAULT device")
        .screen = screen.to_string();
    TestApp::from_config(config)
}

fn cache_state(resp: &common::app::TestResponse) -> &str {
    resp.headers
        .get("x-byonk-preview")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("<missing>")
}

#[tokio::test]
async fn admin_disabled_returns_404() {
    let app = TestApp::new();
    let resp = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_token_returns_401() {
    let app = TestApp::new_admin("secret");
    let resp = app
        .get_with_headers(
            "/api/admin/devices/DEFAULT/preview",
            &[("Authorization", "Bearer nope")],
        )
        .await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_token_returns_401() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/devices/DEFAULT/preview").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

/// A key with no device config has no configured screen, so there is nothing
/// to preview — that is a 404, not an empty image.
#[tokio::test]
async fn unknown_device_returns_404() {
    let app = TestApp::new_admin("secret");
    let resp = app
        .get_with_headers("/api/admin/devices/ZZ:ZZ:ZZ:ZZ:ZZ:ZZ/preview", &[AUTH])
        .await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn default_device_renders_a_png() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let resp = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
    assert!(
        resp.is_png(),
        "body is not a PNG ({} bytes)",
        resp.body.len()
    );
    assert_eq!(
        resp.headers
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
}

/// The camera pulls a frame every few seconds; a browser or proxy caching one
/// of them would outlive the render it came from.
#[tokio::test]
async fn the_response_is_not_cacheable_downstream() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let resp = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(
        resp.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
}

/// First request renders, second is served from the cache.
#[tokio::test]
async fn a_repeat_request_is_served_from_the_cache() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let first = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    let second = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;

    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(cache_state(&first), "miss");
    assert_eq!(cache_state(&second), "hit");
    assert_eq!(first.body, second.body);
}

/// `?force` is the "Refresh preview" button: the screen's data may have moved
/// even though nothing in its configuration did.
#[tokio::test]
async fn force_bypasses_the_cache() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    app.get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    let forced = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview?force=1", &[AUTH])
        .await;

    assert_eq!(forced.status, StatusCode::OK);
    assert!(forced.is_png());
    assert_eq!(cache_state(&forced), "miss");

    // ...and the forced render repopulates the cache rather than emptying it.
    let after = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(cache_state(&after), "hit");
}

/// Changing the device's screen must show up immediately — this is the whole
/// point of a preview on a configuration page.
#[tokio::test]
async fn changing_the_screen_re_renders() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    app.patch_json(
        "/api/admin/devices/DEFAULT",
        &[AUTH],
        r#"{"screen":"byonk-builtin/calibration/grey"}"#,
    )
    .await;
    let grey = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(grey.status, StatusCode::OK);
    assert_eq!(cache_state(&grey), "miss");

    let patched = app
        .patch_json(
            "/api/admin/devices/DEFAULT",
            &[AUTH],
            r#"{"screen":"byonk-builtin/calibration/gamut"}"#,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());

    let gamut = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(gamut.status, StatusCode::OK);
    assert_eq!(
        cache_state(&gamut),
        "miss",
        "a changed screen must not be served from the cache"
    );
    assert_ne!(grey.body, gamut.body, "the preview did not change screens");
}

/// A screen that cannot render still has to produce an image: the device page
/// shows the error the panel would show, rather than a broken-image icon that
/// says nothing about what went wrong.
#[tokio::test]
async fn a_failing_screen_returns_an_error_image() {
    let app = app_with_default_screen("nosuch/screen");
    let resp = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
    assert!(
        resp.is_png(),
        "a failed render must still yield a PNG ({} bytes)",
        resp.body.len()
    );
}

// ---------------------------------------------------------------------------
// View options. These change how the preview is *drawn* and must never change
// what the panel shows, so they live only in the query string — nothing here
// is written back to the device.
// ---------------------------------------------------------------------------

/// `?dither=off` returns the pre-dither, full-colour rasterization. On a
/// calibration screen that is a visibly different image from the dithered one.
#[tokio::test]
async fn dither_off_returns_the_undithered_render() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let dithered = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    let raw = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview?dither=off", &[AUTH])
        .await;

    assert_eq!(raw.status, StatusCode::OK);
    assert!(raw.is_png(), "undithered body is not a PNG");
    assert_ne!(
        dithered.body, raw.body,
        "?dither=off returned the dithered image"
    );
}

/// `?measured=off` draws the spec colours instead of the panel's measured
/// ones. Without a calibration there is nothing to differ, so this asserts the
/// request is accepted and rendered rather than asserting on pixels.
#[tokio::test]
async fn measured_off_is_accepted() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let resp = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview?measured=off", &[AUTH])
        .await;
    assert_eq!(resp.status, StatusCode::OK);
    assert!(resp.is_png());
}

/// Each combination of view options is cached in its own slot. Folding them
/// into the fingerprint instead would leave one slot per device, and toggling
/// back and forth would re-render every single time.
#[tokio::test]
async fn each_view_variant_gets_its_own_cache_slot() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");

    for query in ["", "?dither=off", "?measured=off"] {
        let first = app
            .get_with_headers(
                &format!("/api/admin/devices/DEFAULT/preview{query}"),
                &[AUTH],
            )
            .await;
        assert_eq!(
            cache_state(&first),
            "miss",
            "variant {query:?} first request"
        );
    }
    // Every variant is now warm — including the first, which the later
    // variants must not have evicted or overwritten.
    for query in ["", "?dither=off", "?measured=off"] {
        let again = app
            .get_with_headers(
                &format!("/api/admin/devices/DEFAULT/preview{query}"),
                &[AUTH],
            )
            .await;
        assert_eq!(
            cache_state(&again),
            "hit",
            "variant {query:?} second request"
        );
    }
}

/// Anything other than an explicit no means yes, so a client that sends
/// `dither=on` gets the dithered render rather than an accidental raw one.
#[tokio::test]
async fn only_an_explicit_no_turns_an_option_off() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let plain = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
        .await;
    for query in ["?dither=on", "?dither=1", "?dither=true"] {
        let resp = app
            .get_with_headers(
                &format!("/api/admin/devices/DEFAULT/preview{query}"),
                &[AUTH],
            )
            .await;
        assert_eq!(resp.body, plain.body, "{query} should keep dithering on");
    }
}

/// The spellings a URL invites. All of them must mean the same thing, or the
/// toggle works from one client and silently does nothing from another.
#[tokio::test]
async fn off_zero_false_and_no_all_disable() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let raw = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview?dither=off", &[AUTH])
        .await;
    for query in ["?dither=0", "?dither=false", "?dither=no", "?dither=OFF"] {
        let resp = app
            .get_with_headers(
                &format!("/api/admin/devices/DEFAULT/preview{query}"),
                &[AUTH],
            )
            .await;
        assert_eq!(resp.body, raw.body, "{query} should disable dithering");
    }
}
