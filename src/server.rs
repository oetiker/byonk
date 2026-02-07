//! HTTP server setup and configuration.
//!
//! This module provides the router and application state used by both
//! the production server and integration tests.

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
use crate::models::AppConfig;
use crate::services::{ContentCache, ContentPipeline, InMemoryRegistry, RenderService};

/// Dither tuning override: (error_clamp, noise_scale, chroma_clamp).
pub type TuningOverride = (Option<f32>, Option<f32>, Option<f32>);

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
    /// device_config_key → dither tuning (error_clamp, noise_scale, chroma_clamp).
    pub tuning: Arc<RwLock<HashMap<String, TuningOverride>>>,
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

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub registry: Arc<InMemoryRegistry>,
    pub renderer: Arc<RenderService>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
    pub dev_overrides: DevOverrides,
}

/// Create application state from an asset loader.
pub fn create_app_state(asset_loader: Arc<AssetLoader>) -> anyhow::Result<AppState> {
    let config = Arc::new(AppConfig::load_from_assets(&asset_loader));
    create_app_state_with_config(asset_loader, config)
}

/// Create application state with a custom config.
pub fn create_app_state_with_config(
    asset_loader: Arc<AssetLoader>,
    config: Arc<AppConfig>,
) -> anyhow::Result<AppState> {
    create_app_state_with_overrides(asset_loader, config, DevOverrides::default())
}

/// Create application state with shared overrides (for dev mode).
pub fn create_app_state_with_overrides(
    asset_loader: Arc<AssetLoader>,
    config: Arc<AppConfig>,
    dev_overrides: DevOverrides,
) -> anyhow::Result<AppState> {
    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&asset_loader)?);
    let content_pipeline = Arc::new(
        ContentPipeline::new(config.clone(), asset_loader, renderer.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create content pipeline: {e}"))?,
    );
    let content_cache = Arc::new(ContentCache::new());

    Ok(AppState {
        config,
        registry,
        renderer,
        content_pipeline,
        content_cache,
        dev_overrides,
    })
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
        axum::extract::State(state.config),
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
        axum::extract::State(state.config),
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
