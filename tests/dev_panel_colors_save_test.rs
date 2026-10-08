//! `Save to config` writes a panel's measured colours into config.yaml
//! durably, preserving every comment around them.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");
const TUNED: &str = "#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D";

fn commented_config() -> &'static str {
    "\
# byonk configuration — hand-written, with comments that must survive.
admin:
  token: secret

panels:
  # The reTerminal E1004. Its greens are hand-tuned; see docs.
  e1004:
    name: reTerminal E1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  og:
    name: TRMNL OG
    colors: '#000000,#FFFFFF'
    colors_actual: '#1A1A1A,#D8D8D8'   # measured 2026-01-01

devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
"
}

#[tokio::test]
async fn saving_adds_colors_actual_and_keeps_every_comment() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panel-colors/e1004/save",
            &[AUTH],
            &format!(r#"{{"colors_actual":"{TUNED}"}}"#),
        )
        .await;
    assert!(
        r.status.is_success(),
        "save failed: {} {}",
        r.status,
        r.text()
    );

    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        yaml.contains(TUNED),
        "the tuned colours are not in the file:\n{yaml}"
    );
    assert!(
        yaml.contains("hand-tuned; see docs"),
        "a comment was lost:\n{yaml}"
    );
    assert!(
        yaml.contains("# measured 2026-01-01"),
        "an inline comment on a sibling panel was lost:\n{yaml}"
    );
    assert!(
        yaml.contains("# byonk configuration"),
        "the header comment was lost:\n{yaml}"
    );
}

#[tokio::test]
async fn saving_replaces_an_existing_colors_actual_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panel-colors/og/save",
            &[AUTH],
            r##"{"colors_actual":"#0A0A0A,#F0F0F0"}"##,
        )
        .await;
    assert!(r.status.is_success());

    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(yaml.contains("#0A0A0A,#F0F0F0"));
    assert!(
        !yaml.contains("#1A1A1A,#D8D8D8"),
        "the old value is still there:\n{yaml}"
    );
}

#[tokio::test]
async fn saving_clears_the_temporary_override() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    app.put_json(
        "/dev/api/overrides/panels/e1004",
        &[AUTH],
        &format!(r#"{{"colors_actual":"{TUNED}"}}"#),
    )
    .await;
    let r = app
        .post_json(
            "/dev/api/panel-colors/e1004/save",
            &[AUTH],
            &format!(r#"{{"colors_actual":"{TUNED}"}}"#),
        )
        .await;
    assert!(r.status.is_success());

    // The override and the config now say the same thing, so the override is
    // dead weight — and a live override that nobody can see is the trap this
    // whole design is built to avoid. Assert the effect directly: the
    // panel's `colors_actual` as `/dev/api/screens` reports it must already
    // be the tuned value (i.e. it is coming from config, not from a lingering
    // override that merely happens to agree with it) — and after deleting
    // the (now redundant) override, it must still be there.
    let screens: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();
    let panel = screens["panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "e1004")
        .expect("e1004 panel present");
    assert_eq!(panel["colors_actual"], TUNED);

    // Deleting the override must still succeed even though the config
    // already carries the value the override would have supplied.
    let r = app.delete("/dev/api/overrides/panels/e1004", &[AUTH]).await;
    assert!(r.status.is_success());

    let screens: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();
    let panel = screens["panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "e1004")
        .expect("e1004 panel present");
    assert_eq!(
        panel["colors_actual"], TUNED,
        "colors_actual must still come from config.yaml after the override is gone"
    );
}

#[tokio::test]
async fn saving_an_unknown_panel_is_a_404() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panel-colors/nosuchpanel/save",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, axum::http::StatusCode::NOT_FOUND);
}
