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

/// The preview cache must keep an image exactly as long as the device is
/// told to sleep. A `final.refresh` that moved the device's cadence but left
/// the preview expiring on the screen's own interval would serve a stale
/// image, or re-render needlessly, for the old interval.
///
/// If this breaks, `DevicePreview::final_rung` no longer reaches
/// `RenderResult::refresh_rate`, which is what the handler stores as the TTL.
#[tokio::test]
async fn final_refresh_sets_the_preview_cache_ttl() {
    use byonk::models::CascadeValues;

    async fn ttl_secs(fin: Option<u32>) -> i64 {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let mut config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        config.admin.token = Some("secret".to_string());
        let device = config.devices.get_mut("DEFAULT").expect("DEFAULT");
        device.screen = "byonk-builtin/calibration/grey".to_string();
        device.final_rung = fin.map(|r| CascadeValues {
            refresh: Some(r),
            ..Default::default()
        });
        let state = byonk::server::create_app_state_with_config(asset_loader, config)
            .expect("create state");
        let cache = state.preview_cache.clone();
        let app = TestApp::from_state(state);
        let resp = app
            .get_with_headers("/api/admin/devices/DEFAULT/preview", &[AUTH])
            .await;
        assert_eq!(resp.status, StatusCode::OK);
        assert_eq!(cache_state(&resp), "miss");
        cache
            .ttl("DEFAULT#11")
            .expect("the render was cached")
            .num_seconds()
    }

    let own = ttl_secs(None).await;
    assert_ne!(
        own, 7200,
        "the veto value must differ from the screen's own rate"
    );
    assert_eq!(ttl_secs(Some(7200)).await, 7200);
}

// ---- X-Byonk-Resolution on the preview --------------------------------

