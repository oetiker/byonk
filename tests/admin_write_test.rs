mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

// Device `screen` values are always qualified `handle/path` package refs.
//
// Most of these tests just need "some known, resolvable non-default builtin
// screen" — `calibration/color` and `calibration/grey`, the only other
// screens left in the minimal `byonk-builtin` repo, stand in wherever the
// test doesn't care about params. The two params-patch tests below use
// `TRANSIT` (`examples/swiss-departure-board`, registered as a real screen
// repo handle since Task 11) instead, so they validate against a real,
// non-empty params schema rather than an unknown-and-ignored param.
const COLOR: &str = "byonk-builtin/calibration/color";
const GREY: &str = "byonk-builtin/calibration/grey";
const UNKNOWN: &str = "byonk-builtin/does-not-exist";
// A real, params-bearing screen (declares `station`/`limit`), now that Task 11
// registers `examples` as a real screen repo handle. Used by the two
// params-patch tests below so they exercise `validate_params` against a
// non-empty schema instead of an unknown-and-therefore-ignored param on a
// screen with no schema at all.
const TRANSIT: &str = "examples/swiss-departure-board";

#[tokio::test]
async fn test_write_on_embedded_config_returns_409() {
    let app = TestApp::new_admin("secret"); // embedded-only
    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{COLOR}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_add_device_persists_and_hot_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{COLOR}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK);

    // File updated + comment preserved.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("CC:DD:EE:FF:00:11"));
    assert!(
        on_disk.contains(COLOR),
        "qualified screen ref written to disk"
    );

    // Hot-reload: GET /devices shows it without restart.
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == "CC:DD:EE:FF:00:11")
        .expect("device present after hot reload");
    assert_eq!(
        row["screen"], COLOR,
        "screen reads back as the qualified ref"
    );
}

#[tokio::test]
async fn test_add_unknown_screen_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{UNKNOWN}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_bare_name_screen_is_rejected() {
    // A bare (unqualified) name is no longer resolvable — it must be a `handle/path` ref.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"hello"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_builtin_ref_screen_is_accepted() {
    // A qualified builtin ref that ships in the embedded package must be assignable.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"key":"CC:DD:EE:FF:00:11","screen":"{COLOR}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK);
}

#[tokio::test]
async fn test_patch_default_device_screen_persists() {
    // Setting the DEFAULT device's screen is now done via the per-device
    // endpoint (the reserved DEFAULT device ships in the embedded config).
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"screen":"{COLOR}"}}"#);
    let resp = app
        .patch_json("/api/admin/devices/DEFAULT", &[AUTH], &body)
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains(COLOR),
        "expected '{COLOR}' on disk:\n{on_disk}"
    );
}

