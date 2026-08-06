//! Proves the image pipeline achieves its purpose: that `vibrance` measurably
//! increases the share of pixels landing on chromatic palette entries after
//! dithering. Every other test in this feature proves an operation behaves as
//! specified; this one proves the specification was worth implementing.
//!
//! Complements, rather than duplicates, the two existing test suites:
//! - `cargo test --lib services::image_process` (17 tests) pins
//!   decode/crop/resize/re-encode behaviour of the `image_process.rs` wrapper
//!   in isolation, without going through a screen or the eink-dither stage.
//! - `cargo test --test lua_api_test image_process` (25 tests) pins the Lua
//!   binding directly against `LuaRuntime::run_script_from_asset`, including
//!   `palette_aware`'s own internal endpoint-compression step — but never
//!   runs the result through `screen.svg` -> Tera -> eink-dither, i.e. never
//!   through the actual per-device dithering palette a real device gets.
//!
//! This file is the only one that renders a full screen end-to-end
//! (`ScreenStore::render`) and inspects the *final dithered device PNG*,
//! proving `vibrance` (set in Lua script options, consumed by
//! `crates/eink-photo`) survives all three layers and changes what a real
//! e-ink panel would show.

mod common;

use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::screen_store::{RenderOpts, ScreenStore};

/// Count how many pixels of a dithered PNG land on a non-grey palette entry.
fn chromatic_share(png: &[u8]) -> f64 {
    let img = image::load_from_memory(png).expect("valid png").to_rgb8();
    let total = img.pixels().len() as f64;
    let chromatic = img
        .pixels()
        .filter(|p| {
            let [r, g, b] = p.0;
            let max = r.max(g).max(b) as i32;
            let min = r.min(g).min(b) as i32;
            // A palette entry is chromatic when its channels disagree.
            max - min > 24
        })
        .count() as f64;
    chromatic / total
}

/// A muted, low-saturation photograph — the case this feature exists for.
///
/// The brief's original tonal range (base ~90-146, tinted +10/-8 around a
/// dark-to-mid grey) turned out to already dither into ~82% chromatic
/// pixels at `vibrance = 0` against the `trmnl_og_4clr` panel used below,
/// regardless of vibrance: this panel's palette has no *grey* ink at all
/// (only pure black/white/red/yellow), and a pure red swatch's OKLab
/// lightness sits close to a dark-to-mid grey's — closer than either black
/// or white — so error diffusion picks red for most of that range on its
/// own. Shifting the source to a near-white, faintly yellow-tinted range
/// (base ~205-237, blue channel -10) keeps the baseline mostly on `white`
/// (whose lightness is the near match without help), while `vibrance`'s
/// saturation boost is what tips increasingly many of those near-white
/// pixels over to `yellow` instead — measured empirically until the
/// dull/vivid gap was driven by vibrance rather than by an unlucky palette
/// distance quirk. See the mutation notes in `task-10-report.md` for the
/// numbers.
fn muted_photo(w: u32, h: u32) -> Vec<u8> {
    let mut img = image::RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        // Gentle tonal variation with only a whisper of colour: every pixel
        // sits close to a near-white grey, so little reaches a chromatic
        // palette entry without vibrance's help.
        let base = 205 + ((x / 8 + y / 8) % 5) as u8 * 8;
        px.0 = [base, base, base.saturating_sub(10)];
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// Like `common::store::build_store`, but the fixture's `config.yaml` also
/// declares a `panels:` entry with chromatic official colors — a real
/// 4-color panel profile lifted from this repo's own `config.yaml`
/// (`trmnl_og_4clr`).
///
/// This is needed because `RenderOpts` has no `colors` field: the render
/// palette comes from `resolve_query_palette(&opts.model, None)`, which for
/// every model except `"x"` resolves to the 4-*grey* `DEFAULT_COLORS`
/// constant (see `src/api/display.rs`) — chromatic-free. The only way to get
/// a chromatic entry into the palette actually used for dithering is
/// `RenderOpts::panel`, resolved against the `panels:` config section
/// (`ScreenStore::render`'s `panel_colors` -> `resolve_render_params`). So
/// this helper, not `common::store::build_store`, is what makes the test
/// meaningful at all.
fn build_store_with_color_panel(dir: &std::path::Path) -> Arc<ScreenStore> {
    let config_path = dir.join("config.yaml");
    let repo_dir = dir.join("local");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: local\ndescription: Test fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        format!(
            "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n\
             screen_repos:\n  local:\n    path: {}\n\
             panels:\n  color:\n    name: \"Test 4-color panel\"\n\
             \x20   colors: \"#000000,#FFFFFF,#FF0000,#FFFF00\"\n",
            repo_dir.display()
        ),
    )
    .unwrap();

    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    assert!(
        config.get_panel("color").is_some(),
        "fixture config.yaml must parse a 'color' panel"
    );
    let state = create_app_state_with_config(asset_loader, config).expect("create app state");
    state.screen_store.clone()
}

