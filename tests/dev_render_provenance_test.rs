//! `POST /dev/api/render` answers with `X-Byonk-Resolution`: for every
//! render setting, every rung's value and status and which rung won.
//!
//! See `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
//! §4.4–4.6 and the plan `docs/superpowers/plans/2026-10-05-render-provenance.md`.

mod common;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn resolution(r: &common::app::TestResponse) -> serde_json::Value {
    let raw = r
        .headers
        .get("x-byonk-resolution")
        .expect("a dev render must carry X-Byonk-Resolution")
        .to_str()
        .expect("the header must be visible ASCII");
    serde_json::from_str(raw).unwrap()
}

/// A screen with a `data-byonk-tone="continuous"` region, so that a change of
/// dither algorithm is actually observable in the rendered PNG.
///
/// Unmarked content is matched against the official palette regardless of
/// tuning (`eink-dither`'s `RegionMap::model`), so a flat-colour screen would
/// render identically under every algorithm and these tests would pass with
/// the bug present. See the same note in `dev_overrides_test.rs`.
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

/// `local/plain`: a flat screen whose script sets `dither` and `refresh_rate`,
/// so the screen rung carries values the device rung can outrank or lose to.
fn scaffold_plain_screen(dir: &std::path::Path) {
    let screen_dir = dir.join("local").join("plain");
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!(
            "title: Plain\ndescription: Flat screen, for tests.\nbyonk: \"{}\"\nrefresh: 900\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("script.lua"),
        "return { data = { width = layout.width, height = layout.height }, dither = \"atkinson\", refresh_rate = 60 }\n",
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {{ data.width }} {{ data.height }}" width="{{ data.width }}" height="{{ data.height }}">
  <rect x="0" y="0" width="{{ data.width }}" height="{{ data.height }}" fill="#FFFFFF"/>
</svg>
"##,
    )
    .unwrap();
}

/// `local/needs`: fails unless `params.greeting == "hi"`, naming what it got,
/// so a test can see exactly which params reached the script.
fn scaffold_needs_screen(dir: &std::path::Path) {
    let screen_dir = dir.join("local").join("needs");
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!(
            "title: Needs\ndescription: Params probe, for tests.\nbyonk: \"{}\"\nrefresh: 900\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("script.lua"),
        "if params.greeting ~= \"hi\" then error(\"greeting was \" .. tostring(params.greeting)) end\nreturn { data = { width = layout.width, height = layout.height } }\n",
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {{ data.width }} {{ data.height }}" width="{{ data.width }}" height="{{ data.height }}">
  <rect x="0" y="0" width="{{ data.width }}" height="{{ data.height }}" fill="#FFFFFF"/>
</svg>
"##,
    )
    .unwrap();
}

fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    scaffold_gradient_screen(dir);
    scaffold_plain_screen(dir);
    scaffold_needs_screen(dir);
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
  kitchen:
    screen: local/gradient
    panel: e1004
    dither: floyd-steinberg
    refresh: 600
    params:
      greeting: hi
  porch:
    screen: local/plain
    final:
      strength: 0.4
",
        dir.join("local").display()
    );
    TestApp::new_with_config_yaml(&yaml, dir)
}

#[tokio::test]
async fn the_header_reports_every_setting_with_every_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"screen":"local/plain"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert!(r.is_png());
    let res = resolution(&r);
    for key in [
        "colors",
        "colors_actual",
        "dither",
        "max_error",
        "noise_scale",
        "chroma_clamp",
        "strength",
        "gamut.knee",
        "gamut.amount",
        "gamut.max_compression",
        "refresh",
    ] {
        let s = &res[key];
        assert!(s.is_object(), "missing {key}");
        for rung in [
            "override",
            "device_final",
            "screen",
            "device_base",
            "panel",
            "fallback",
        ] {
            // `order` names all six rungs; a rung missing from `rungs` is
            // absent, a present one carries its status.
            assert!(
                s["order"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!(rung)),
                "{key}.order lacks {rung}"
            );
            let entry = &s["rungs"][rung];
            assert!(
                entry.is_null() || entry["status"].is_string(),
                "{key}.{rung}"
            );
        }
        assert!(s["rungs"].get("device_final").is_none(), "{key}");
    }
    assert_eq!(res["dither"]["from"], "screen");
    assert_eq!(res["refresh"]["effective"], 60);
}

