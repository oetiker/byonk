//! Device Save from the dev UI: the same write the admin PATCH and the MCP
//! tool make, behind the dev session instead of the bearer token.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn config() -> &'static str {
    "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    dither: floyd-steinberg
"
}

#[tokio::test]
async fn saving_writes_the_final_rung_and_plain_fields() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"final":{"dither":"atkinson"},"name":"Kitchen","min_png_bytes":102401}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let d = &doc["devices"]["kitchen"];
    assert_eq!(d["final"]["dither"], "atkinson");
    assert_eq!(d["dither"], "floyd-steinberg", "the base rung is untouched");
    assert_eq!(d["name"], "Kitchen");
    assert_eq!(d["min_png_bytes"], 102401);
}

#[tokio::test]
async fn clear_removes_a_base_field() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["dither"]}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert!(doc["devices"]["kitchen"].get("dither").is_none());
}

#[tokio::test]
async fn an_unknown_device_is_a_404_and_a_bad_screen_a_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json("/dev/api/devices/nosuch", &[AUTH], r#"{"name":"x"}"#)
        .await;
    assert_eq!(r.status, axum::http::StatusCode::NOT_FOUND);
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"screen":"local/does-not-exist"}"#,
        )
        .await;
    assert!(r.status.is_client_error(), "{}", r.status);
}

fn device_of(path: &std::path::Path, key: &str) -> serde_yaml::Value {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["devices"][key].clone()
}

fn rich_config() -> &'static str {
    "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    gamut:
      knee: 0.5
      amount: 0.9
    final:
      dither: floyd-steinberg
      strength: 0.4
"
}

/// Ruling C3: clear first, then set -- a key both cleared and set ends up
/// set, so a whole rung can be replaced in one atomic request.
#[tokio::test]
async fn clear_and_set_of_the_same_key_ends_up_set() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(rich_config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["final"],"final":{"dither":"atkinson"}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let d = device_of(&config_path, "kitchen");
    assert_eq!(d["final"]["dither"], "atkinson");
    assert!(
        d["final"].get("strength").is_none(),
        "the cleared rung kept strength: {d:?}"
    );
}

#[tokio::test]
async fn one_gamut_sub_field_can_be_cleared_and_the_others_stay() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(rich_config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["gamut.knee","final.strength"],"final":{"dither":"stucki"}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let d = device_of(&config_path, "kitchen");
    assert!(d["gamut"].get("knee").is_none(), "{d:?}");
    assert_eq!(d["gamut"]["amount"].as_f64(), Some(0.9));
    assert_eq!(d["final"]["dither"], "stucki");
    assert!(d["final"].get("strength").is_none(), "{d:?}");
}

#[tokio::test]
async fn clearing_and_setting_params_replaces_them_wholesale() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    params:
      a: 1
      b: 2
";
    let (app, config_path) = TestApp::new_with_config_yaml(yaml, dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["params"],"params":{"c":3}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let d = device_of(&config_path, "kitchen");
    assert_eq!(d["params"]["c"], 3);
    assert!(d["params"].get("a").is_none(), "{d:?}");
}

/// The admin PATCH shares `apply_device_patch`, so it follows the same rule.
#[tokio::test]
async fn the_admin_patch_follows_the_same_clear_then_set_rule() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(rich_config(), dir.path());
    let r = app
        .patch_json(
            "/api/admin/devices/kitchen",
            &[AUTH],
            r#"{"clear":["final.dither"],"final":{"dither":"atkinson"}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(
        device_of(&config_path, "kitchen")["final"]["dither"],
        "atkinson"
    );
}

#[tokio::test]
async fn saving_drops_the_device_override() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(config(), dir.path());
    app.put_json(
        "/dev/api/overrides/devices/kitchen",
        &[AUTH],
        r#"{"strength":0.5}"#,
    )
    .await;
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[AUTH], r#"{"strength":0.5}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let v: serde_json::Value = app
        .get_with_headers("/dev/api/overrides", &[AUTH])
        .await
        .json();
    assert_eq!(v["devices"], serde_json::json!([]));
}

#[tokio::test]
async fn saving_on_an_embedded_config_is_a_conflict() {
    let app = TestApp::new_with_dev_login(Some("secret"), "u", "p");
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[AUTH], r#"{"name":"x"}"#)
        .await;
    assert_eq!(r.status, axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_route_needs_a_dev_login() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[], r#"{"name":"x"}"#)
        .await;
    assert_eq!(r.status, axum::http::StatusCode::UNAUTHORIZED);
}

const COMMENTED: &str = "\
admin:
  token: secret
devices:
  # the hall display, first on purpose
  hall:
    screen: byonk-builtin/default
  # the kitchen display
  kitchen:
    screen: byonk-builtin/default   # what it shows
    dither: floyd-steinberg   # a default
    # hand-tuned on 2026-10-01
    strength: 1
    error_clamp: 0.2
    final: {dither: atkinson}
  office: {screen: byonk-builtin/default, name: Office}   # flow style
# end of devices
";

/// The whole file, byte for byte: the entry stays where it is, every comment
/// stays on its line, and untouched keys keep their spelling (`strength: 1`).
#[tokio::test]
async fn device_save_edits_the_entry_in_place_and_keeps_every_comment() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED, dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["dither"],"name":"Kitchen","final":{"strength":0.5}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        "\
admin:
  token: secret
devices:
  # the hall display, first on purpose
  hall:
    screen: byonk-builtin/default
  # the kitchen display
  kitchen:
    screen: byonk-builtin/default   # what it shows
    # hand-tuned on 2026-10-01
    strength: 1
    error_clamp: 0.2
    final: {dither: atkinson, strength: 0.5}
    name: Kitchen
  office: {screen: byonk-builtin/default, name: Office}   # flow style
# end of devices
"
    );
}

#[tokio::test]
async fn the_admin_patch_keeps_a_flow_device_in_flow_style() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED, dir.path());
    let r = app
        .patch_json(
            "/api/admin/devices/office",
            &[AUTH],
            r#"{"name":"Front office"}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        COMMENTED.replace(
            "  office: {screen: byonk-builtin/default, name: Office}   # flow style",
            "  office: {screen: byonk-builtin/default, name: \"Front office\"}   # flow style"
        )
    );
}

/// Ruling C3 still holds in place: clear first, then set.
#[tokio::test]
async fn clear_then_set_of_the_same_key_still_ends_up_set_and_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED, dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["dither"],"dither":"stucki"}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        COMMENTED.replace(
            "    dither: floyd-steinberg   # a default\n",
            "    dither: stucki   # a default\n"
        )
    );
}
