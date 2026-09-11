//! The dev colour override is keyed by PANEL, so every device using that
//! panel profile sees it — which is the point: a calibration belongs to the
//! hardware, not to one unit of it.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// Two devices, one panel profile. `e1004` carries a `match:` so the
/// Board-header route is exercised too.
fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    let yaml = "\
admin:
  token: secret
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
    colors_actual: '#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D'
devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
  hallway:
    screen: byonk-builtin/default
    panel: e1004
  office:
    screen: byonk-builtin/default
";
    TestApp::new_with_config_yaml(yaml, dir)
}

#[tokio::test]
async fn put_and_delete_an_override_by_panel_id() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let r = app
        .put_json(
            "/dev/api/panel-colors/e1004",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00994D"}"##,
        )
        .await;
    assert!(r.status.is_success(), "PUT failed: {}", r.status);

    let r = app.delete("/dev/api/panel-colors/e1004", &[AUTH]).await;
    assert!(r.status.is_success(), "DELETE failed: {}", r.status);
}

#[tokio::test]
async fn an_unknown_panel_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let r = app
        .put_json(
            "/dev/api/panel-colors/nosuchpanel",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(
        r.status,
        axum::http::StatusCode::NOT_FOUND,
        "an override for a panel that does not exist can never be read back, \
         so it must be refused rather than stored where nothing will find it"
    );
}
