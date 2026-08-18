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
//! e-ink panel would show. It also proves `colors_actual` (measured display
//! colors) reaches that same final PNG — a claim the palette_aware-focused
//! Lua-level tests don't make, since they never dither.
//!
//! What this file uniquely earns, precisely: not "vibrance changes
//! `image_process`'s output" (already pinned at the Lua-binding level by
//! `lua_api_test::test_image_process_vibrance_changes_the_output`), but
//! that the effect *survives dithering against a real device palette* —
//! i.e. that the palette resolved for the Lua script's `device.colors`
//! context is the same palette actually used to quantize the final PNG. A
//! regression where those two diverge (panel colors correctly threaded
//! upstream into the script context, but silently dropped before the final
//! `render_png_from_svg` call) is invisible to every other test in this
//! feature and is exactly what this file is built to catch.

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
///
/// Measured with this exact fixture (`dull` = vibrance 0, `vivid` =
/// vibrance 80, 24,000 px, `trmnl_og_4clr`-shaped palette): dull =
/// 0.240083 (4250 red + 1512 yellow), vivid = 0.303875 (4061 red + 3232
/// yellow) — ratio **1.2657**, a 5.5% margin over the `1.2` bar this test
/// asserts. That margin is not a flake risk (the pipeline has no RNG
/// anywhere in `crates/eink-photo`/`image_process.rs`, confirmed by
/// `render_is_deterministic_for_identical_inputs` below), but it IS brittle
/// to any future change in `eink-dither`'s nearest-palette matching — if
/// this test starts failing, re-sweep vibrance 0..100 on this fixture
/// before assuming the feature broke; a shifted margin, not a reversed
/// sign, points at the matcher instead.
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

/// Build a `ScreenStore` whose fixture `config.yaml` declares two `panels:`
/// entries with chromatic official colors — a real 4-color panel profile
/// lifted from this repo's own `config.yaml` (`trmnl_og_4clr`):
/// - `color`: `colors` only, no `colors_actual`.
/// - `color_measured`: the same `colors`, plus `colors_actual` (this repo's
///   real measured values for that panel) — exercises the
///   `measured_colors`/`use_actual` arguments `ScreenStore::render` passes
///   into `render_png_from_svg` (`screen_store.rs:1124-1129`) when a panel
///   has measured colors.
///
/// A panel is needed at all because `RenderOpts` has no `colors` field: the
/// render palette comes from `resolve_query_palette(&opts.model, None)`,
/// which for every model except `"x"` resolves to the 4-*grey*
/// `DEFAULT_COLORS` constant (see `src/api/display.rs`) — chromatic-free.
/// The only way to get a chromatic entry into the palette actually used for
/// dithering is `RenderOpts::panel`, resolved against the `panels:` config
/// section (`ScreenStore::render`'s `panel_colors` ->
/// `resolve_render_params`). There is no shared store-builder for this in
/// `tests/common/store.rs` (its `build_store` hardcodes a `panels:`-free
/// config), so this helper builds its own fixture.
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
             \x20   colors: \"#000000,#FFFFFF,#FF0000,#FFFF00\"\n\
             \x20 color_measured:\n    name: \"Test 4-color panel (measured)\"\n\
             \x20   colors: \"#000000,#FFFFFF,#FF0000,#FFFF00\"\n\
             \x20   colors_actual: \"#383038,#B8B8B0,#9C484B,#D0BE47\"\n",
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
    assert!(
        config.get_panel("color_measured").is_some()
            && config
                .get_panel("color_measured")
                .unwrap()
                .colors_actual
                .is_some(),
        "fixture config.yaml must parse a 'color_measured' panel with colors_actual"
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

/// What this test actually pins: end-to-end determinism for identical
/// inputs, and that the screen *name* alone (`"dull"` vs `"vivid"`) does not
/// leak into the render. Render the same script (`vibrance = 0.0`) under two
/// different screen names and require identical chromatic shares.
///
/// This is narrower than its former name (`broken_premise_...`) implied: it
/// was checked against all three mutations applied for this task (vibrance
/// stage skipped, vibrance param dropped in the Lua binding, panel palette
/// bypassed downstream) and passed under every one of them — a regression in
/// vibrance itself cannot fail this test, only a name-dependent or
/// nondeterministic render can. What it *does* guard is real: it is the
/// reason the 5.5% margin on `vivid > dull * 1.2` above is a safe assertion
/// rather than a flaky one — if identical inputs ever stopped producing
/// identical outputs, that margin would be meaningless. (The brief's
/// deliberate-break check — setting `vibrance` to `0.0` in both runs of the
/// *primary* test and confirming the `1.2` assertion fails — was performed
/// manually during development, not left as a permanent test; see
/// `task-10-report.md`'s mutation section for that evidence instead.)
#[test]
fn render_is_deterministic_for_identical_inputs() {
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
        "with vibrance neutralized on both sides and only the screen name \
         differing, the shares must be identical: {dull} vs {vivid}"
    );
}

