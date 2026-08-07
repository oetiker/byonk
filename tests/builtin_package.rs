//! End-to-end check that the embedded `byonk-builtin` screen repo resolves the
//! minimal set of built-in screens (`default` + `calibration/*`) through the
//! screen repo loader — and nothing else. Example screens (`hello`,
//! `gphoto`, `swiss-departure-board`, ...) were split out into the shipped
//! `examples` embed (Task 10); they are not part of `byonk-builtin` anymore
//! and aren't resolvable at all until Task 11 registers the `examples`
//! screen repo handle.

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
fn test_builtin_list_all_is_exactly_default_and_calibration() {
    let loader = std::sync::Arc::new(AssetLoader::new(None, None, None));
    let pl = ScreenRepoLoader::new(loader, Default::default());

    let refs: Vec<String> = pl
        .list_all()
        .into_iter()
        .map(|r| format!("{}/{}", r.handle, r.path))
        .collect();

    for expected in [
        "byonk-builtin/default",
        "byonk-builtin/calibration/color",
        "byonk-builtin/calibration/gamut",
        "byonk-builtin/calibration/grey",
    ] {
        assert!(
            refs.iter().any(|r| r == expected),
            "list_all() should include {expected}; got {refs:?}"
        );
    }
    assert_eq!(
        refs.len(),
        4,
        "byonk-builtin must ship exactly default + calibration/{{color,gamut,grey}}, got {refs:?}"
    );

    // Example screens must not resolve through byonk-builtin anymore.
    for moved in [
        "byonk-builtin/example/hello",
        "byonk-builtin/useful/gphoto",
        "byonk-builtin/useful/swiss-departure-board",
        "byonk-builtin/demo/font/bitmap",
        "byonk-builtin/example/webscrape",
        "byonk-builtin/example/mandelbrot",
    ] {
        assert!(
            pl.resolve(moved).is_none(),
            "{moved} moved to the examples embed and must not resolve as byonk-builtin"
        );
    }
}