#[tokio::test]
async fn screen_and_device_together_take_the_screen_and_the_devices_settings() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/plain","device":"kitchen"}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    // Spec §2: a screen that sets its own dither beats the device's bare
    // `dither:` (piece 3 flipped this; the veto now lives in `final:`).
    assert_eq!(res["dither"]["effective"], "atkinson");
    assert_eq!(res["dither"]["from"], "screen");
    assert_eq!(
        res["dither"]["rungs"]["device_base"]["value"],
        "floyd-steinberg"
    );
    assert_eq!(res["dither"]["rungs"]["device_base"]["status"], "outranked");
    // The screen still wins refresh; the device's value is outranked.
    assert_eq!(res["refresh"]["effective"], 60);
    assert_eq!(res["refresh"]["rungs"]["device_base"]["value"], 600);
    // The device has no `colors`; its panel's palette wins.
    assert_eq!(res["colors"]["from"], "panel");
}

#[tokio::test]
async fn device_alone_renders_its_own_screen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"kitchen"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    // local/gradient returns no dither and refresh_rate 3600.
    assert_eq!(resolution(&r)["refresh"]["effective"], 3600);
}

#[tokio::test]
async fn a_draft_base_replaces_the_configured_device_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    // The draft has no dither: replacing the rung REMOVES the device's
    // floyd-steinberg, so the screen's atkinson wins.
    let r = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/plain","device":"kitchen","draft":{"base":{"strength":0.5}}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    assert_eq!(res["dither"]["from"], "screen");
    assert_eq!(res["strength"]["rungs"]["device_base"]["value"], 0.5);
}

#[tokio::test]
async fn a_draft_panel_with_a_wrong_length_calibration_is_rejected_in_the_report() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r##"{"screen":"local/plain","panel":"e1004","draft":{"panel":{"colors":"#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00","colors_actual":"#101010,#E8E8E8"}}}"##)
        .await;
    assert!(
        r.status.is_success(),
        "a calibration mistake must not fail the render"
    );
    let res = resolution(&r);
    assert_eq!(res["colors_actual"]["rungs"]["panel"]["status"], "rejected");
    assert!(res["colors_actual"]["rungs"]["panel"]["reason"]
        .as_str()
        .unwrap()
        .contains("has 2 usable entries"));
}

#[tokio::test]
async fn fields_the_cascade_cannot_carry_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    for body in [
        r##"{"screen":"local/plain","override":{"colors":"#000000,#FFFFFF"}}"##,
        r#"{"screen":"local/plain","override":{"refresh":60}}"#,
        r#"{"screen":"local/plain","dither":"atkinson"}"#,
        r#"{"screen":"local/plain","draft":{"nope":{}}}"#,
    ] {
        let r = app.post_json("/dev/api/render", &[AUTH], body).await;
        assert!(r.status.is_client_error(), "{body} → {}", r.status);
        assert_ne!(
            r.status,
            axum::http::StatusCode::METHOD_NOT_ALLOWED,
            "{body}"
        );
    }
}

#[tokio::test]
async fn a_draft_final_replaces_the_configured_final_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    // porch has final.strength 0.4. A draft final without strength removes it,
    // and its own dither vetoes the screen's atkinson.
    let r = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"device":"porch","draft":{"final":{"dither":"stucki"}}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    assert_eq!(res["dither"]["effective"], "stucki");
    assert_eq!(res["dither"]["from"], "device_final");
    assert!(
        res["strength"]["rungs"].get("device_final").is_none(),
        "the draft final rung replaced the configured one, strength included"
    );
}

#[tokio::test]
async fn without_a_draft_final_the_configured_final_rung_stays() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"porch"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(
        resolution(&r)["strength"]["rungs"]["device_final"]["value"],
        0.4
    );
}