/// Write a screen into the fixture's writable `local` repo whose script
/// embeds `photo.png` after running it through `image_process` with the
/// given vibrance.
fn write_photo_screen(dir: &std::path::Path, name: &str, vibrance: f32) {
    let screen_dir = dir.join("local").join(name);
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(screen_dir.join("photo.png"), muted_photo(200, 120)).unwrap();
    // `ScreenMeta` requires `title`/`description`/`byonk` (no defaults) —
    // the brief's draft used `name:`, which `ScreenMeta::from_yaml` rejects,
    // so `resolve()` returns `None` and every render fails with "Screen
    // '...' not found". Use the same required-field shape
    // `StarterKind::Minimal`'s scaffolded `meta.yaml` uses (see
    // `starter_meta()` in `src/services/screen_store.rs`).
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!(
            "title: {name}\ndescription: photo test\nbyonk: \"{}\"\nrefresh: 3600\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("script.lua"),
        format!(
            r#"
            local photo = read_asset("photo.png")
            local src, w, h = image_process(photo, {{
                width = 200, height = 120, fit = "stretch",
                vibrance = {vibrance},
            }})
            return {{ data = {{ src = src, w = w, h = h }}, refresh_rate = 3600 }}
            "#
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{{ device.width }}" height="{{ device.height }}">
  <image x="0" y="0" width="{{ data.w }}" height="{{ data.h }}" href="{{ data.src }}"/>
</svg>"#,
    )
    .unwrap();
}

#[test]
fn vibrance_increases_the_chromatic_share_of_the_dithered_output() {
    // Without vibrance, muted colours never reach a chromatic palette entry
    // and the whole image dithers into greys — the exact failure this
    // feature exists to fix.
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store_with_color_panel(tmp.path());
    write_photo_screen(tmp.path(), "dull", 0.0);
    write_photo_screen(tmp.path(), "vivid", 80.0);

    // The render palette must contain chromatic entries — see
    // `build_store_with_color_panel`'s doc comment for why `panel` is
    // required to make that true.
    let opts = || RenderOpts {
        model: "og".to_string(),
        width: Some(200),
        height: Some(120),
        panel: Some("color".to_string()),
        timestamp: Some(1_750_000_000),
        ..Default::default()
    };

    let render = |name: &str| {
        let r = store.render(&format!("local/{name}"), opts());
        assert!(
            r.error.is_none(),
            "{name} must render: {:?} / {:?}",
            r.error,
            r.log
        );
        r.png
    };

    let dull = chromatic_share(&render("dull"));
    let vivid = chromatic_share(&render("vivid"));

    // Guard against the whole test silently measuring nothing.
    assert!(
        vivid > 0.0,
        "the render palette has no chromatic entries; this test proves nothing"
    );

    assert!(
        vivid > dull * 1.2,
        "vibrance must push colour onto the palette: {dull} -> {vivid}"
    );
}

/// Deliberately break the premise: render the same "vivid" screen at
/// `vibrance = 0.0` twice (under different screen names, since `render`
/// resolves screens by name). With vibrance neutralized on both sides the
/// pipeline is deterministic (no RNG anywhere in `crates/eink-photo` or
/// `src/services/image_process.rs`), so the two chromatic shares must be
/// equal, and the `1.2`-ratio assertion must fail. This is the mutation
/// check the brief requires before trusting the assertion above: it proves
/// the test can fail, and that it fails for the right reason (vibrance is
/// the only thing distinguishing "dull" from "vivid" — collapse that
/// distinction and the test collapses with it).
#[test]
fn broken_premise_both_screens_at_zero_vibrance_do_not_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store_with_color_panel(tmp.path());
    write_photo_screen(tmp.path(), "dull", 0.0);
    write_photo_screen(tmp.path(), "vivid", 0.0);

    let opts = || RenderOpts {
        model: "og".to_string(),
        width: Some(200),
        height: Some(120),
        panel: Some("color".to_string()),
        timestamp: Some(1_750_000_000),
        ..Default::default()
    };

    let render = |name: &str| {
        let r = store.render(&format!("local/{name}"), opts());
        assert!(r.error.is_none(), "{name} must render: {:?}", r.error);
        r.png
    };

    let dull = chromatic_share(&render("dull"));
    let vivid = chromatic_share(&render("vivid"));

    assert!(
        (dull - vivid).abs() < f64::EPSILON,
        "with vibrance neutralized on both sides the shares must be identical: {dull} vs {vivid}"
    );
    assert!(
        !(vivid > dull * 1.2),
        "the real assertion must NOT hold when vibrance is neutralized on both sides: {dull} -> {vivid}"
    );
}
