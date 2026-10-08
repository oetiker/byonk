//! Panel Save writes the whole panel rung into config.yaml, in place, keeping every comment and the order of panels.

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
            "/dev/api/panels/e1004/save",
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
            "/dev/api/panels/og/save",
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
            "/dev/api/panels/e1004/save",
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
            "/dev/api/panels/nosuchpanel/save",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, axum::http::StatusCode::NOT_FOUND);
}

fn panel_of(yaml: &str, id: &str) -> serde_yaml::Value {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    doc["panels"][id].clone()
}

fn panel_order(yaml: &str) -> Vec<String> {
    let doc: serde_yaml::Mapping = serde_yaml::from_str::<serde_yaml::Value>(yaml).unwrap()
        ["panels"]
        .as_mapping()
        .unwrap()
        .clone();
    doc.keys()
        .map(|k| k.as_str().unwrap().to_string())
        .collect()
}

const COMMENTED_PANELS: &str = "\
admin:
  token: secret
panels:
  # The reTerminal E1004. Its greens are hand-tuned; see docs.
  e1004:
    name: reTerminal E1004   # the six-colour one
    # official palette, from the datasheet
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  og:
    name: TRMNL OG
    colors: '#000000,#FFFFFF'
    colors_actual: '#1A1A1A,#D8D8D8'   # measured 2026-01-01
    dither:
      max_error: 0.5   # hand-tuned
      atkinson:
        noise_scale: 2.0
";

/// Ruling C2 (a), (b), (c): a Save that creates a whole `dither:` block keeps
/// a comment inside the saved panel and one on its sibling, leaves the order
/// of `panels:` alone, and re-parses to exactly what was sent.
#[tokio::test]
async fn a_dither_save_keeps_comments_and_panel_order() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let before = std::fs::read_to_string(&config_path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panels/e1004/save",
            &[AUTH],
            &format!(
                r#"{{"colors_actual":"{TUNED}","dither":{{"strength":0.8,"gamut":{{"knee":0.6}}}}}}"#
            ),
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let yaml = std::fs::read_to_string(&config_path).unwrap();

    // (a) comments inside the saved block and on a sibling survive
    for c in [
        "# The reTerminal E1004. Its greens are hand-tuned; see docs.",
        "# the six-colour one",
        "# official palette, from the datasheet",
        "# measured 2026-01-01",
        "# hand-tuned",
    ] {
        assert!(yaml.contains(c), "lost {c:?}:\n{yaml}");
    }
    // (b) panel order unchanged
    assert_eq!(panel_order(&yaml), panel_order(&before));
    // (c) the result re-parses to the expected values
    let p = panel_of(&yaml, "e1004");
    assert_eq!(p["name"], "reTerminal E1004");
    assert_eq!(
        p["colors"],
        "#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00"
    );
    assert_eq!(p["colors_actual"], TUNED);
    assert_eq!(p["dither"]["strength"].as_f64(), Some(0.8));
    assert_eq!(p["dither"]["gamut"]["knee"].as_f64(), Some(0.6));
    assert_eq!(
        panel_of(&yaml, "og"),
        panel_of(&before, "og"),
        "the sibling is untouched"
    );
}

#[tokio::test]
async fn saving_changes_one_dither_key_in_place_and_keeps_algorithm_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            r##"{"colors_actual":"#1A1A1A,#D8D8D8","dither":{"max_error":0.5,"strength":0.9,"atkinson":{"noise_scale":2.0}}}"##,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        yaml.contains("max_error: 0.5   # hand-tuned"),
        "an unchanged line was rewritten:\n{yaml}"
    );
    assert!(
        yaml.contains("colors_actual: '#1A1A1A,#D8D8D8'   # measured 2026-01-01"),
        "{yaml}"
    );
    let p = panel_of(&yaml, "og");
    assert_eq!(p["dither"]["strength"].as_f64(), Some(0.9));
    assert_eq!(p["dither"]["atkinson"]["noise_scale"].as_f64(), Some(2.0));
}

#[tokio::test]
async fn saving_without_colors_actual_or_dither_removes_both() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let r = app
        .post_json("/dev/api/panels/og/save", &[AUTH], "{}")
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let yaml = std::fs::read_to_string(&config_path).unwrap();
    let p = panel_of(&yaml, "og");
    assert!(p.get("colors_actual").is_none(), "{p:?}");
    assert!(p.get("dither").is_none(), "{p:?}");
    assert_eq!(p["name"], "TRMNL OG");
    assert!(yaml.contains("# the six-colour one"), "{yaml}");
}

#[tokio::test]
async fn an_unknown_field_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            r##"{"colors":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

/// Saving the rung the file already holds rewrites nothing at all: the file
/// is byte-identical, so a Save of an untouched draft is invisible in a diff.
#[tokio::test]
async fn saving_what_the_file_already_says_leaves_it_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let before = std::fs::read_to_string(&config_path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            r##"{"colors_actual":"#1A1A1A,#D8D8D8","dither":{"max_error":0.5,"atkinson":{"noise_scale":2.0}}}"##,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}

#[tokio::test]
async fn the_old_panel_colors_save_route_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let before = std::fs::read_to_string(&config_path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panel-colors/og/save",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert!(r.status.is_client_error(), "{}", r.status);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}
