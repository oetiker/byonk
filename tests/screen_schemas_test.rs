//! Every bundled screen's params (now declared in `meta.yaml`) must parse, and
//! known screens expose their documented params through the package loader.
//!
//! Screen params parsing itself is covered generically in
//! `src/models/screen_meta.rs`'s unit tests (fixture YAML, not tied to any
//! bundled screen). This file only exercises the wiring through
//! `ScreenRepoLoader` for screens that are still part of the minimal embedded
//! `byonk-builtin` repo after the builtin/examples split (Task 10) — `gphoto`,
//! `swiss-departure-board`, `hello`, `mandelbrot`, and the font demos moved to
//! the `examples` embed, which isn't registered as a screen repo handle until
//! Task 11, so they are no longer resolvable here.

use byonk::assets::AssetLoader;
use byonk::services::screen_repo_loader::ScreenRepoLoader;
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
        3,
        "expected exactly the 3 minimal builtin screens (default + calibration/color + calibration/grey) to resolve, got {}",
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
