//! The server answers "which devices use this panel", because a device
//! without `panel:` resolves through the Board/Model headers it polls with,
//! which the dev UI cannot see (spec §5, controller ruling R-F).

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    let yaml = "\
admin:
  token: secret
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  other:
    name: Other
    colors: '#000000,#FFFFFF'
devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
  office:
    screen: byonk-builtin/default
  attic:
    screen: byonk-builtin/default
  garage:
    screen: byonk-builtin/default
    panel: other
";
    TestApp::new_with_config_yaml(yaml, dir)
}

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

async fn usage(app: &TestApp) -> serde_json::Value {
    let r = app.get_with_headers("/dev/api/panel-usage", &[AUTH]).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    r.json()
}

#[tokio::test]
async fn before_any_poll_only_explicit_panels_count_and_the_rest_are_unseen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let u = usage(&app).await;
    assert_eq!(u["panels"]["e1004"], serde_json::json!(["kitchen"]));
    assert_eq!(u["panels"]["other"], serde_json::json!(["garage"]));
    assert_eq!(u["unseen"], serde_json::json!(["attic", "office"]));
}

#[tokio::test]
async fn a_poll_resolves_a_device_by_board_or_by_model() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    poll(&app, "office", "og", Some("e1004")).await;
    poll(&app, "attic", "e1004", None).await;
    let u = usage(&app).await;
    assert_eq!(
        u["panels"]["e1004"],
        serde_json::json!(["attic", "kitchen", "office"])
    );
    assert_eq!(u["unseen"], serde_json::json!([]));
}

#[tokio::test]
async fn a_seen_device_matching_no_panel_is_neither_counted_nor_unseen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    poll(&app, "office", "og", Some("something-else")).await;
    let u = usage(&app).await;
    assert_eq!(u["panels"]["e1004"], serde_json::json!(["kitchen"]));
    assert_eq!(u["unseen"], serde_json::json!(["attic"]));
}

#[tokio::test]
async fn the_dev_render_resolves_a_board_matched_device_after_it_polled() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let header = |r: &common::app::TestResponse| -> serde_json::Value {
        serde_json::from_str(r.headers["x-byonk-resolution"].to_str().unwrap()).unwrap()
    };
    let before = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"office"}"#)
        .await;
    assert_eq!(header(&before)["colors"]["from"], "fallback");
    poll(&app, "office", "og", Some("e1004")).await;
    let after = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"office"}"#)
        .await;
    assert_eq!(header(&after)["colors"]["from"], "panel");
}