#[tokio::test]
async fn a_draft_base_may_carry_colors_actual() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r##"{"device":"kitchen","draft":{"base":{"colors_actual":"#111111,#EEEEEE,#AA2222,#CCCC33,#2222AA,#22AA22"}}}"##,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    assert_eq!(res["colors_actual"]["from"], "device_base");
    assert_eq!(
        res["colors_actual"]["rungs"]["panel"]["status"],
        "outranked"
    );
}

#[tokio::test]
async fn draft_params_replace_the_device_params_and_simulated_ones_merge_on_top() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    // Configured params (greeting: hi) satisfy the probe.
    let ok = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/needs","device":"kitchen"}"#,
        )
        .await;
    assert!(ok.status.is_success(), "{} {}", ok.status, ok.text());
    // An empty draft removes them: the script sees nil.
    let gone = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/needs","device":"kitchen","draft":{"params":{}}}"#,
        )
        .await;
    assert_eq!(gone.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    assert!(gone.text().contains("greeting was nil"), "{}", gone.text());
    // Simulated params still land on top of the draft.
    let back = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/needs","device":"kitchen","draft":{"params":{}},"simulated":{"params":{"greeting":"hi"}}}"#,
        )
        .await;
    assert!(back.status.is_success(), "{} {}", back.status, back.text());
}

#[tokio::test]
async fn neither_screen_nor_device_is_a_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app.post_json("/dev/api/render", &[AUTH], "{}").await;
    assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_get_render_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .get_with_headers("/dev/api/render?screen=local/plain", &[AUTH])
        .await;
    assert_eq!(r.status, axum::http::StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn the_header_stays_inside_its_budget_for_sixteen_entry_palettes() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let grey16: Vec<String> = (0..16)
        .map(|i| {
            let v = i * 17;
            format!("#{v:02X}{v:02X}{v:02X}")
        })
        .collect();
    let g = grey16.join(",");
    let body = format!(
        r#"{{"screen":"local/plain","device":"kitchen","draft":{{"base":{{"colors":"{g}"}},"panel":{{"colors":"{g}","colors_actual":"{g}"}}}},"override":{{"colors_actual":"{g}"}}}}"#
    );
    let r = app.post_json("/dev/api/render", &[AUTH], &body).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let len = r.headers.get("x-byonk-resolution").unwrap().len();
    // nginx's default `proxy_buffer_size` is one memory page (4 KiB on
    // x86_64) and must hold the status line and EVERY response header; past
    // it a reverse-proxied preview fails with 502. 3.5 KiB leaves room for
    // the other headers. (HA Ingress's aiohttp allows 8190 per field.)
    assert!(len < 3584, "X-Byonk-Resolution is {len} bytes");
}

/// The image and the report come from one computation: whatever algorithm the
/// header names is the one the PNG was dithered with.
#[tokio::test]
async fn the_reported_algorithm_is_the_rendered_algorithm() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let render = |body: &'static str| {
        let app = &app;
        async move {
            let r = app.post_json("/dev/api/render", &[AUTH], body).await;
            assert!(r.status.is_success(), "{} {}", r.status, r.text());
            let algo = resolution(&r)["dither"]["effective"].clone();
            (algo, r.body)
        }
    };
    let (configured_algo, configured_png) = render(r#"{"device":"kitchen"}"#).await;
    let (same_algo, same_png) =
        render(r#"{"device":"kitchen","override":{"dither":"floyd-steinberg"}}"#).await;
    let (other_algo, other_png) =
        render(r#"{"device":"kitchen","override":{"dither":"atkinson"}}"#).await;

    assert_eq!(configured_algo, "floyd-steinberg");
    assert_eq!(same_algo, "floyd-steinberg");
    assert_eq!(
        configured_png, same_png,
        "same reported algorithm, different image"
    );
    assert_eq!(other_algo, "atkinson");
    assert_ne!(
        configured_png, other_png,
        "different reported algorithm, same image"
    );
}