#[tokio::test]
async fn test_patch_default_device_bad_screen_returns_400() {
    // The reserved DEFAULT device goes through the same per-device screen
    // validation as any other device — an unknown ref must be rejected.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = format!(r#"{{"screen":"{UNKNOWN}"}}"#);
    let resp = app
        .patch_json("/api/admin/devices/DEFAULT", &[AUTH], &body)
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_delete_default_device_returns_409() {
    // The reserved DEFAULT device can never be deleted: HA's manual
    // config-entry removal must be harmless, and this 409 is the
    // defense-in-depth backstop.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app.delete("/api/admin/devices/DEFAULT", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_patch_settings_toggles_registration() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"registration_enabled":false}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("enabled: false"));
}

#[tokio::test]
async fn test_patch_settings_bogus_auth_mode_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let resp = app
        .patch_json("/api/admin/settings", &[AUTH], r#"{"auth_mode":"bogus"}"#)
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_patch_then_delete_device() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"CC:DD","screen":"{COLOR}"}}"#),
    )
    .await;

    let patch = app
        .patch_json(
            "/api/admin/devices/CC:DD",
            &[AUTH],
            &format!(r#"{{"screen":"{GREY}"}}"#),
        )
        .await;
    assert_eq!(patch.status, StatusCode::OK);

    let del = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::OK);
    let del_again = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del_again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_patch_panel_and_dither_read_back() {
    // Repro for the device-page bug: setting Panel/Dither via PATCH writes to
    // disk but GET /api/admin/devices reported them as null. The device is
    // POSTed into the shipped empty `devices: {}` map (the live scenario).
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .post_json(
            "/api/admin/devices",
            &[AUTH],
            &format!(r#"{{"key":"9C:13:9E:AB:99:D4","screen":"{COLOR}"}}"#),
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let resp = app
        .patch_json(
            "/api/admin/devices/9C:13:9E:AB:99:D4",
            &[AUTH],
            r#"{"panel":"trmnl_x"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let resp = app
        .patch_json(
            "/api/admin/devices/9C:13:9E:AB:99:D4",
            &[AUTH],
            r#"{"dither":"atkinson"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    // Disk has both keys (this part already worked in the field).
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("panel: trmnl_x"),
        "expected 'panel: trmnl_x' on disk:\n{on_disk}"
    );
    assert!(
        on_disk.contains("dither: atkinson"),
        "expected 'dither: atkinson' on disk:\n{on_disk}"
    );

    // The bug: GET /api/admin/devices must read them back, not null.
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "9C:13:9E:AB:99:D4")
        .expect("device row present");
    assert_eq!(row["screen"], COLOR, "screen should read back");
    assert_eq!(row["panel"], "trmnl_x", "panel should read back");
    assert_eq!(row["dither"], "atkinson", "dither should read back");
}

#[tokio::test]
async fn test_patch_panel_and_dither_read_back_for_seen_device() {
    // Same as above, but the device has been SEEN by the registry first (it
    // polled /api/setup). This exercises the first loop of list_devices, which
    // is the path the live HA-onboarded device takes.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    let mac = "9C:13:9E:AB:99:D4";
    app.register_device(mac).await;

    let resp = app
        .post_json(
            "/api/admin/devices",
            &[AUTH],
            &format!(r#"{{"key":"9C:13:9E:AB:99:D4","screen":"{COLOR}"}}"#),
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    app.patch_json(
        "/api/admin/devices/9C:13:9E:AB:99:D4",
        &[AUTH],
        r#"{"panel":"trmnl_x"}"#,
    )
    .await;
    app.patch_json(
        "/api/admin/devices/9C:13:9E:AB:99:D4",
        &[AUTH],
        r#"{"dither":"atkinson"}"#,
    )
    .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == mac)
        .expect("device row present");
    assert_eq!(row["screen"], COLOR, "screen should read back");
    assert_eq!(row["panel"], "trmnl_x", "panel should read back");
    assert_eq!(row["dither"], "atkinson", "dither should read back");
}

#[tokio::test]
async fn test_patch_params_submap_reads_back_with_populated_devices() {
    // Repro for the nested-params write bug: with a populated devices map, the
    // config writer mis-indented the params sub-map so it parsed back empty.
    // Uses `TRANSIT` (declares a `station` param) rather than `COLOR` so the
    // params PATCH below is validated against a real, non-empty schema.
    let dir = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", dir.path());

    // Two devices -> populated devices map (exercises the non-empty write path).
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{TRANSIT}"}}"#),
    )
    .await;
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"CC:DD","screen":"{TRANSIT}"}}"#),
    )
    .await;

    let resp = app
        .patch_json(
            "/api/admin/devices/AA:BB",
            &[AUTH],
            r#"{"params":{"station":"Olten"}}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .expect("device row present");
    assert_eq!(row["params"]["station"], "Olten", "params should read back");
}

#[tokio::test]
async fn test_patch_params_merge_preserves_other_keys() {
    // Editing one param (no screen change) must merge, not replace: a second
    // single-key PATCH must not drop the first key. Uses `TRANSIT` (declares
    // `station`/`limit` params) so both PATCHes are validated against a real,
    // non-empty schema.
    let dir = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{TRANSIT}"}}"#),
    )
    .await;

    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        r#"{"params":{"station":"Olten"}}"#,
    )
    .await;
    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        r#"{"params":{"limit":5}}"#,
    )
    .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(
        row["params"]["station"], "Olten",
        "first key preserved after 2nd patch"
    );
    assert_eq!(row["params"]["limit"], 5, "second key present");
}

#[tokio::test]
async fn test_patch_with_screen_change_replaces_params() {
    // A screen change replaces params wholesale with whatever the caller
    // provides — not with the new screen's defaults; there is no meta.yaml
    // defaults lookup in this path.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{COLOR}"}}"#),
    )
    .await;
    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        r#"{"params":{"old":"x"}}"#,
    )
    .await;

    app.patch_json(
        "/api/admin/devices/AA:BB",
        &[AUTH],
        &format!(r#"{{"screen":"{GREY}","params":{{"new":"y"}}}}"#),
    )
    .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(
        row["params"].get("old"),
        None,
        "old key dropped on screen change"
    );
    assert_eq!(row["params"]["new"], "y", "new key present");
}

