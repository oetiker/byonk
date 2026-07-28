//! Shared fixture: a `ScreenStore` over a temp writable `local` screen repo.
//!
//! Built through the real `AppState` path so the store and the content
//! pipeline share one `ScreenRepoManager` — the invariant
//! `tests/screen_store_wiring_test.rs` guards. Constructing a `ScreenStore`
//! directly with hand-matched `Arc`s would not exercise that path.

use std::path::Path;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::screen_store::{ScreenStore, StarterKind};

/// A `ScreenStore` whose `local` handle is a writable repo at `dir/local`,
/// pre-scaffolded with one minimal screen per entry in `screens`.
///
/// Callers needing specific file contents write them directly under
/// `dir/local/<screen>/` afterwards — the disk sources stat and read on every
/// access, so no loader rebuild is needed for an in-place file change.
pub fn build_store(dir: &Path, screens: &[&str]) -> Arc<ScreenStore> {
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
             screen_repos:\n  local:\n    path: {}\n",
            repo_dir.display()
        ),
    )
    .unwrap();

    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    let state = create_app_state_with_config(asset_loader, config).expect("create app state");
    let store = state.screen_store.clone();
    for name in screens {
        store
            .create_screen("local", name, StarterKind::Minimal)
            .unwrap_or_else(|e| panic!("scaffold fixture screen {name}: {e:?}"));
    }
    store
}
