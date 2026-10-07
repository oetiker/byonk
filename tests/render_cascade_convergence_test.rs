//! Spec §7: "The five call paths agree." One device, one screen, one panel,
//! resolved through `/api/display`, `POST /dev/api/render`,
//! `ScreenStore::render`, the CLI and `/api/admin/devices/{key}/preview`.
//!
//! The fixture deliberately contests every rung it can: the panel has a
//! per-algorithm dither block, the device sets a base rung AND a final rung,
//! and the script sets its own algorithm and tuning. A fixture that set one
//! value per setting would pass against five different resolvers.
//!
//! The `Override` rung is the exception: it is empty on all five paths. The
//! CLI and the admin preview have no override by design, so it is not a
//! convergence dimension and nothing here exercises it.
//!
//! It carries TWO devices, because one cannot contest every ordering. A
//! device whose `final.dither` vetoes the script masks the screen-versus-base
//! ordering entirely — reverse `ORDER_DITHER` and a one-device fixture stays
//! green, which is exactly the release's one behaviour change going
//! unobserved. So the second device states a bare `dither:` default and no
//! `final:` block, and the screen must outrank it.
//!
//! **Every `via_*` drives its path's own entry point** — the axum handler,
//! `ScreenStore::render`, or (for the CLI, whose `run_render_command` lives in
//! an unreachable binary crate and writes a file) the same
//! `cli_render_inputs_for` that `run_render_command` calls. None of them
//! builds a `RenderInputs` by hand: a test that did would re-test the
//! resolver, which `src/api/resolution.rs`'s own unit tests already cover, and
//! would still pass with two rungs swapped in a handler — precisely the wiring
//! this release rewired.
//!
//! The settings are compared, never the PNG bytes: two paths can render the
//! same pixels from different settings, and the point is the settings.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use byonk::api::display::{colors_to_hex_strings, parse_colors_header, RenderParams};
use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::{create_app_state_with_config, AppState};
use byonk::services::screen_store::{DevicePreview, RenderOpts};
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");
/// The device that vetoes the script's algorithm with `final.dither`.
const DEVICE_VETO: &str = "AA:BB:CC:DD:EE:FF";
/// The device that states a bare `dither:` default the screen outranks —
/// the release's one behaviour change.
const DEVICE_DEFAULT: &str = "BB:CC:DD:EE:FF:AA";
const SCREEN: &str = "local/cascade";
const PANEL: &str = "trmnl_x";

/// The panel's palette, deliberately NOT the 4-grey `DEFAULT_COLORS` every
/// path falls back to: an identical value on the panel rung and the fallback
/// would leave `colors` uncontested, and a path that dropped the panel rung
/// would still agree with the other four.
const PANEL_COLORS: &str = "#000000,#3C3C3C,#B4B4B4,#FFFFFF";
const PANEL_COLORS_ACTUAL: &str = "#0A0A0A,#4F4F4F,#A6A6A6,#F5F5F5";

/// `config.yaml` for the fixture. The panel's `dither:` block is written in
/// the shape `PanelDitherConfig` actually deserializes — scalar keys are the
/// flat defaults, map-valued keys are per-algorithm overrides. There is no
/// `defaults:`/`algorithms:` nesting; writing one would create two algorithms
/// called "defaults" and "algorithms" and silently leave the real blocks unused.
///
/// Hex colours are single-quoted YAML scalars, not Rust `"#…"` inside a raw
/// string: `"#` terminates an `r#"…"#` literal.
fn config_yaml(dir: &std::path::Path) -> String {
    format!(
        "\
admin:
  token: secret
screen_repos:
  local:
    path: {repo}
panels:
  {PANEL}:
    name: TRMNL X
    colors: '{PANEL_COLORS}'
    colors_actual: '{PANEL_COLORS_ACTUAL}'
    dither:
      max_error: 1.0
      atkinson:
        noise_scale: 0.25
      stucki:
        noise_scale: 0.75
devices:
  '{DEVICE_VETO}':
    screen: {SCREEN}
    panel: {PANEL}
    dither: sierra          # base rung -- the screen outranks this now
    strength: 0.9           # base rung
    refresh: 600            # base rung
    gamut:
      knee: 0.8             # base rung -- unasserted by every wiring test
    final:
      dither: atkinson      # the veto: beats the script's stucki
      refresh: 300          # the owner's veto on how often it wakes up
  '{DEVICE_DEFAULT}':
    screen: {SCREEN}
    panel: {PANEL}
    dither: sierra          # base rung, with no `final:` above it
    strength: 0.4           # base rung
    gamut:
      amount: 0.6           # a DIFFERENT gamut field, on the same rung
",
        repo = dir.join("local").display(),
    )
}