#[tokio::test]
async fn test_patch_settings_screen_repo_refresh_interval_persists() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .patch_json(
            "/api/admin/settings",
            &[AUTH],
            r#"{"screen_repo_refresh_interval":3600}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("screen_repo_refresh_interval: 3600"),
        "expected 'screen_repo_refresh_interval: 3600' in:\n{on_disk}"
    );
}

#[tokio::test]
async fn test_patch_name_reads_back_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{COLOR}"}}"#),
    )
    .await;
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"name":"Kitchen"}"#)
        .await;

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["name"], "Kitchen");

    // Empty string clears it.
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"name":""}"#)
        .await;
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["name"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_patch_refresh_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());

    app.post_json(
        "/api/admin/devices",
        &[AUTH],
        &format!(r#"{{"key":"AA:BB","screen":"{COLOR}"}}"#),
    )
    .await;
    let resp = app
        .patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"refresh":600}"#)
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["refresh"], 600);

    // 0 clears the override.
    app.patch_json("/api/admin/devices/AA:BB", &[AUTH], r#"{"refresh":0}"#)
        .await;
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "AA:BB")
        .unwrap();
    assert_eq!(row["refresh"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_add_device_on_embedded_config_returns_409_even_with_no_key() {
    // Regression for the apply_device_add extraction: the embedded/read-only
    // config check must still run before the body is validated, so a request
    // missing `key` against an embedded config 409s (not 400) — same as it
    // did before the shared core existed.
    let app = TestApp::new_admin("secret"); // embedded-only, no key in body
    let body = format!(r#"{{"screen":"{COLOR}"}}"#);
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
}

// ---------------------------------------------------------------------------
// Device settings: validation, preservation, and clearing.
// ---------------------------------------------------------------------------

const MAC: &str = "CC:DD:EE:FF:00:22";

/// Create a device carrying every managed setting at once.
async fn seed_fully_configured(app: &TestApp) {
    let body = format!(
        r##"{{"key":"{MAC}","screen":"{COLOR}","panel":"reterminal_e1004",
            "dither":"floyd-steinberg","colors":"#000000,#FFFFFF",
            "refresh":900,"name":"Bench panel","max_error":0.9,
            "noise_scale":2.5,"chroma_clamp":0.3,"strength":0.8,
            "temperature_profile":"a","maximum_compatibility":true,
            "min_png_bytes":102401}}"##
    );
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK, "seed device: {}", resp.text());
}

/// The trap this branch introduced and must not fall into: adding a key to
/// `MANAGED_DEVICE_KEYS` takes it out of `unmanaged_device_keys`' protection,
/// so a key listed there but forgotten in the patch merge is *deleted* by any
/// patch that omits it. Silently, and only visible on the glass hours later.
///
/// Patch one unrelated field and demand every other setting is still on disk.
#[tokio::test]
async fn test_patch_preserves_every_tuning_knob() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"name":"Renamed"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());

    let on_disk = std::fs::read_to_string(&path).unwrap();
    let block = on_disk
        .split(MAC)
        .nth(1)
        .expect("device block present after patch");
    for expected in [
        "panel: reterminal_e1004",
        "dither: floyd-steinberg",
        "refresh: 900",
        "max_error: 0.9",
        "noise_scale: 2.5",
        "chroma_clamp: 0.3",
        "strength: 0.8",
        "temperature_profile: a",
        "maximum_compatibility: true",
        "min_png_bytes: 102401",
    ] {
        assert!(
            block.contains(expected),
            "`{expected}` was dropped by a patch that never mentioned it\n{block}"
        );
    }
    assert!(block.contains("Renamed"), "the patched field did change");
}

#[tokio::test]
async fn test_patch_rejects_an_unknown_dither_algorithm() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    // `steinburg`, not `steinberg` — a misspelling, deliberately not one of
    // the accepted aliases (`test_patch_accepts_a_dither_alias` next door
    // covers those, and an underscored `floyd_steinberg` must still pass).
    // Before validation this wrote fine and then rendered Atkinson, because
    // `svg_to_png`'s match falls through to it — a wrong kernel, no error.
    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"dither":"floyd-steinburg"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    assert!(
        resp.text().contains("floyd-steinberg"),
        "the error should list the real names: {}",
        resp.text()
    );
}

/// `floyd_steinberg` is an accepted alias, not a typo — validation normalises
/// before it judges, so it must go through.
#[tokio::test]
async fn test_patch_accepts_a_dither_alias() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"dither":"sierra_light"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("sierra_light"), "alias stored as written");
}

