//! HTTP server setup and configuration.
//!
//! This module provides the router and application state used by both
//! the production server and integration tests.

use arc_swap::ArcSwap;
use axum::{
    http::{header::CONNECTION, Request},
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    trace::{MakeSpan, TraceLayer},
};
use tracing::{Level, Span};

use crate::api;
use crate::assets::AssetLoader;
use crate::error::ApiError;
use crate::models::{config::ScreenRepoRef, AppConfig, DitherTuningValues};
use crate::services::screen_repo_cache::ScreenRepoCache;
use crate::services::screen_repo_manager::ScreenRepoManager;
use crate::services::{ContentCache, ContentPipeline, InMemoryRegistry, RenderService};

/// Runtime overrides set by the dev GUI and consumed by production handlers.
///
/// Both maps are keyed by **device config key** (the key from `config.devices`)
/// so that each device can be tuned independently, even when multiple devices
/// share the same panel profile.
#[derive(Clone, Default)]
pub struct DevOverrides {
    /// device_config_key → overridden `colors_actual` string (from color-tuning popup).
    pub panel_colors: Arc<RwLock<HashMap<String, String>>>,
    /// device_config_key → dither algorithm string (from dither dropdown).
    pub dither: Arc<RwLock<HashMap<String, String>>>,
    /// device_config_key → dither tuning values.
    pub tuning: Arc<RwLock<HashMap<String, DitherTuningValues>>>,
}

/// Custom span maker that adds a unique request ID to each request's tracing span.
#[derive(Clone, Copy)]
struct RequestIdSpan;

impl<B> MakeSpan<B> for RequestIdSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        // Generate a short unique request ID (first 8 chars of a random hex string)
        let request_id = format!("{:08x}", rand::random::<u32>());

        tracing::span!(
            Level::INFO,
            "request",
            request_id = %request_id,
            method = %request.method(),
            uri = %request.uri(),
        )
    }
}

/// Hot-swappable application config shared by the server and the content pipeline.
pub type SharedConfig = std::sync::Arc<ArcSwap<AppConfig>>;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: SharedConfig,
    pub asset_loader: Arc<AssetLoader>,
    pub admin_token: Option<String>,
    /// Serializes admin config writes so concurrent requests can't interleave file patches.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    pub registry: Arc<InMemoryRegistry>,
    pub renderer: Arc<RenderService>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
    pub dev_overrides: DevOverrides,
    pub screen_repo_manager: Arc<ScreenRepoManager>,
    /// True when byonk started as an HA Supervisor add-on (i.e. `/data/options.json`
    /// was present). In add-on mode, global-config admin writes are read-only.
    pub addon_mode: bool,
}

/// Create application state from an asset loader.
pub fn create_app_state(asset_loader: Arc<AssetLoader>) -> anyhow::Result<AppState> {
    let config = AppConfig::load_from_assets(&asset_loader)?;
    create_app_state_with_config(asset_loader, config)
}

/// Create application state with a custom config.
pub fn create_app_state_with_config(
    asset_loader: Arc<AssetLoader>,
    config: AppConfig,
) -> anyhow::Result<AppState> {
    create_app_state_with_overrides(asset_loader, Arc::new(config), DevOverrides::default())
}

/// Build the set of on-disk screen repos from `SCREEN_REPOS_DIR`.
///
/// Only handles that appear in `registry` (i.e. `config.screen_repos`) AND whose
/// directory `dir/<handle>` exists on disk are included. Non-builtin screen repos
/// are placed manually under `SCREEN_REPOS_DIR` in this phase; git fetching is a
/// later plan. `byonk-builtin` needs no disk entry — it registers embedded
/// inside `ScreenRepoLoader`.
fn collect_disk_packages(
    dir: &std::path::Path,
    registry: &HashMap<String, ScreenRepoRef>,
) -> HashMap<String, std::path::PathBuf> {
    registry
        .keys()
        .filter_map(|handle| {
            let path = dir.join(handle);
            if path.is_dir() {
                Some((handle.clone(), path))
            } else {
                None
            }
        })
        .collect()
}

