//! Regression guard for the wiring fact `ScreenStore::new`'s doc comment
//! spells out: `AppState.screen_store` and `AppState.content_pipeline` must
//! resolve screens through the exact same `Arc<ScreenRepoManager>`.
//! `ScreenStore::render` resolves through the pipeline's manager while every
//! other `ScreenStore` method resolves through the manager handed to
//! `ScreenStore::new` directly. If production wiring (`server.rs`) ever
//! passed two different `ScreenRepoManager` instances, a registry change made
//! through one (e.g. an admin config write, which calls `reload_config` and
//! rebuilds `state.screen_repo_manager`'s loader) would silently not be
//! visible to the other — exactly the kind of bug a `ScreenStore`-only unit
//! test (constructed with hand-matched Arcs) can never catch, since it never
//! exercises the real `AppState` construction path or a live config reload.

use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::{create_app_state_with_config, reload_config};
use byonk::services::screen_store::RenderOpts;

/// Minimal screen fixture: manifest + one screen with no params.
fn write_screen_repo(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("hello")).unwrap();
    std::fs::write(
        dir.join("byonk-screens.yaml"),
        "name: testrepo\ndescription: Wiring test fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("hello/meta.yaml"),
        "title: Hello\ndescription: Wiring test screen.\nbyonk: \"0.15\"\nrefresh: 60\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("hello/script.lua"),
        "return { data = {}, refresh_rate = 60 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("hello/screen.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480"><rect width="800" height="480" fill="white"/></svg>"#,
    )
    .unwrap();
}

#[test]
fn screen_store_sees_config_reload_registered_repo_same_as_content_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");
    let repo_dir = dir.path().join("testrepo");
    write_screen_repo(&repo_dir);

    // Start with a config that does NOT register `testrepo` at all.
    std::fs::write(
        &config_path,
        "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n",
    )
    .unwrap();

    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path.clone())));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load initial config");
    let state = create_app_state_with_config(asset_loader, config).expect("create app state");

    // Sanity check: not yet registered, so render must fail to resolve.
    let before = state
        .screen_store
        .render("testrepo/hello", RenderOpts::default());
    assert!(
        before.error.is_some(),
        "testrepo/hello must not resolve before it's registered"
    );

    // Rewrite config.yaml to register `testrepo` as a writable `path:` repo,
    // then reload — this is exactly what an admin config write does
    // (src/api/admin/write.rs calls `reload_config`, which rebuilds
    // `state.screen_repo_manager`'s loader).
    let updated_yaml = format!(
        "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\nscreen_repos:\n  testrepo:\n    path: {}\n",
        repo_dir.display()
    );
    std::fs::write(&config_path, updated_yaml).unwrap();
    reload_config(&state).expect("reload_config");

    // `screen_store.render` resolves through `content_pipeline`'s manager
    // (see `ScreenStore::new`'s doc comment). This only succeeds if that
    // manager is the SAME `Arc<ScreenRepoManager>` `reload_config` just
    // rebuilt via `state.screen_repo_manager` — proving the two are not
    // independently-constructed instances.
    let after = state
        .screen_store
        .render("testrepo/hello", RenderOpts::default());
    assert!(
        after.error.is_none(),
        "testrepo/hello must resolve via screen_store.render after reload_config \
         registered it through state.screen_repo_manager: {:?}",
        after.error
    );
    assert!(!after.png.is_empty());

    // `screen_store`'s other methods (create/copy/rename/delete/validate/
    // read_file/write_file) resolve through `manager` directly, and
    // `state.screen_repo_manager` is that same manager by construction —
    // confirm the registry change is visible there too.
    assert!(
        state
            .screen_repo_manager
            .loader()
            .resolve("testrepo/hello")
            .is_some(),
        "state.screen_repo_manager must also resolve testrepo/hello"
    );
}