/// The screen rung. It sets its own algorithm and one tuning number, so the
/// screen rung is contested against both device rungs; `refresh_rate` makes it
/// contested for `refresh` too.
const SCRIPT: &str =
    "return { data = {}, dither = \"stucki\", max_error = 2.0, refresh_rate = 1200 }\n";

/// A `local` screen repo holding the one screen the fixture renders.
fn scaffold(dir: &std::path::Path) {
    let repo = dir.join("local");
    let screen = repo.join("cascade");
    std::fs::create_dir_all(&screen).unwrap();
    std::fs::write(
        repo.join("byonk-screens.yaml"),
        "name: local\ndescription: Convergence fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        screen.join("meta.yaml"),
        format!(
            "title: Cascade\ndescription: Convergence fixture.\nbyonk: \"{}\"\nrefresh: 1800\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    std::fs::write(screen.join("script.lua"), SCRIPT).unwrap();
    std::fs::write(
        screen.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#202020"/>
      <stop offset="100%" stop-color="#E0E0E0"/>
    </linearGradient>
  </defs>
  <g data-byonk-tone="continuous">
    <rect x="0" y="0" width="800" height="480" fill="url(#g)"/>
  </g>
</svg>
"##,
    )
    .unwrap();
}

/// The settings a render was resolved with — what the ditherer receives, not
/// what it drew. Fallback values are deliberately absent: the renderer never
/// sees one (plan ruling 7), so `None` here means "the engine's own default
/// applies", exactly as `RenderParams` means it.
#[derive(Debug, PartialEq)]
struct ExpectedParams {
    dither: Option<String>,
    max_error: Option<f32>,
    noise_scale: Option<f32>,
    chroma_clamp: Option<f32>,
    strength: Option<f32>,
    gamut_knee: Option<f32>,
    gamut_amount: Option<f32>,
    gamut_max_compression: Option<f32>,
    palette_hex: String,
    measured_hex: Option<String>,
}

/// Review Focus 4, asserted by the fixture itself: `final.dither = atkinson`
/// must win over the script's `stucki`, AND the panel's per-algorithm block
/// must be looked up for `atkinson` — so `noise_scale` must be 0.25, not 0.75
/// and not the flat default. Getting the veto right while looking its tuning
/// up for the loser is the one failure spec §4.2 exists to prevent.
fn expected_veto() -> ExpectedParams {
    ExpectedParams {
        dither: Some("atkinson".to_string()),
        // The script's, which no higher rung sets.
        max_error: Some(2.0),
        // The panel's block for the WINNING algorithm.
        noise_scale: Some(0.25),
        // Nothing sets it on any rung, and the engine's default (no damping)
        // is infinity, which is reported as no fallback value at all.
        chroma_clamp: None,
        // The device base rung's.
        strength: Some(0.9),
        // `gamut` is a struct that reaches the ditherer and the per-path
        // wiring tests do NOT cover it — a swapped gamut source passes every
        // one of them. So the fixture sets it on a rung and asserts it here.
        gamut_knee: Some(0.8),
        gamut_amount: None,
        gamut_max_compression: None,
        palette_hex: PANEL_COLORS.to_string(),
        measured_hex: Some(PANEL_COLORS_ACTUAL.to_string()),
    }
}

/// The release's one behaviour change, and the ordering `expected_veto`
/// cannot see: with no `final:` above it, a bare `devices.<key>.dither` loses
/// to a screen that sets its own. The panel block must then follow the
/// SCREEN's algorithm — `noise_scale` 0.75, the `stucki` block — which is the
/// same §4.2 rule checked against the other winner.
fn expected_screen_wins() -> ExpectedParams {
    ExpectedParams {
        dither: Some("stucki".to_string()),
        max_error: Some(2.0),
        noise_scale: Some(0.75),
        chroma_clamp: None,
        strength: Some(0.4),
        gamut_knee: None,
        gamut_amount: Some(0.6),
        gamut_max_compression: None,
        palette_hex: PANEL_COLORS.to_string(),
        measured_hex: Some(PANEL_COLORS_ACTUAL.to_string()),
    }
}

// ---- Three independent readers ------------------------------------------
//
// A single reader shared by all five paths could be wrong in a way that made
// every path agree on the same wrong answer. These three disagree by
// construction: one walks the JSON resolution report, one reads the cache
// entry `/api/display` left behind, one reads a `RenderParams` directly.

/// A number from the JSON resolution report, as the RENDERER would get it:
/// `None` when the winning rung is the fallback, mirroring
/// `Resolved::configured()`.
fn configured_f32(report: &serde_json::Value, key: &str) -> Option<f32> {
    let entry = &report[key];
    assert!(!entry.is_null(), "the report has no `{key}` setting");
    if entry["from"] == "fallback" {
        return None;
    }
    Some(entry["effective"].as_f64().expect("a number") as f32)
}

fn from_report(report: &serde_json::Value) -> ExpectedParams {
    let dither = &report["dither"];
    ExpectedParams {
        dither: (dither["from"] != "fallback")
            .then(|| dither["effective"].as_str().expect("a name").to_string()),
        max_error: configured_f32(report, "max_error"),
        noise_scale: configured_f32(report, "noise_scale"),
        chroma_clamp: configured_f32(report, "chroma_clamp"),
        strength: configured_f32(report, "strength"),
        gamut_knee: configured_f32(report, "gamut.knee"),
        gamut_amount: configured_f32(report, "gamut.amount"),
        gamut_max_compression: configured_f32(report, "gamut.max_compression"),
        // Unlike the numbers, the palette IS used when it comes from the
        // fallback — it is the model's own default, not an engine default.
        palette_hex: report["colors"]["effective"]
            .as_str()
            .expect("a palette")
            .to_string(),
        measured_hex: report["colors_actual"]["effective"]
            .as_str()
            .map(str::to_string),
    }
}

fn from_params(params: &RenderParams) -> ExpectedParams {
    ExpectedParams {
        dither: params.dither.clone(),
        max_error: params.max_error,
        noise_scale: params.noise_scale,
        chroma_clamp: params.chroma_clamp,
        strength: params.strength,
        gamut_knee: params.gamut.knee,
        gamut_amount: params.gamut.amount,
        gamut_max_compression: params.gamut.max_compression,
        palette_hex: colors_to_hex_strings(&params.palette).join(","),
        measured_hex: params
            .measured_colors
            .as_deref()
            .map(|c| colors_to_hex_strings(c).join(",")),
    }
}

/// The resolution JSON out of an `X-Byonk-Resolution` response header.
fn resolution_header(r: &common::app::TestResponse) -> serde_json::Value {
    let raw = r
        .headers
        .get("x-byonk-resolution")
        .expect("this path must answer with X-Byonk-Resolution")
        .to_str()
        .expect("the header must be visible ASCII");
    serde_json::from_str(raw).unwrap()
}

// ---- The fixture --------------------------------------------------------

struct Fixture {
    app: TestApp,
    state: AppState,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let config_path = dir.path().join("config.yaml");
        std::fs::write(&config_path, config_yaml(dir.path())).unwrap();
        let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
        let config = AppConfig::load_from_assets(&asset_loader).expect("load fixture config");
        let state = create_app_state_with_config(asset_loader, config).expect("create app state");
        Self {
            app: TestApp::from_state(state.clone()),
            state,
            _dir: dir,
        }
    }

    /// `/api/display` — the real-hardware path. It answers with an image URL,
    /// not with its resolution, so the settings are read out of the cache
    /// entry the handler stored: those are the values `/api/image/{hash}.png`
    /// dithers with, i.e. the ones that reach the glass.
    async fn via_api_display(&self, device: &str) -> ExpectedParams {
        let api_key = self.app.register_device(device).await;
        let headers = common::fixtures::display_headers(device, &api_key);
        let r = self
            .app
            .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text());
        let json: serde_json::Value = r.json();
        let filename = json["filename"].as_str().expect("a filename");
        let cached = self
            .app
            .content_cache
            .get(filename)
            .expect("/api/display must cache the content it resolved");
        ExpectedParams {
            dither: cached.dither.clone(),
            max_error: cached.max_error,
            noise_scale: cached.noise_scale,
            chroma_clamp: cached.chroma_clamp,
            strength: cached.strength,
            gamut_knee: cached.gamut.knee,
            gamut_amount: cached.gamut.amount,
            gamut_max_compression: cached.gamut.max_compression,
            palette_hex: colors_to_hex_strings(cached.colors.as_deref().expect("a palette"))
                .join(","),
            measured_hex: cached
                .colors_actual
                .as_deref()
                .map(|c| colors_to_hex_strings(c).join(",")),
        }
    }

    /// `POST /dev/api/render` — the dev UI's preview, which already reports
    /// its own resolution.
    async fn via_dev_render(&self, device: &str) -> ExpectedParams {
        let body = format!(r#"{{"device":"{device}"}}"#);
        let r = self.app.post_json("/dev/api/render", &[AUTH], &body).await;
        assert!(r.status.is_success(), "{} {}", r.status, r.text());
        assert!(r.is_png());
        from_report(&resolution_header(&r))
    }

    /// `ScreenStore::render` — the authoring renderer, driven the way a device
    /// preview drives it: the device's rungs in `RenderOpts::device`, never in
    /// the override slot.
    async fn via_screen_store(&self, device: &str) -> ExpectedParams {
        let config = self.state.config.load_full();
        let dc = config
            .get_device_config(device)
            .expect("the fixture's device");
        let opts = RenderOpts {
            model: "og".to_string(),
            panel: dc.panel.clone(),
            params: dc.params.clone(),
            device: Some(DevicePreview {
                mac: device.to_string(),
                colors: dc.colors.clone(),
                colors_actual: dc.colors_actual.clone(),
                dither: dc.dither.clone(),
                tuning: byonk::models::DitherTuningValues {
                    deprecated_error_clamp: None,
                    max_error: dc.max_error,
                    noise_scale: dc.noise_scale,
                    chroma_clamp: dc.chroma_clamp,
                    strength: dc.strength,
                    gamut: dc.gamut.clone(),
                },
                refresh: dc.refresh,
                final_rung: dc.device_final(),
                ..DevicePreview::default()
            }),
            ..RenderOpts::default()
        };
        let store = self.state.screen_store.clone();
        let result = tokio::task::spawn_blocking(move || store.render(SCREEN, opts))
            .await
            .unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let report = result
            .resolution
            .expect("a successful render must carry its resolution");
        from_report(&serde_json::to_value(report).unwrap())
    }

    /// The CLI. `run_render_command` initializes a global tracing subscriber
    /// and writes a PNG to a path, and lives in the binary crate an
    /// integration test cannot link — so this drives the same
    /// `cli_render_inputs_for` it calls, with the arguments it derives
    /// (device config by MAC, the panel that device names, and the `og`
    /// model's default palette for a run with no `--colors`).
    async fn via_cli(&self, device: &str) -> ExpectedParams {
        let config = self.state.config.load_full();
        let device_config = config.get_device_config(device);
        let panel = device_config
            .and_then(|d| d.panel.as_deref())
            .and_then(|name| config.get_panel(name));
        let pipeline = self.state.content_pipeline.clone();
        // `None` for the device context: the CLI builds one, but this
        // fixture's script reads nothing from `device`, and
        // `run_script_for_device` fills in the refresh rungs from the device
        // config either way.
        let mac = device.to_string();
        let script = tokio::task::spawn_blocking(move || {
            // `ContentError` wraps an `mlua::Error`, which is neither `Send`
            // nor `Sync`, so it cannot cross the blocking boundary.
            pipeline
                .run_script_for_device(&mac, None)
                .map_err(|e| e.to_string())
        })
        .await
        .unwrap()
        .expect("the fixture screen runs");
        let params =
            byonk::api::resolution::resolve_render(&byonk::api::resolution::cli_render_inputs_for(
                device_config,
                panel,
                &script,
                parse_colors_header("#000000,#555555,#AAAAAA,#FFFFFF"),
            ))
            .into_params();
        from_params(&params)
    }

    /// `GET /api/admin/devices/{key}/preview` — the Home Assistant device
    /// page's camera frame. `?force` skips the cache, so this renders and
    /// therefore reports its resolution.
    async fn via_admin_preview(&self, device: &str) -> ExpectedParams {
        let r = self
            .app
            .get_with_headers(
                &format!("/api/admin/devices/{device}/preview?force=1&resolution=1"),
                &[AUTH],
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text());
        assert!(r.is_png());
        assert_eq!(
            r.headers.get("x-byonk-preview").unwrap(),
            "miss",
            "?force must re-render, or the header describes an older resolution"
        );
        from_report(&resolution_header(&r))
    }
}

#[tokio::test]
async fn all_five_render_paths_resolve_identically() {
    let fx = Fixture::new();

    for (case, device, want) in [
        (
            "final.dither vetoes the script",
            DEVICE_VETO,
            expected_veto(),
        ),
        (
            "the screen outranks the device's default",
            DEVICE_DEFAULT,
            expected_screen_wins(),
        ),
    ] {
        let display = fx.via_api_display(device).await;
        let dev = fx.via_dev_render(device).await;
        let store = fx.via_screen_store(device).await;
        let cli = fx.via_cli(device).await;
        let admin = fx.via_admin_preview(device).await;

        for (name, got) in [
            ("/api/display", &display),
            ("POST /dev/api/render", &dev),
            ("ScreenStore::render", &store),
            ("CLI render", &cli),
            ("/api/admin/devices/{key}/preview", &admin),
        ] {
            assert_eq!(got, &want, "{name} disagrees with the cascade ({case})");
        }
    }
}

/// `refresh` is not a field of `RenderParams`, so the convergence comparison
/// above cannot reach it — and it is assembled into a `Ladder` in **two**
/// independently-written places: `resolve_render` in `src/api/resolution.rs`
/// (which the dev provenance report comes from) and `resolve_refresh_rate` in
/// `src/services/content_pipeline.rs` (which is what a real device is told).
/// They share the order constant, so precedence cannot drift, but the
/// assembly is duplicated, with two separately-authored `> 0` filters and a
/// hard-coded `over: None` on the `content_pipeline` side.
///
/// The fixture contests three rungs: the device's `final.refresh` (300) over
/// the script's `refresh_rate` (1200) over the device's base `refresh` (600),
/// with the screen's `meta.yaml` 1800 as the fallback below all three. Swap
/// any two of them in either assembly and the two answers part company.
#[tokio::test]
async fn the_dev_report_and_the_device_agree_on_refresh() {
    let fx = Fixture::new();

    let api_key = fx.app.register_device(DEVICE_VETO).await;
    let headers = common::fixtures::display_headers(DEVICE_VETO, &api_key);
    let display = fx
        .app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
        .await;
    assert_eq!(display.status, StatusCode::OK, "{}", display.text());
    let json: serde_json::Value = display.json();
    let served = json["refresh_rate"].as_u64().expect("a refresh rate");

    let body = format!(r#"{{"device":"{DEVICE_VETO}"}}"#);
    let dev = fx.app.post_json("/dev/api/render", &[AUTH], &body).await;
    assert!(dev.status.is_success(), "{} {}", dev.status, dev.text());
    let reported = resolution_header(&dev)["refresh"]["effective"]
        .as_u64()
        .expect("a refresh rate");

    assert_eq!(
        reported, served,
        "the dev provenance report says the device refreshes every {reported}s, \
         but /api/display tells the device {served}s"
    );
    assert_eq!(
        served, 300,
        "`final.refresh` is the owner's veto: it must beat the script's 1200 \
         and the device's base 600"
    );
}
