//! `RenderOpts::params` and `RenderOpts::device` — the two inputs a *device*
//! preview needs that an authoring render does not have.
//!
//! `ScreenStore::render` was written for the authoring case, where there is no
//! device: it ran every script with empty params and a hard-coded
//! `dev-simulator` identity. A device preview has to render what the device
//! itself would see, so both have to be supplied by the caller. These tests
//! read them back out of the script's own `data` table, which is the only
//! place the plumbing is observable without comparing pixels.

use std::collections::HashMap;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::screen_store::{DevicePreview, RenderOpts, ScreenStore};

/// A screen repo with one screen whose script echoes its params and the
/// device identity straight back into `data`.
fn write_repo(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("echo")).unwrap();
    std::fs::write(
        dir.join("byonk-screens.yaml"),
        "name: testrepo\ndescription: Render-opts fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("echo/meta.yaml"),
        "title: Echo\ndescription: Echoes params and device back.\nbyonk: \"0.18\"\nrefresh: 60\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("echo/script.lua"),
        r#"
return {
  data = {
    greeting = params.greeting,
    mac = device.mac,
    firmware = device.firmware_version,
    battery = device.battery_voltage,
    rssi = device.rssi,
  },
  refresh_rate = 60,
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("echo/screen.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480"><rect width="800" height="480" fill="white"/></svg>"#,
    )
    .unwrap();
}

fn store(dir: &std::path::Path) -> Arc<ScreenStore> {
    let config_path = dir.join("config.yaml");
    let repo_dir = dir.join("testrepo");
    write_repo(&repo_dir);
    std::fs::write(
        &config_path,
        format!(
            "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n\
             screen_repos:\n  testrepo:\n    path: {}\n",
            repo_dir.display()
        ),
    )
    .unwrap();
    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    create_app_state_with_config(asset_loader, config)
        .expect("create app state")
        .screen_store
        .clone()
}

fn params(pairs: &[(&str, &str)]) -> HashMap<String, serde_yaml::Value> {
    pairs
        .iter()
        .map(|(k, v)| {
            (
                (*k).to_string(),
                serde_yaml::Value::String((*v).to_string()),
            )
        })
        .collect()
}

#[test]
fn params_reach_the_script() {
    let dir = tempfile::tempdir().unwrap();
    let out = store(dir.path()).render(
        "testrepo/echo",
        RenderOpts {
            params: params(&[("greeting", "moin")]),
            ..RenderOpts::default()
        },
    );
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert_eq!(out.data["greeting"], "moin");
}

/// The authoring case must keep working unchanged: no params supplied means
/// the script sees none, not a stale or defaulted value.
#[test]
fn default_render_opts_still_pass_no_params() {
    let dir = tempfile::tempdir().unwrap();
    let out = store(dir.path()).render("testrepo/echo", RenderOpts::default());
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert!(
        out.data["greeting"].is_null(),
        "expected no greeting param, got {:?}",
        out.data["greeting"]
    );
}

#[test]
fn device_identity_reaches_the_script() {
    let dir = tempfile::tempdir().unwrap();
    let out = store(dir.path()).render(
        "testrepo/echo",
        RenderOpts {
            device: Some(DevicePreview {
                mac: "AA:BB:CC:DD:EE:FF".to_string(),
                firmware_version: Some("1.2.3".to_string()),
                battery_voltage: Some(3.9),
                rssi: Some(-64),
                ..DevicePreview::default()
            }),
            ..RenderOpts::default()
        },
    );
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert_eq!(out.data["mac"], "AA:BB:CC:DD:EE:FF");
    assert_eq!(out.data["firmware"], "1.2.3");
    assert_eq!(out.data["rssi"], -64);
}

/// Without a device, the authoring identity stands — a screen that reads
/// `device.mac` still gets something rather than erroring.
#[test]
fn no_device_keeps_the_authoring_identity() {
    let dir = tempfile::tempdir().unwrap();
    let out = store(dir.path()).render("testrepo/echo", RenderOpts::default());
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert_eq!(out.data["mac"], "dev-simulator");
}

/// A device with no telemetry yet (never polled) must not inherit the
/// authoring placeholder battery/rssi — a preview showing 4.2 V for a device
/// that has never reported is a lie, not a default.
#[test]
fn device_without_telemetry_reports_none() {
    let dir = tempfile::tempdir().unwrap();
    let out = store(dir.path()).render(
        "testrepo/echo",
        RenderOpts {
            device: Some(DevicePreview {
                mac: "AA:BB:CC:DD:EE:FF".to_string(),
                ..DevicePreview::default()
            }),
            ..RenderOpts::default()
        },
    );
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert!(out.data["battery"].is_null(), "battery must not be faked");
    assert!(out.data["rssi"].is_null(), "rssi must not be faked");
}

// ---------------------------------------------------------------------------
// The device-config layer: `colors`, `dither` and the tuning fields do NOT
// belong in `RenderOpts`' override slots. They sit below the script's own
// choices and above the panel's, and getting that order wrong makes the
// preview disagree with the panel for any screen that sets its own dither.
// ---------------------------------------------------------------------------

/// A second repo whose screens report the pre-script dither the runtime
/// resolved (`device.dither.algorithm`), and paint a gradient so two
/// different algorithms produce visibly different bytes.
fn write_dither_repo(dir: &std::path::Path) {
    std::fs::write(
        dir.join("byonk-screens.yaml"),
        "name: testrepo\ndescription: Dither fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480"><defs><linearGradient id="g"><stop offset="0%" stop-color="#000000"/><stop offset="100%" stop-color="#FFFFFF"/></linearGradient></defs><rect width="800" height="480" fill="url(#g)"/></svg>"##;

    // Reports the resolved algorithm; states no dither of its own.
    std::fs::create_dir_all(dir.join("plain")).unwrap();
    std::fs::write(
        dir.join("plain/meta.yaml"),
        "title: Plain\ndescription: No dither of its own.\nbyonk: \"0.18\"\nrefresh: 60\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("plain/script.lua"),
        "return { data = { algo = device.dither.algorithm, colors = device.colors }, \
         refresh_rate = 60 }\n",
    )
    .unwrap();
    std::fs::write(dir.join("plain/screen.svg"), svg).unwrap();

    // Picks its own dither, which must beat the device config.
    std::fs::create_dir_all(dir.join("opinionated")).unwrap();
    std::fs::write(
        dir.join("opinionated/meta.yaml"),
        "title: Opinionated\ndescription: Picks its own dither.\nbyonk: \"0.18\"\nrefresh: 60\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("opinionated/script.lua"),
        "return { data = {}, dither = \"floyd-steinberg\", refresh_rate = 60 }\n",
    )
    .unwrap();
    std::fs::write(dir.join("opinionated/screen.svg"), svg).unwrap();
}

fn dither_store(dir: &std::path::Path) -> Arc<ScreenStore> {
    let config_path = dir.join("config.yaml");
    let repo_dir = dir.join("testrepo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    write_dither_repo(&repo_dir);
    std::fs::write(
        &config_path,
        format!(
            "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n\
             screen_repos:\n  testrepo:\n    path: {}\n",
            repo_dir.display()
        ),
    )
    .unwrap();
    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    create_app_state_with_config(asset_loader, config)
        .expect("create app state")
        .screen_store
        .clone()
}

fn with_device_dither(algo: Option<&str>) -> RenderOpts {
    RenderOpts {
        device: Some(DevicePreview {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            dither: algo.map(str::to_string),
            ..DevicePreview::default()
        }),
        ..RenderOpts::default()
    }
}

#[test]
fn device_config_dither_reaches_the_script_context() {
    let dir = tempfile::tempdir().unwrap();
    let out = dither_store(dir.path()).render("testrepo/plain", with_device_dither(Some("sierra")));
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert_eq!(out.data["algo"], "sierra");
}

#[test]
fn render_opts_dither_overrides_the_device_config() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = with_device_dither(Some("sierra"));
    opts.dither = Some("burkes".to_string());
    let out = dither_store(dir.path()).render("testrepo/plain", opts);
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert_eq!(out.data["algo"], "burkes");
}

/// The precedence that matters: `script > device_config`. A screen that picks
/// its own dither must render identically whether or not the device config
/// names a different one — otherwise the preview would dither differently
/// from the panel.
#[test]
fn script_dither_beats_the_device_config() {
    let dir = tempfile::tempdir().unwrap();
    let store = dither_store(dir.path());

    let without = store.render("testrepo/opinionated", with_device_dither(None));
    let with = store.render("testrepo/opinionated", with_device_dither(Some("sierra")));
    assert!(without.error.is_none(), "{:?}", without.error);
    assert!(with.error.is_none(), "{:?}", with.error);
    assert!(!without.png.is_empty());
    assert_eq!(
        without.png, with.png,
        "a device-config dither must not displace the script's own choice"
    );
}

/// The contrast case, so the test above cannot pass by the device-config
/// dither being ignored everywhere: on a screen with no opinion, it decides.
#[test]
fn device_config_dither_decides_when_the_script_has_no_opinion() {
    let dir = tempfile::tempdir().unwrap();
    let store = dither_store(dir.path());

    let atkinson = store.render("testrepo/plain", with_device_dither(Some("atkinson")));
    let floyd = store.render(
        "testrepo/plain",
        with_device_dither(Some("floyd-steinberg")),
    );
    assert!(atkinson.error.is_none(), "{:?}", atkinson.error);
    assert!(floyd.error.is_none(), "{:?}", floyd.error);
    assert!(!atkinson.png.is_empty());
    assert_ne!(
        atkinson.png, floyd.png,
        "two different device-config dithers must produce different pixels"
    );
}

/// `DeviceConfig::colors` occupies the device-config palette slot, below the
/// script and above the panel.
#[test]
fn device_config_colors_reach_the_script_context() {
    let dir = tempfile::tempdir().unwrap();
    let out = dither_store(dir.path()).render(
        "testrepo/plain",
        RenderOpts {
            device: Some(DevicePreview {
                mac: "AA:BB:CC:DD:EE:FF".to_string(),
                colors: Some("#000000,#FFFFFF".to_string()),
                ..DevicePreview::default()
            }),
            ..RenderOpts::default()
        },
    );
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    assert_eq!(out.data["colors"][0], "#000000");
    assert_eq!(out.data["colors"][1], "#FFFFFF");
    assert_eq!(out.data["colors"].as_array().map(Vec::len), Some(2));
}