/// A 16-entry palette whose entries differ per `seed`, as a comma string.
fn grey16(seed: u8) -> String {
    (0..16u8)
        .map(|i| {
            let v = i * 15 + seed;
            format!("#{v:02X}{v:02X}{v:02X}")
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// An app whose one device carries a 16-entry palette on EVERY rung of this
/// path that can hold one: device base and `final:` (both `colors` and
/// `colors_actual`), the panel (both), and the screen (both, returned by its
/// script). The override rung does not exist on the preview path.
fn adversarial_app(dir: &std::path::Path) -> TestApp {
    let repo = dir.join("local");
    let screen = repo.join("wide");
    std::fs::create_dir_all(&screen).unwrap();
    std::fs::write(
        repo.join("byonk-screens.yaml"),
        "name: local\ndescription: Header budget fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        screen.join("meta.yaml"),
        format!(
            "title: Wide\ndescription: Header budget fixture.\nbyonk: \"{}\"\nrefresh: 1800\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    let lua_list = |seed: u8| {
        grey16(seed)
            .split(',')
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    std::fs::write(
        screen.join("script.lua"),
        format!(
            "return {{ data = {{}}, colors = {{ {} }}, colors_actual = {{ {} }} }}\n",
            lua_list(1),
            lua_list(2)
        ),
    )
    .unwrap();
    std::fs::write(
        screen.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480"><rect width="800" height="480" fill="#808080"/></svg>"##,
    )
    .unwrap();

    let yaml = format!(
        "admin:\n  token: secret\nscreen_repos:\n  local:\n    path: {repo}\n\
panels:\n  trmnl_x:\n    name: TRMNL X\n    colors: '{pc}'\n    colors_actual: '{pa}'\n\
devices:\n  wide:\n    screen: local/wide\n    panel: trmnl_x\n    colors: '{bc}'\n    colors_actual: '{ba}'\n\
    final:\n      colors: '{fc}'\n      colors_actual: '{fa}'\n",
        repo = repo.display(),
        pc = grey16(3),
        pa = grey16(4),
        bc = grey16(5),
        ba = grey16(6),
        fc = grey16(7),
        fa = grey16(8),
    );
    let (app, _) = TestApp::new_with_config_yaml(&yaml, dir);
    app
}

/// The diagnostic is opt-in: this is the Home Assistant camera frame, and an
/// always-on header of unbounded size is what `c5fbde6a` already fixed once.
#[tokio::test]
async fn resolution_header_is_absent_unless_requested() {
    let app = app_with_default_screen("byonk-builtin/calibration/grey");
    let plain = app
        .get_with_headers("/api/admin/devices/DEFAULT/preview?force=1", &[AUTH])
        .await;
    assert_eq!(cache_state(&plain), "miss");
    assert!(plain.headers.get("x-byonk-resolution").is_none());

    let asked = app
        .get_with_headers(
            "/api/admin/devices/DEFAULT/preview?force=1&resolution=1",
            &[AUTH],
        )
        .await;
    assert!(asked.headers.get("x-byonk-resolution").is_some());

    // `?resolution=0` says no, like every other flag on this endpoint.
    let off = app
        .get_with_headers(
            "/api/admin/devices/DEFAULT/preview?force=1&resolution=0",
            &[AUTH],
        )
        .await;
    assert!(off.headers.get("x-byonk-resolution").is_none());
}

#[tokio::test]
async fn the_requested_header_stays_inside_its_budget_for_sixteen_entry_palettes() {
    let dir = tempfile::tempdir().unwrap();
    let app = adversarial_app(dir.path());
    let r = app
        .get_with_headers(
            "/api/admin/devices/wide/preview?force=1&resolution=1",
            &[AUTH],
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(cache_state(&r), "miss");
    let len = r.headers.get("x-byonk-resolution").unwrap().len();
    eprintln!("preview X-Byonk-Resolution: {len} bytes");
    // nginx's default `proxy_buffer_size` is one memory page (4 KiB on
    // x86_64) and must hold the status line and EVERY response header; past
    // it a reverse-proxied preview fails with 502. 3.5 KiB leaves room for
    // the other headers. (HA Ingress's aiohttp allows 8190 per field.) This
    // endpoint can carry more palette strings than `/dev/api/render` -- base,
    // `final:`, screen and panel rungs, each for `colors` and `colors_actual`
    // -- so the dev endpoint's headroom does not transfer.
    assert!(len < 3584, "X-Byonk-Resolution is {len} bytes");
}

/// Sighting poll as `/api/display` sees it: records the Board and Model the
/// admin preview later reads.
async fn poll(app: &TestApp, id: &str, model: &str, board: Option<&str>) {
    let key = app.register_device(id).await;
    let mut h: Vec<(&str, String)> = common::fixtures::display_headers(id, &key);
    h.retain(|(k, _)| *k != "Model");
    h.push(("Model", model.to_string()));
    if let Some(b) = board {
        h.push(("Board", b.to_string()));
    }
    let r = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&h))
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
}

/// PNG width and height from the IHDR chunk.
fn png_size(resp: &common::app::TestResponse) -> (u32, u32) {
    let b = &resp.body;
    (
        u32::from_be_bytes(b[16..20].try_into().unwrap()),
        u32::from_be_bytes(b[20..24].try_into().unwrap()),
    )
}

#[tokio::test]
async fn a_new_sighting_re_renders_the_preview_with_the_sighted_panel() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "\
admin:
  token: secret
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    width: 640
    height: 400
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  pinned:
    name: Pinned
    width: 320
    height: 200
    colors: '#000000,#FFFFFF'
devices:
  office:
    screen: byonk-builtin/default
  fixed:
    screen: byonk-builtin/default
    panel: pinned
";
    let (app, _) = TestApp::new_with_config_yaml(yaml, dir.path());
    let from = |r: &common::app::TestResponse| -> serde_json::Value {
        let h: serde_json::Value =
            serde_json::from_str(r.headers["x-byonk-resolution"].to_str().unwrap()).unwrap();
        h["colors"]["from"].clone()
    };
    let url = "/api/admin/devices/office/preview?resolution=1";
    // Seen, but with a Board no panel matches; the Model stays "og" throughout,
    // so only the sighting's Board can change the fingerprint.
    poll(&app, "office", "og", Some("no-such-board")).await;
    let first = app.get_with_headers(url, &[AUTH]).await;
    assert_eq!(cache_state(&first), "miss");
    assert_eq!(from(&first), "fallback");
    assert_eq!(png_size(&first), (800, 480));
    assert_eq!(
        cache_state(&app.get_with_headers(url, &[AUTH]).await),
        "hit"
    );

    poll(&app, "office", "og", Some("e1004")).await;
    let third = app.get_with_headers(url, &[AUTH]).await;
    assert_eq!(
        cache_state(&third),
        "miss",
        "a new sighting must not re-serve the old panel's render"
    );
    assert_eq!(from(&third), "panel");
    assert_eq!(png_size(&third), (640, 400));

    // An explicit `panel:` outranks whatever the device reported.
    poll(&app, "fixed", "og", Some("e1004")).await;
    let fixed = app
        .get_with_headers("/api/admin/devices/fixed/preview?resolution=1", &[AUTH])
        .await;
    assert_eq!(png_size(&fixed), (320, 200));
}
