//! Every bundled screen's params (now declared in `meta.yaml`) must parse, and
//! known screens expose their documented params through the package loader.

use byonk::assets::AssetLoader;
use byonk::services::package_loader::PackageLoader;
use std::sync::Arc;

fn loader() -> PackageLoader {
    PackageLoader::new(
        Arc::new(AssetLoader::new(None, None, None)),
        Default::default(),
    )
}

fn param_names(screen_ref: &str) -> Vec<String> {
    let pl = loader();
    let resolved = pl
        .resolve(screen_ref)
        .unwrap_or_else(|| panic!("{screen_ref} resolves"));
    resolved
        .meta
        .params
        .fields
        .iter()
        .map(|f| f.name.clone())
        .collect()
}

#[test]
fn test_transit_params() {
    let names = param_names("byonk-builtin/useful/swiss-departure-board");
    assert!(names.contains(&"station".to_string()));
    assert!(names.contains(&"limit".to_string()));
}

#[test]
fn test_gphoto_params() {
    let names = param_names("byonk-builtin/useful/gphoto");
    assert!(names.contains(&"album_url".to_string()));
}

#[test]
fn test_fontdemo_bitmap_is_enum() {
    let pl = loader();
    let resolved = pl
        .resolve("byonk-builtin/demo/font/bitmap")
        .expect("bitmap font demo resolves");
    let f = resolved
        .meta
        .params
        .fields
        .iter()
        .find(|f| f.name == "font_prefix")
        .expect("font_prefix param");
    assert!(!f.options.is_empty());
}

#[test]
fn test_no_param_screens_have_empty_schema() {
    for screen_ref in [
        "byonk-builtin/default",
        "byonk-builtin/calibration/grey",
        "byonk-builtin/example/hello",
        "byonk-builtin/example/mandelbrot",
    ] {
        assert!(
            param_names(screen_ref).is_empty(),
            "{screen_ref} should have no params"
        );
    }
}

#[test]
fn test_all_bundled_screens_have_parseable_meta() {
    // Resolving every screen forces its meta.yaml (title/description/byonk/params)
    // to parse; a bad meta would make list_all drop it, so assert a sane count too.
    let pl = loader();
    let all = pl.list_all();
    assert!(
        all.len() >= 11,
        "expected all 11 migrated builtin screens to resolve, got {}",
        all.len()
    );
    for r in &all {
        assert!(
            !r.meta.title.is_empty(),
            "{}/{} has a title",
            r.handle,
            r.path
        );
    }
}
