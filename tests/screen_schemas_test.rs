//! Every bundled screen's params (now declared in `meta.yaml`) must parse, and
//! known screens expose their documented params through the package loader.
//!
//! Screen params parsing itself is covered generically in
//! `src/models/screen_meta.rs`'s unit tests (fixture YAML, not tied to any
//! bundled screen). This file exercises two things:
//!
//! - The `ScreenRepoLoader` resolution wiring, for screens that are still
//!   part of the minimal embedded `byonk-builtin` repo after the
//!   builtin/examples split (Task 10).
//! - `meta.yaml` parsing for the screens that moved to the `examples` embed
//!   (`gphoto`, `swiss-departure-board`, the font demos, ...). Those aren't
//!   resolvable through a screen repo handle until Task 11 registers
//!   `examples`, but their embedded `meta.yaml` bytes are readable right now
//!   via `AssetLoader::read_example`/`list_examples`, so their params get
//!   parsed and asserted directly — no repo handle is needed just to parse
//!   an embedded manifest.

use byonk::assets::AssetLoader;
use byonk::models::compat::{compat_warning, engine_version};
use byonk::models::screen_meta::ScreenMeta;
use byonk::services::screen_repo_loader::ScreenRepoLoader;
use std::path::Path;
use std::sync::Arc;

fn loader() -> ScreenRepoLoader {
    ScreenRepoLoader::new(
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

/// Parse an embedded example screen's `meta.yaml` directly (no screen repo
/// handle required — `examples` isn't registered until Task 11).
fn example_meta(screen_dir: &str) -> ScreenMeta {
    let loader = AssetLoader::new(None, None, None);
    let raw = loader
        .read_example(Path::new(&format!("{screen_dir}/meta.yaml")))
        .unwrap_or_else(|e| panic!("{screen_dir}/meta.yaml reads: {e}"));
    ScreenMeta::from_yaml(std::str::from_utf8(&raw).unwrap())
        .unwrap_or_else(|e| panic!("{screen_dir}/meta.yaml parses: {e}"))
}

#[test]
fn test_transit_params() {
    let meta = example_meta("swiss-departure-board");
    let names: Vec<String> = meta.params.fields.iter().map(|f| f.name.clone()).collect();
    assert!(names.contains(&"station".to_string()));
    assert!(names.contains(&"limit".to_string()));
}

#[test]
fn test_gphoto_params() {
    let meta = example_meta("gphoto");
    let names: Vec<String> = meta.params.fields.iter().map(|f| f.name.clone()).collect();
    assert!(names.contains(&"album_url".to_string()));
}

#[test]
fn test_fontdemo_bitmap_is_enum() {
    let meta = example_meta("demo/font/bitmap");
    let f = meta
        .params
        .fields
        .iter()
        .find(|f| f.name == "font_prefix")
        .expect("font_prefix param");
    assert!(!f.options.is_empty());
}

#[test]
fn test_all_examples_have_parseable_meta() {
    // Every `*/meta.yaml` reachable from `list_examples()` must parse — a
    // malformed or param-less example `meta.yaml` should fail loudly here
    // rather than shipping silently (examples aren't resolvable through a
    // screen repo handle until Task 11, so nothing else exercises this).
    let loader = AssetLoader::new(None, None, None);
    let meta_paths: Vec<String> = loader
        .list_examples()
        .into_iter()
        .filter(|p| p.ends_with("/meta.yaml"))
        .collect();
    assert!(
        !meta_paths.is_empty(),
        "expected at least one example meta.yaml, got none"
    );
    for path in &meta_paths {
        let raw = loader
            .read_example(Path::new(path))
            .unwrap_or_else(|e| panic!("{path} reads: {e}"));
        let meta = ScreenMeta::from_yaml(std::str::from_utf8(&raw).unwrap())
            .unwrap_or_else(|e| panic!("{path} parses: {e}"));
        assert!(!meta.title.is_empty(), "{path} has a title");
    }
}

#[test]
fn test_no_param_screens_have_empty_schema() {
    for screen_ref in ["byonk-builtin/default", "byonk-builtin/calibration/grey"] {
        assert!(
            param_names(screen_ref).is_empty(),
            "{screen_ref} should have no params"
        );
    }
}

#[test]
fn test_all_bundled_screens_have_parseable_meta() {
    // Resolving every screen forces its meta.yaml (title/description/byonk/params)
    // to parse; a bad meta would make list_all drop it, so assert the exact,
    // small, closed set the minimal byonk-builtin repo ships.
    let pl = loader();
    let all = pl.list_all();
    assert_eq!(
        all.len(),
        5,
        "expected exactly the 5 minimal builtin screens (default + calibration/{{color,gamut,grey,tone}}) to resolve, got {}",
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

/// No screen byonk itself ships may declare an engine requirement the running
/// engine doesn't satisfy — otherwise `GET /api/admin/screens` reports a
/// `compat_warning` on every one of them (including `byonk-builtin/default`,
/// the fallback screen), which trains users to ignore the field entirely.
///
/// This is the guard against the exact drift that shipped in 0.16.0: every
/// `meta.yaml` said `byonk: "0.15"` (= `^0.15`, i.e. `<0.16.0`) while the
/// crate had moved on. Non-vacuous — revert any shipped `meta.yaml` to
/// `"0.15"` and this fails.
#[test]
fn every_shipped_screen_is_compatible_with_this_engine() {
    let engine = engine_version();

    // Embedded `byonk-builtin`.
    for r in loader().list_all() {
        assert_eq!(
            compat_warning(engine, &r.meta.byonk),
            None,
            "{}/{} declares an incompatible engine requirement",
            r.handle,
            r.path
        );
    }

    // Embedded `examples` (seeded to disk as the `examples` repo at runtime;
    // read straight out of the embed here, as the tests above do).
    let asset_loader = AssetLoader::new(None, None, None);
    let meta_paths: Vec<String> = asset_loader
        .list_examples()
        .into_iter()
        .filter(|p| p.ends_with("/meta.yaml"))
        .collect();
    assert!(!meta_paths.is_empty());
    for path in &meta_paths {
        let raw = asset_loader.read_example(Path::new(path)).unwrap();
        let meta = ScreenMeta::from_yaml(std::str::from_utf8(&raw).unwrap()).unwrap();
        assert_eq!(
            compat_warning(engine, &meta.byonk),
            None,
            "{path} declares an incompatible engine requirement"
        );
    }
}
