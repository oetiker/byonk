//! `POST /dev/api/render` is a preview. It must not change what a real device
//! renders.
//!
//! The dev render used to sync its query parameters into `DevOverrides`
//! (`src/api/dev.rs`, "Sync overrides to shared state so the production
//! /api/display handler picks them up"), keyed by `query.mac`. So merely
//! *looking* at a preview with a device selected re-tuned that physical
//! display — no button, no indication — and a preview that omitted `dither`
//! *cleared* an override someone had deliberately set.
//!
//! Pushing to real hardware is a separate, explicit act (`PUT
//! /dev/api/panel-colors/{panel}`). Previewing is not it.
//!
//! These tests are behavioural on purpose: `TestApp` exposes no `AppState`,
//! and the thing worth pinning is not that a map stayed empty but that the
//! bytes a device is served did not move.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// A screen with a `data-byonk-tone="continuous"` region, so that a change of
/// dither algorithm is actually observable in the rendered PNG.
///
/// Unmarked content is matched against the official palette regardless of
/// tuning (`eink-dither`'s `RegionMap::model`), so a flat-colour screen would
/// render identically under every algorithm and these tests would pass with
/// the bug present. See the same note in `dev_panel_colors_test.rs`.
fn scaffold_gradient_screen(dir: &std::path::Path) {
    let repo_dir = dir.join("local");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: local\ndescription: Test fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();

    let screen_dir = repo_dir.join("gradient");
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!(
            "title: Gradient\ndescription: Continuous-tone gradient, for tests.\nbyonk: \"{}\"\nrefresh: 3600\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    // No `dither` in the return: the device config's algorithm must be what
    // decides, or these tests would be measuring the script instead.
    std::fs::write(
        screen_dir.join("script.lua"),
        "return { data = { width = layout.width, height = layout.height }, refresh_rate = 3600 }\n",
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {{ data.width }} {{ data.height }}" width="{{ data.width }}" height="{{ data.height }}">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#202020"/>
      <stop offset="50%" stop-color="#9070A0"/>
      <stop offset="100%" stop-color="#E0E0E0"/>
    </linearGradient>
  </defs>
  <g data-byonk-tone="continuous">
    <rect x="0" y="0" width="{{ data.width }}" height="{{ data.height }}" fill="url(#g)"/>
  </g>
</svg>
"##,
    )
    .unwrap();
}

fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    scaffold_gradient_screen(dir);
    let yaml = format!(
        "\
admin:
  token: secret
screen_repos:
  local:
    path: {}
panels:
  e1004:
    name: reTerminal E1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
    colors_actual: '#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D'
devices:
  e1004:
    screen: local/gradient
    panel: e1004
  kitchen:
    screen: local/gradient
    panel: e1004
    dither: atkinson
    strength: 1.0
",
        dir.join("local").display()
    );
    TestApp::new_with_config_yaml(&yaml, dir)
}

fn display_headers(device_id: &str, api_key: &str) -> Vec<(String, String)> {
    vec![
        ("ID".to_string(), device_id.to_string()),
        ("Access-Token".to_string(), api_key.to_string()),
        ("Width".to_string(), "800".to_string()),
        ("Height".to_string(), "480".to_string()),
        ("Battery-Voltage".to_string(), "4.12".to_string()),
        ("RSSI".to_string(), "-67".to_string()),
        ("FW-Version".to_string(), "1.7.1".to_string()),
        ("Model".to_string(), "og".to_string()),
        ("Host".to_string(), "localhost:3000".to_string()),
    ]
}

/// Fetch `/api/display`, follow its `image_url`, return the PNG bytes.
async fn fetch_device_png(app: &TestApp, headers: &[(String, String)]) -> Vec<u8> {
    let pairs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let display_response = app.get_with_headers("/api/display", &pairs).await;
    let image_url = common::assert_valid_display_response(&display_response);
    let path = image_url
        .split("localhost:3000")
        .nth(1)
        .expect("display response should carry a path after the host");
    let image_response = app.get(path).await;
    common::assert_png(&image_response);
    image_response.body
}

/// The headline guarantee: previewing with a device selected leaves that
/// device's own render untouched.
///
/// Restoring the `if let Some(ref device_key) = query.mac { … }` sync block in
/// `handle_render` makes this fail — the preview's `dither=floyd-steinberg`
/// lands in `DevOverrides::dither` under `kitchen`, and `/api/display` serves
/// a differently-dithered PNG from then on.
#[tokio::test]
async fn a_dev_preview_does_not_retune_the_device_it_previews() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let key = app.register_device("kitchen").await;
    let headers = display_headers("kitchen", &key);

    let before = fetch_device_png(&app, &headers).await;

    // A preview, with a device selected and an algorithm the device is not
    // configured for.
    let preview = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"device":"kitchen","width":800,"height":480,"override":{"dither":"floyd-steinberg"}}"#,
        )
        .await;
    assert!(
        preview.status.is_success(),
        "preview failed: {}",
        preview.status
    );

    let after = fetch_device_png(&app, &headers).await;

    assert_eq!(
        before, after,
        "a dev preview changed what the real device renders"
    );
}

/// The same guarantee for the tuning numbers, which travelled in their own
/// map (`DevOverrides::tuning`) and were applied as one all-or-nothing block.
#[tokio::test]
async fn a_dev_preview_does_not_retune_the_device_with_tuning_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let key = app.register_device("kitchen").await;
    let headers = display_headers("kitchen", &key);

    let before = fetch_device_png(&app, &headers).await;

    let preview = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"device":"kitchen","width":800,"height":480,"override":{"strength":0.2,"max_error":0.1}}"#,
        )
        .await;
    assert!(
        preview.status.is_success(),
        "preview failed: {}",
        preview.status
    );

    let after = fetch_device_png(&app, &headers).await;

    assert_eq!(
        before, after,
        "a dev preview's tuning numbers changed what the real device renders"
    );
}

/// The key-space landmine: the sync block wrote `query.colors_actual` into
/// `DevOverrides::panel_colors` under the **device** key, while
/// `src/api/display.rs` reads that map by **panel** key. Left behind when the
/// 2026-09-10 colour-tuning amendment re-keyed colour overrides from device to
/// panel.
///
/// Normally inert, because a device key is a MAC and a panel key is a profile
/// name. `attic` below makes them collide — a device whose config key is
/// literally `e1004`, the panel's id — and then a preview of *that one device*
/// silently repaints every other device on the panel.
///
/// The collision is what makes the defect observable; it is not a claim that
/// anyone would name a device this way. Restoring the sync block makes this
/// fail.
#[tokio::test]
async fn a_dev_preview_cannot_leak_colours_into_the_panel_key_space() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let kitchen_key = app.register_device("kitchen").await;
    let kitchen_headers = display_headers("kitchen", &kitchen_key);
    // The colliding device must exist for the preview to resolve it at all.
    let _ = app.register_device("e1004").await;

    let before = fetch_device_png(&app, &kitchen_headers).await;

    // Preview the colliding device with measured colours well away from the
    // panel's configured ones.
    let preview = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r##"{"device":"e1004","width":800,"height":480,"override":{"colors_actual":"#1A1A1A,#F0F0F0,#C04040,#E0D050,#4050B0,#20A860"}}"##,
        )
        .await;
    assert!(
        preview.status.is_success(),
        "preview failed: {}",
        preview.status
    );

    let after = fetch_device_png(&app, &kitchen_headers).await;

    assert_eq!(
        before, after,
        "previewing device `e1004` leaked its colours into panel `e1004`, \
         changing an unrelated device's render"
    );
}
