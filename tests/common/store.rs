//! Shared fixture: a `ScreenStore` over a temp writable `local` screen repo.
//!
//! Built through the real `AppState` path so the store and the content
//! pipeline share one `ScreenRepoManager` — the invariant
//! `tests/screen_store_wiring_test.rs` guards. Constructing a `ScreenStore`
//! directly with hand-matched `Arc`s would not exercise that path.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::content_pipeline::ContentPipeline;
use byonk::services::renderer::RenderService;
use byonk::services::screen_repo_cache::ScreenRepoCache;
use byonk::services::screen_repo_manager::ScreenRepoManager;
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

/// A `ScreenStore` whose `local` handle resolves to a **read-only**,
/// disk-backed source — the same `DiskSource::Git`/`GitScreenRepoSource`
/// shape a fetched `screen_repos: { local: { repo: … } }` entry produces
/// (`RESERVED_HANDLES` is enforced only in `addon_options.rs`, not by
/// `ScreenRepoManager` itself, so nothing stops a `local` handle from
/// resolving to a read-only source in practice). Built via the manager's
/// `extra_disk` parameter rather than an actual git fetch — same
/// `DiskSource::Git` code path and resulting `GitScreenRepoSource`, without
/// needing a real repo/network access in a test.
pub fn build_store_with_readonly_local(dir: &Path) -> Arc<ScreenStore> {
    let repo_dir = dir.join("local");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: local\ndescription: Read-only fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();

    let asset_loader = Arc::new(AssetLoader::new(None, None, None));
    let config: Arc<arc_swap::ArcSwap<AppConfig>> =
        Arc::new(arc_swap::ArcSwap::from_pointee(AppConfig::default()));
    let mut extra_disk = HashMap::new();
    extra_disk.insert("local".to_string(), repo_dir);
    let manager = ScreenRepoManager::new(
        asset_loader.clone(),
        config.clone(),
        ScreenRepoCache::new(dir.join("cache")),
        extra_disk,
        None,
        None,
    );
    let renderer = Arc::new(RenderService::new(&asset_loader).unwrap());
    let pipeline =
        Arc::new(ContentPipeline::new(config, asset_loader, renderer, manager.clone()).unwrap());
    Arc::new(ScreenStore::new(manager, pipeline))
}