#[tokio::test]
async fn test_patch_rejects_an_unknown_panel() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"panel":"reterminal_e1005"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    assert!(resp.text().contains("reterminal_e1004"), "{}", resp.text());
}

#[tokio::test]
async fn test_patch_rejects_malformed_colors() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    for bad in [
        r##"{"colors":"#00000,#FFFFFF"}"##,  // five digits
        r##"{"colors":"#GGGGGG,#FFFFFF"}"##, // not hex
        r##"{"colors":"#FFFFFF"}"##,         // nothing to dither between
    ] {
        let resp = app
            .patch_json(&format!("/api/admin/devices/{MAC}"), &[AUTH], bad)
            .await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST, "accepted {bad}");
    }
}

/// `c` parses in byonk but is still commented out in firmware 1.8.14, which
/// reads it back as `default` — storing it would record a setting that does
/// nothing.
#[tokio::test]
async fn test_patch_rejects_temperature_profile_c() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"temperature_profile":"c"}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_clear_removes_a_setting_and_leaves_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"clear":["noise_scale","min_png_bytes"]}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());

    let on_disk = std::fs::read_to_string(&path).unwrap();
    let block = on_disk.split(MAC).nth(1).expect("device block");
    assert!(!block.contains("noise_scale"), "cleared:\n{block}");
    assert!(!block.contains("min_png_bytes"), "cleared:\n{block}");
    assert!(block.contains("max_error: 0.9"), "untouched:\n{block}");
    assert!(block.contains("strength: 0.8"), "untouched:\n{block}");
}

#[tokio::test]
async fn test_clear_rejects_screen_and_unknown_names() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    for bad in [r#"{"clear":["screen"]}"#, r#"{"clear":["nonsense"]}"#] {
        let resp = app
            .patch_json(&format!("/api/admin/devices/{MAC}"), &[AUTH], bad)
            .await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST, "accepted {bad}");
    }
}

/// Ruling C3: a field both cleared and set in one request ends up set -- the
/// clear applies to the existing entry first, the sets merge over the result.
#[tokio::test]
async fn test_clear_then_set_of_the_same_field_ends_up_set() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_fully_configured(&app).await;

    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{MAC}"),
            &[AUTH],
            r#"{"noise_scale":1.5,"clear":["noise_scale"]}"#,
        )
        .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());

    let on_disk = std::fs::read_to_string(&path).unwrap();
    let block = on_disk.split(MAC).nth(1).expect("device block");
    assert!(block.contains("noise_scale: 1.5"), "set wins:\n{block}");
}

#[tokio::test]
async fn test_clear_is_rejected_when_creating_a_device() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());

    // Empty too: a create has nothing to clear, so mentioning `clear` at all
    // means the caller thinks it is doing something it is not.
    for clear in ["[\"panel\"]", "[]"] {
        let body = format!(r#"{{"key":"{MAC}","screen":"{COLOR}","clear":{clear}}}"#);
        let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST, "accepted {clear}");
    }
}

// ---------------------------------------------------------------------------
// The `final:` rung, `colors_actual` and `gamut` through the real handler.
// ---------------------------------------------------------------------------

/// The `device_block` of `MAC`, parsed from what is actually on disk.
fn device_on_disk(path: &std::path::Path) -> serde_yaml::Value {
    let yaml = std::fs::read_to_string(path).unwrap();
    let root: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    root.get("devices")
        .and_then(|d| d.get(MAC))
        .cloned()
        .expect("device block present on disk")
}

async fn seed_with_final(app: &TestApp) {
    let body = format!(
        r##"{{"key":"{MAC}","screen":"{COLOR}","dither":"sierra",
            "final":{{"strength":0.4}}}}"##
    );
    let resp = app.post_json("/api/admin/devices", &[AUTH], &body).await;
    assert_eq!(resp.status, StatusCode::OK, "seed device: {}", resp.text());
}

async fn patch(app: &TestApp, body: &str) -> common::app::TestResponse {
    app.patch_json(&format!("/api/admin/devices/{MAC}"), &[AUTH], body)
        .await
}