/// Build the [`ScreenRepoManager`] from the embedded builtin screen repo, any
/// on-disk screen repos under `SCREEN_REPOS_DIR` declared in `config.screen_repos`, and
/// any previously fetched checkouts under the cache root (`SCREEN_REPOS_CACHE_DIR`
/// env, falling back to a temp dir). Rebuilds the loader once before
/// returning so the manager is immediately usable.
pub fn build_screen_repo_manager(
    asset_loader: Arc<AssetLoader>,
    config: SharedConfig,
) -> Arc<ScreenRepoManager> {
    let cache_root = std::env::var("SCREEN_REPOS_CACHE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("byonk-packages"));

    let extra_disk = std::env::var("SCREEN_REPOS_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|dir| collect_disk_packages(std::path::Path::new(&dir), &config.load().screen_repos))
        .unwrap_or_default();

    let manager = ScreenRepoManager::new(
        asset_loader,
        config,
        ScreenRepoCache::new(cache_root),
        extra_disk,
    );
    manager.rebuild_loader();
    manager
}

/// Create application state with shared overrides (for dev mode).
pub fn create_app_state_with_overrides(
    asset_loader: Arc<AssetLoader>,
    config: Arc<AppConfig>,
    dev_overrides: DevOverrides,
) -> anyhow::Result<AppState> {
    let admin_token = std::env::var("BYONK_ADMIN_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| config.admin.token.clone());

    let shared_config: SharedConfig = Arc::new(ArcSwap::from(config.clone()));

    let screen_repo_manager =
        build_screen_repo_manager(asset_loader.clone(), shared_config.clone());

    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&asset_loader)?);
    let content_pipeline = Arc::new(
        ContentPipeline::new(
            shared_config.clone(),
            asset_loader.clone(),
            renderer.clone(),
            screen_repo_manager.clone(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to create content pipeline: {e}"))?,
    );
    let content_cache = Arc::new(ContentCache::new());

    Ok(AppState {
        config: shared_config,
        asset_loader,
        admin_token,
        write_lock: Arc::new(tokio::sync::Mutex::new(())),
        registry,
        renderer,
        content_pipeline,
        content_cache,
        dev_overrides,
        screen_repo_manager,
        addon_mode: false,
    })
}

/// Reparse the config file via the asset loader and atomically swap it in.
pub fn reload_config(state: &AppState) -> anyhow::Result<()> {
    let fresh = AppConfig::load_from_assets(&state.asset_loader)?;
    state.config.store(Arc::new(fresh));
    state.screen_repo_manager.rebuild_loader();
    Ok(())
}

/// Build the API router with all endpoints and middleware.
///
/// This is the core router used by both production and tests.
/// It includes the `Connection: close` header to prevent connection
/// accumulation from ESP32 clients.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // TRMNL API endpoints (with and without trailing slashes for firmware compatibility)
        // TRMNL firmware 1.6.9+ sends requests with trailing slashes
        .route("/api/setup", get(handle_setup))
        .route("/api/setup/", get(handle_setup))
        .route("/api/display", get(handle_display))
        .route("/api/display/", get(handle_display))
        .route("/api/image/:hash", get(handle_image))
        .route("/api/log", post(api::handle_log))
        .route("/api/log/", post(api::handle_log))
        .route("/api/time", get(api::handle_time))
        .route("/api/time/", get(api::handle_time))
        // Health check
        .route("/health", get(|| async { "OK" }))
        // Admin API
        .nest("/api/admin", crate::api::admin::admin_router())
        // Add state and tracing with request IDs
        .with_state(state)
        .layer(TraceLayer::new_for_http().make_span_with(RequestIdSpan))
        // Disable keep-alive: ESP32 HTTPClient defaults to keep-alive but never
        // reuses connections, causing orphaned connections to accumulate.
        // See: https://github.com/usetrmnl/trmnl-firmware/pull/274
        .layer(SetResponseHeaderLayer::overriding(
            CONNECTION,
            axum::http::HeaderValue::from_static("close"),
        ))
}

// Wrapper handlers to extract state components for the underlying API handlers

async fn handle_setup(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    api::handle_setup(
        axum::extract::State(state.config.load_full()),
        axum::extract::State(state.registry),
        headers,
    )
    .await
}

async fn handle_display(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    api::handle_display(
        axum::extract::State(state.config.load_full()),
        axum::extract::State(state.registry),
        axum::extract::State(state.content_pipeline),
        axum::extract::State(state.content_cache),
        axum::extract::State(state.dev_overrides),
        headers,
    )
    .await
}

async fn handle_image(
    axum::extract::State(state): axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
) -> Result<axum::response::Response, ApiError> {
    api::handle_image(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_cache),
        axum::extract::State(state.content_pipeline),
        path,
    )
    .await
}

#[cfg(test)]
mod reload_tests {
    use super::*;

    #[test]
    fn test_appstate_has_package_manager_resolving_builtin() {
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let cfg = AppConfig::default();
        let state = create_app_state_with_config(loader, cfg).unwrap();
        assert!(state
            .screen_repo_manager
            .loader()
            .resolve("byonk-builtin/default")
            .is_some());
    }

    #[test]
    fn test_config_swap_is_visible() {
        use crate::models::config::{DeviceConfig, RESERVED_DEFAULT_KEY};
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state(loader).unwrap();
        // Embedded config ships the reserved DEFAULT device (HA owns all
        // other device registration).
        assert_eq!(
            state.config.load().default_device_screen(),
            Some("byonk-builtin/default")
        );

        // Swap in a config whose DEFAULT device screen is a sentinel.
        let mut cfg = (**state.config.load()).clone();
        cfg.devices.insert(
            RESERVED_DEFAULT_KEY.to_string(),
            DeviceConfig {
                screen: "sentinel".to_string(),
                ..Default::default()
            },
        );
        state.config.store(Arc::new(cfg));
        assert_eq!(
            state.config.load().default_device_screen(),
            Some("sentinel")
        );
    }
}
