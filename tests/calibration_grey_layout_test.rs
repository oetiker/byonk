//! The builtin grey calibration screen has to stay legible as the palette grows.
//!
//! Found by rendering it on the 16-grey panel for the first time. The screen
//! laid every swatch out in one row, which gives each `#RRGGBB` label a
//! sixteenth of the panel width — roughly a third of what the text needs — so
//! the labels collided into an unreadable smear, the registration circles
//! overlapped each other, and the leftmost one hung off the edge of the
//! screen. It is the screen an operator reaches for to calibrate exactly that
//! panel.
//!
//! Asserting on pixels would not catch this: the render is structurally valid
//! either way, and "is this text legible" is not a question a PNG comparison
//! answers. So these read back the geometry the script itself computed, which
//! is the same geometry `screen.svg` places every swatch, label and mark with.

use std::path::Path;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::screen_store::{RenderOpts, RenderResult, ScreenStore};

/// Two probe panels differing only in how many entries their palette has, so
/// a failure points at the palette size and nothing else. The 16-entry ramp
/// is `trmnl_x`'s; the 4-entry one is `trmnl_og`'s.
const CONFIG: &str = r##"
devices:
  DEFAULT:
    screen: byonk-builtin/default
panels:
  probe16:
    name: "Probe 16-grey"
    width: 1872
    height: 1404
    colors: "#000000,#111111,#222222,#333333,#444444,#555555,#666666,#777777,#888888,#999999,#AAAAAA,#BBBBBB,#CCCCCC,#DDDDDD,#EEEEEE,#FFFFFF"
  probe4:
    name: "Probe 4-grey"
    width: 800
    height: 480
    colors: "#000000,#555555,#AAAAAA,#FFFFFF"
"##;

fn store(dir: &Path) -> Arc<ScreenStore> {
    let config_path = dir.join("config.yaml");
    std::fs::write(&config_path, CONFIG).unwrap();
    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    let state = create_app_state_with_config(asset_loader, config).expect("create app state");
    state.screen_store.clone()
}

fn render_on(dir: &Path, panel: &str, w: u32, h: u32) -> RenderResult {
    let out = store(dir).render(
        "byonk-builtin/calibration/grey",
        RenderOpts {
            panel: Some(panel.to_string()),
            width: Some(w),
            height: Some(h),
            ..RenderOpts::default()
        },
    );
    assert!(out.error.is_none(), "render failed: {:?}", out.error);
    out
}

/// One swatch's geometry, as the script handed it to the template.
struct Swatch {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    center_x: f64,
    label_y: f64,
    circle_y: f64,
    label: String,
}

fn swatches(out: &RenderResult) -> (Vec<Swatch>, f64, f64) {
    let num = |v: &serde_json::Value, k: &str| -> f64 {
        v.get(k)
            .and_then(|n| n.as_f64())
            .unwrap_or_else(|| panic!("swatch geometry is missing `{k}`: {v}"))
    };
    let bars = out.data["bars"]
        .as_array()
        .expect("the screen must return a `bars` array")
        .iter()
        .map(|b| Swatch {
            x: num(b, "x"),
            y: num(b, "y"),
            w: num(b, "width"),
            h: num(b, "height"),
            center_x: num(b, "center_x"),
            label_y: num(b, "label_y"),
            circle_y: num(b, "circle_y"),
            label: b["label"].as_str().unwrap_or_default().to_string(),
        })
        .collect();
    let font = out.data["font_label"].as_f64().expect("font_label");
    let circle_r = out.data["circle_r"].as_f64().expect("circle_r");
    (bars, font, circle_r)
}

/// Outfit is proportional, so this is an estimate rather than a measurement —
/// but a deliberately *generous* one. Real advances for `#0123456789ABCDEF`
/// run above 0.55em, so a label that fails this check overflows its column by
/// more than the slack, and one that passes has room to spare. The original
/// single-row layout misses it by 55%, which is far outside the error bar.
const EM_PER_CHAR: f64 = 0.55;

