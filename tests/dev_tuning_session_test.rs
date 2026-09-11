//! A tuning session holds one device at a 60-second refresh so that a colour
//! calibration iterates in a minute rather than in the screen's own cadence.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn a_session_shortens_that_device_s_refresh_and_ending_it_restores_the_rate() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = common_app(dir.path());

    let mac = "AA:BB:CC:DD:EE:01";
    let api_key = app.register_device(mac).await;

    let baseline = refresh_rate_of(&app, mac, &api_key).await;
    assert_ne!(
        baseline, 60,
        "the fixture screen must not already answer 60s, or this test proves nothing"
    );

    let r = app
        .put_json(&format!("/dev/api/tuning-session/{mac}"), &[AUTH], "")
        .await;
    assert!(r.status.is_success(), "PUT session failed: {}", r.status);

    assert_eq!(
        refresh_rate_of(&app, mac, &api_key).await,
        60,
        "a live tuning session must shorten the refresh rate"
    );

    let r = app
        .delete(&format!("/dev/api/tuning-session/{mac}"), &[AUTH])
        .await;
    assert!(r.status.is_success());

    assert_eq!(
        refresh_rate_of(&app, mac, &api_key).await,
        baseline,
        "ending the session must hand the rate straight back"
    );
}

#[tokio::test]
async fn another_device_is_unaffected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = common_app(dir.path());

    let target = "AA:BB:CC:DD:EE:01";
    let other = "AA:BB:CC:DD:EE:02";
    let other_key = app.register_device(other).await;
    let baseline = refresh_rate_of(&app, other, &other_key).await;

    app.put_json(&format!("/dev/api/tuning-session/{target}"), &[AUTH], "")
        .await;

    assert_eq!(
        refresh_rate_of(&app, other, &other_key).await,
        baseline,
        "a session on one device must not touch another"
    );
}

/// Two devices, keyed by the MACs the tests register.
///
/// The plan's original fixture keyed devices by friendly names ("kitchen",
/// "hallway"), but `/api/display`'s device-config lookup
/// (`src/api/display.rs`, the `device_entry_key` resolution block) only ever
/// matches a config key against the polling MAC or a registration code —
/// never an arbitrary name — so a MAC-registered device would never resolve
/// to such a key at all (`device_entry_key` would be `None`) and the whole
/// test would be inert. Keying by MAC directly is how every other
/// MAC-in-path test in this suite (e.g. `admin_recovery_test.rs`,
/// `admin_write_test.rs`) does it, and it's what actually resolves here.
fn common_app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    let yaml = "\
admin:
  token: secret
devices:
  \"AA:BB:CC:DD:EE:01\":
    screen: byonk-builtin/default
  \"AA:BB:CC:DD:EE:02\":
    screen: byonk-builtin/default
";
    TestApp::new_with_config_yaml(yaml, dir)
}

async fn refresh_rate_of(app: &TestApp, mac: &str, api_key: &str) -> u32 {
    let headers = common::fixtures::display_headers(mac, api_key);
    let r = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
        .await;
    let body: serde_json::Value = r.json();
    body["refresh_rate"]
        .as_u64()
        .unwrap_or_else(|| panic!("no refresh_rate in {body}")) as u32
}