/// Proves `colors_actual` (measured display colors) changes the final
/// dithered device PNG, not just the pre-dither `image_process` output the
/// existing 25 Lua-level tests already pin for `palette_aware`. Same script,
/// same `vibrance`, same official `colors` — the only difference between
/// the two renders is whether the panel also carries `colors_actual`.
///
/// This closes a gap the initial version of this file left open: measured
/// colors steer palette-index selection inside the very same
/// `render_png_from_svg` call this file's other tests already exercise
/// (`measured_colors` + `use_actual` args, `screen_store.rs:1124-1129`), and
/// a sibling in-flight branch makes measured colors flow end-to-end for the
/// first time — this is the interaction most likely to break at that merge,
/// and this is the only test positioned to catch it.
/// Write a screen whose script sets `palette_aware` explicitly, with
/// vibrance pinned to 0 so the only variable is the palette the tone
/// mapper's output endpoints are derived from.
fn write_palette_aware_photo_screen(dir: &std::path::Path, name: &str, palette_aware: bool) {
    write_photo_screen(dir, name, 0.0);
    std::fs::write(
        dir.join("local").join(name).join("script.lua"),
        format!(
            r#"
            local photo = read_asset("photo.png")
            local src, w, h = image_process(photo, {{
                width = 200, height = 120, fit = "stretch",
                vibrance = 0,
                palette_aware = {palette_aware},
            }})
            return {{ data = {{ src = src, w = w, h = h }}, refresh_rate = 3600 }}
            "#
        ),
    )
    .unwrap();
}

/// The seam between measured colors and `palette_aware`: proves a panel's
/// `colors_actual` is what `palette_aware` derives its output endpoints
/// from, all the way from the `panels:` config section to the final
/// dithered PNG.
///
/// Neither existing suite covers this pairing. The Lua-level
/// `test_image_process_palette_aware_with_a_palette_actually_uses_it`
/// hands `LuaRuntime` a hand-built `DeviceContext` with `colors_actual`
/// already populated, so it cannot see whether anything upstream actually
/// populates it; `measured_colors_change_the_final_dithered_output` above
/// proves measured colors reach the *ditherer*, which is a different
/// consumer entirely — that path would keep working with `palette_aware`
/// wholly broken.
///
/// A bare "measured panel differs from official panel" assertion would be
/// confounded by exactly that: the two already differ for dithering
/// reasons alone. So this is a 2x2, and the control arm carries the
/// argument.
///
/// The fixture's `color` panel is `#000000..#FFFFFF`, whose endpoints are
/// *exactly* the `(0.0, 1.0)` that `eink_photo`'s tone mapper already
/// defaults to when `output_endpoints` is `None` (`palette_endpoints` in
/// `crates/eink-photo/src/preset.rs`, `unwrap_or((0.0, 1.0))` in
/// `lib.rs`). So on that panel `palette_aware` is a provable no-op —
/// byte-identical output — while on `color_measured` the measured range is
/// compressed well inside 0..1 and the tone curve must visibly change.
///
/// That asymmetry is what makes this mutation-resistant. The palette the
/// Lua binding hands to `palette_aware` is
/// `colors_actual.or(colors)` (`lua_runtime.rs`, captured pre-script in
/// `setup_globals`). Break any hop that carries `colors_actual` from the
/// panel config into `DeviceContext` and the fallback is that same
/// `#000000..#FFFFFF` spec palette — endpoints `(0.0, 1.0)`, no-op,
/// measured arm collapses onto its control and the second assertion
/// fails.
#[test]
fn palette_aware_derives_its_endpoints_from_a_panels_measured_colors() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store_with_color_panel(tmp.path());
    write_palette_aware_photo_screen(tmp.path(), "plain", false);
    write_palette_aware_photo_screen(tmp.path(), "aware", true);

    // Compare a compact fingerprint rather than the PNG itself: a failed
    // byte-vector comparison dumps two ~4KB pixel dumps into the test
    // output, which buries the message that explains the failure.
    let render = |screen: &str, panel: &str| -> String {
        let r = store.render(
            &format!("local/{screen}"),
            RenderOpts {
                model: "og".to_string(),
                width: Some(200),
                height: Some(120),
                panel: Some(panel.to_string()),
                timestamp: Some(1_750_000_000),
                ..Default::default()
            },
        );
        assert!(
            r.error.is_none(),
            "{screen} on {panel} must render: {:?}",
            r.error
        );
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        r.png.hash(&mut h);
        format!("{} bytes, digest {:016x}", r.png.len(), h.finish())
    };

    // Control: on a panel whose spec palette spans the full 0..1 tone
    // range and which has no measured colors, palette_aware has nothing
    // to compress and must not change a single pixel.
    assert_eq!(
        render("plain", "color"),
        render("aware", "color"),
        "on a #000000..#FFFFFF panel with no colors_actual, palette_aware's \
         endpoints are the (0.0, 1.0) default — it must be a no-op. If this \
         fails, the assertion below no longer isolates measured colors."
    );

    // The seam: the same script, the same spec palette, the only
    // difference being that this panel also declares colors_actual.
    assert_ne!(
        render("plain", "color_measured"),
        render("aware", "color_measured"),
        "palette_aware must compress the tone curve into the panel's \
         measured range — so a panel carrying colors_actual must render \
         differently with palette_aware on than off. Identical output means \
         colors_actual never reached the Lua image_process binding and it \
         fell back to the spec palette."
    );
}

#[test]
fn measured_colors_change_the_final_dithered_output() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store_with_color_panel(tmp.path());
    write_photo_screen(tmp.path(), "photo", 40.0);

    let opts = |panel: &str| RenderOpts {
        model: "og".to_string(),
        width: Some(200),
        height: Some(120),
        panel: Some(panel.to_string()),
        timestamp: Some(1_750_000_000),
        ..Default::default()
    };

    let render = |panel: &str| {
        let r = store.render("local/photo", opts(panel));
        assert!(r.error.is_none(), "{panel} must render: {:?}", r.error);
        r.png
    };

    let official_only = render("color");
    let measured = render("color_measured");

    assert_ne!(
        official_only, measured,
        "a panel with colors_actual set must dither differently from the \
         same panel without it — measured colors must reach the final PNG"
    );
}