/// The case that broke: sixteen `#RRGGBB` labels cannot share one row.
#[test]
fn every_swatch_label_fits_inside_its_own_column_at_16_levels() {
    let dir = tempfile::tempdir().unwrap();
    let out = render_on(dir.path(), "probe16", 1872, 1404);
    let (bars, font, _) = swatches(&out);

    assert_eq!(bars.len(), 16, "one swatch per palette entry");

    for b in &bars {
        let est = b.label.chars().count() as f64 * font * EM_PER_CHAR;
        assert!(
            est <= b.w,
            "label {:?} needs about {:.0}px but its column is only {:.0}px wide — \
             the swatches are back on a single row and the labels will collide",
            b.label,
            est,
            b.w
        );
    }
}

/// The grid has to actually be a grid, and it has to tile: every swatch inside
/// the panel, and no two overlapping. Without this, "the label fits" could be
/// satisfied by simply shrinking the columns past each other.
#[test]
fn the_16_level_swatches_tile_the_panel_without_overlapping() {
    let dir = tempfile::tempdir().unwrap();
    let out = render_on(dir.path(), "probe16", 1872, 1404);
    let (bars, _, _) = swatches(&out);

    let rows: std::collections::BTreeSet<i64> = bars.iter().map(|b| b.y as i64).collect();
    assert!(
        rows.len() > 1,
        "16 entries must wrap onto multiple rows, got {} row(s)",
        rows.len()
    );

    for b in &bars {
        assert!(
            b.x >= 0.0 && b.x + b.w <= 1872.0,
            "swatch {:?} runs off the panel horizontally: {}..{}",
            b.label,
            b.x,
            b.x + b.w
        );
        assert!(b.y >= 0.0, "swatch {:?} starts above the panel", b.label);
    }

    for (i, a) in bars.iter().enumerate() {
        for b in bars.iter().skip(i + 1) {
            let overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
            assert!(!overlap, "swatches {:?} and {:?} overlap", a.label, b.label);
        }
    }
}

/// The label and the registration mark share the cell, and both used to be
/// placed as a fraction of the *panel*. In a grid the cell is a fraction of
/// that, so the first row's label was clipped by the top of the screen and the
/// circles were wider than their cell.
#[test]
fn label_and_registration_mark_stay_inside_their_cell_at_16_levels() {
    let dir = tempfile::tempdir().unwrap();
    let out = render_on(dir.path(), "probe16", 1872, 1404);
    let (bars, font, r) = swatches(&out);

    for b in &bars {
        assert!(
            b.label_y - b.y >= font,
            "label {:?} sits {:.0}px below its cell top but the type is {:.0}px — \
             its cap height is clipped",
            b.label,
            b.label_y - b.y,
            font
        );
        assert!(
            b.circle_y - r >= b.label_y,
            "the mark on {:?} overlaps the label baseline",
            b.label
        );
        assert!(
            b.circle_y + r <= b.y + b.h,
            "the mark on {:?} runs out the bottom of its cell",
            b.label
        );
        assert!(
            b.center_x - r >= b.x && b.center_x + r <= b.x + b.w,
            "the mark on {:?} is wider than its cell",
            b.label
        );
    }
}

/// The control. Four entries already fit one row, and wrapping them would be a
/// regression on every panel byonk ships a 4-colour profile for — which is
/// most of them. Without this, "wrap when crowded" could quietly become
/// "always wrap".
#[test]
fn a_four_entry_palette_still_gets_a_single_row() {
    let dir = tempfile::tempdir().unwrap();
    let out = render_on(dir.path(), "probe4", 800, 480);
    let (bars, font, _) = swatches(&out);

    assert_eq!(bars.len(), 4);
    let rows: std::collections::BTreeSet<i64> = bars.iter().map(|b| b.y as i64).collect();
    assert_eq!(
        rows.len(),
        1,
        "4 entries fit one row and must keep it, got {} rows",
        rows.len()
    );
    for b in &bars {
        let est = b.label.chars().count() as f64 * font * EM_PER_CHAR;
        assert!(est <= b.w, "label {:?} overflows even on one row", b.label);
    }
}
