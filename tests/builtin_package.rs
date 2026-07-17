//! End-to-end check that the embedded `byonk-builtin` package resolves the
//! migrated built-in screens through the package loader.

use byonk::assets::AssetLoader;
use byonk::services::screen_repo_loader::ScreenRepoLoader;

#[test]
fn test_builtin_default_resolves() {
    let loader = std::sync::Arc::new(AssetLoader::new(None, None, None));
    let pl = ScreenRepoLoader::new(loader, Default::default());
    let r = pl
        .resolve("byonk-builtin/default")
        .expect("default screen resolves");
    assert!(!r.meta.title.is_empty(), "default screen has a title");
}

#[test]
fn test_builtin_list_all_includes_migrated_screens() {
    let loader = std::sync::Arc::new(AssetLoader::new(None, None, None));
    let pl = ScreenRepoLoader::new(loader, Default::default());

    let refs: Vec<String> = pl
        .list_all()
        .into_iter()
        .map(|r| format!("{}/{}", r.handle, r.path))
        .collect();

    for expected in [
        "byonk-builtin/default",
        "byonk-builtin/useful/gphoto",
        "byonk-builtin/useful/swiss-departure-board",
        "byonk-builtin/calibration/color",
        "byonk-builtin/demo/font/bitmap",
        "byonk-builtin/example/hello",
        "byonk-builtin/example/webscrape",
    ] {
        assert!(
            refs.iter().any(|r| r == expected),
            "list_all() should include {expected}; got {refs:?}"
        );
    }
}
