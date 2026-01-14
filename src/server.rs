//! HTTP server setup and configuration.
//!
//! This module provides the router and application state used by both
//! the production server and integration tests.

use axum::{
    http::header::CONNECTION,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};

use crate::api;
use crate::assets::AssetLoader;
use crate::error::ApiError;
use crate::models::AppConfig;
use crate::services::{ContentCache, ContentPipeline, InMemoryRegistry, RenderService};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<InMemoryRegistry>,
    pub renderer: Arc<RenderService>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
}

/// Create application state from an asset loader.
pub fn create_app_state(asset_loader: Arc<AssetLoader>) -> anyhow::Result<AppState> {
    let config = Arc::new(AppConfig::load_from_assets(&asset_loader));
    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&asset_loader)?);
    let content_pipeline = Arc::new(
        ContentPipeline::new(config, asset_loader, renderer.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create content pipeline: {e}"))?,
    );
    let content_cache = Arc::new(ContentCache::new());

    Ok(AppState {
        registry,
        renderer,
        content_pipeline,
        content_cache,
    })
}

/// Build the API router with all endpoints and middleware.
///
/// This is the core router used by both production and tests.
/// It includes the `Connection: close` header to prevent connection
/// accumulation from ESP32 clients.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // TRMNL API endpoints
        .route("/api/setup", get(handle_setup))
        .route("/api/display", get(handle_display))
        .route("/api/image/:hash", get(handle_image))
        .route("/api/log", post(api::handle_log))
        // Health check
        .route("/health", get(|| async { "OK" }))
        // Add state and tracing
        .with_state(state)
        .layer(TraceLayer::new_for_http())
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
    api::handle_setup(axum::extract::State(state.registry), headers).await
}

async fn handle_display(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    api::handle_display(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_pipeline),
        axum::extract::State(state.content_cache),
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
