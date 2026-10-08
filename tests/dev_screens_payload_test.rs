//! `GET /dev/api/screens` carries every field the dev UI's config editor
//! edits, so a panel or device draft starts from the configured values.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn devices_and_panels_carry_their_rungs_and_plain_fields() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "\
admin:
  token: secret
panels:
  zeta:
    name: Zeta
    colors: '#000000,#FFFFFF'
  e1004:
    name: reTerminal E1004
    colors: '#000000,#FFFFFF'
    colors_actual: '#101010,#E8E8E8'
    dither:
      strength: 0.8
      atkinson:
        noise_scale: 2.0
devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
    name: Kitchen
    dither: floyd-steinberg
    strength: 0.8
    gamut:
      knee: 0.7
    temperature_profile: a
    maximum_compatibility: true
    min_png_bytes: 102401
    params:
      greeting: hi
    final:
      dither: atkinson
  attic:
    screen: byonk-builtin/default
";
    let (app, _) = TestApp::new_with_config_yaml(yaml, dir.path());
    let v: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();

    let ids: Vec<&str> = v["devices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["attic", "kitchen"], "sorted by id");
    let k = &v["devices"][1];
    assert_eq!(k["name"], "Kitchen");
    assert_eq!(k["params"]["greeting"], "hi");
    assert_eq!(k["base"]["dither"], "floyd-steinberg");
    // Ruling PF1: the numbers reach the UI without f32 noise.
    assert_eq!(k["base"]["strength"].as_f64(), Some(0.8));
    assert_eq!(k["base"]["gamut"]["knee"].as_f64(), Some(0.7));
    assert_eq!(k["final"]["dither"], "atkinson");
    assert_eq!(k["temperature_profile"], "a");
    assert_eq!(k["maximum_compatibility"], true);
    assert_eq!(k["min_png_bytes"], 102401);
    assert!(k.get("dither").is_none(), "base.dither replaces it");
    assert!(v["devices"][0]["final"].is_null());

    let pids: Vec<&str> = v["panels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(pids, ["e1004", "zeta"]);
    let p = &v["panels"][0];
    assert_eq!(p["dither"]["strength"].as_f64(), Some(0.8)); // ruling PF2: tidy, not 0.800000011920929
    assert_eq!(p["dither"]["atkinson"]["noise_scale"].as_f64(), Some(2.0));
    assert!(v["panels"][1]["dither"].is_null());
}