/// The composition trap. A `final` missing from `MANAGED_DEVICE_KEYS` is
/// carried across by `unmanaged_device_keys` and re-inserted AFTER
/// `device_block`, so the PATCH answers 200 and the file keeps the old block.
/// Only asserting BOTH fields on disk catches it: the new one is lost to the
/// old block, and a whole-block replacement would lose the old one.
#[tokio::test]
async fn test_patch_final_dither_keeps_final_strength_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_with_final(&app).await;
    assert_eq!(
        device_on_disk(&path)["final"]["strength"].as_f64(),
        Some(0.4),
        "precondition: the file already holds final.strength"
    );

    let resp = patch(&app, r#"{"final":{"dither":"atkinson"}}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());

    let dev = device_on_disk(&path);
    assert_eq!(
        dev["final"]["dither"].as_str(),
        Some("atkinson"),
        "the patched field must reach the file\n{dev:?}"
    );
    assert_eq!(
        dev["final"]["strength"].as_f64(),
        Some(0.4),
        "the untouched final field must survive\n{dev:?}"
    );
    assert_eq!(
        dev["dither"].as_str(),
        Some("sierra"),
        "base rung untouched"
    );
}

/// An unrelated patch must not lose the final block either.
#[tokio::test]
async fn test_unrelated_patch_keeps_the_final_block() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_with_final(&app).await;
    let resp = patch(&app, r#"{"name":"Renamed"}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    assert_eq!(
        device_on_disk(&path)["final"]["strength"].as_f64(),
        Some(0.4)
    );
}

#[tokio::test]
async fn test_clear_addressing_base_final_field_and_whole_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_with_final(&app).await;
    let resp = patch(&app, r#"{"final":{"dither":"atkinson"}}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());

    // A bare name clears the base rung and never touches `final`.
    let resp = patch(&app, r#"{"clear":["dither"]}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    let dev = device_on_disk(&path);
    assert!(dev.get("dither").is_none(), "base dither cleared\n{dev:?}");
    assert_eq!(dev["final"]["dither"].as_str(), Some("atkinson"), "{dev:?}");

    // A dotted name clears that one field of the final rung.
    let resp = patch(&app, r#"{"clear":["final.dither"]}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    let dev = device_on_disk(&path);
    assert!(dev["final"].get("dither").is_none(), "{dev:?}");
    assert_eq!(dev["final"]["strength"].as_f64(), Some(0.4), "{dev:?}");

    // `final` clears the whole rung, and leaves no empty `final:` behind.
    let resp = patch(&app, r#"{"clear":["final"]}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    assert!(device_on_disk(&path).get("final").is_none());
}

#[tokio::test]
async fn test_clear_final_screen_is_rejected_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_with_final(&app).await;
    let resp = patch(&app, r#"{"clear":["final.screen"]}"#).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    assert!(resp.text().contains("final.screen"), "{}", resp.text());
}

/// `final: {screen: ...}` is as wrong in a request as in config.yaml.
#[tokio::test]
async fn test_final_with_a_non_cascade_field_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_with_final(&app).await;
    let resp = patch(&app, r#"{"final":{"screen":"x/y"}}"#).await;
    assert!(
        resp.status.is_client_error(),
        "accepted: {} {}",
        resp.status,
        resp.text()
    );
    assert!(device_on_disk(&path)["final"].get("screen").is_none());
}

/// `gamut` and `colors_actual` were writable by nothing; they must now set,
/// survive an unrelated patch, write floats as sent, and clear.
#[tokio::test]
async fn test_gamut_and_colors_actual_set_survive_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());
    seed_with_final(&app).await;

    let resp = patch(
        &app,
        r##"{"gamut":{"knee":0.7},"colors_actual":"#101010,#F0F0F0",
            "final":{"gamut":{"amount":0.3}}}"##,
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    let resp = patch(&app, r#"{"gamut":{"amount":0.5}}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());

    let text = std::fs::read_to_string(&path).unwrap();
    for expected in ["knee: 0.7", "amount: 0.5", "amount: 0.3", "strength: 0.4"] {
        assert!(text.contains(expected), "want `{expected}` in:\n{text}");
    }
    assert!(!text.contains("0.699"), "widened f32 leaked:\n{text}");
    let dev = device_on_disk(&path);
    assert_eq!(dev["colors_actual"].as_str(), Some("#101010,#F0F0F0"));
    assert_eq!(
        dev["gamut"]["knee"].as_f64(),
        Some(0.7),
        "merged, not replaced"
    );

    let resp = patch(&app, r#"{"clear":["gamut","colors_actual"]}"#).await;
    assert_eq!(resp.status, StatusCode::OK, "{}", resp.text());
    let dev = device_on_disk(&path);
    assert!(dev.get("gamut").is_none() && dev.get("colors_actual").is_none());
    assert_eq!(
        dev["final"]["gamut"]["amount"].as_f64(),
        Some(0.3),
        "{dev:?}"
    );
}
