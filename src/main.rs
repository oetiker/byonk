use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod api;
mod error;
mod models;
mod rendering;
mod services;

use models::AppConfig;
use services::{ContentPipeline, InMemoryRegistry, RenderService, UrlSigner};

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Byonk API",
        description = "Bring Your Own Ink - content server for TRMNL e-ink devices",
        version = "0.1.0",
        license(name = "MIT")
    ),
    paths(
        api::handle_setup,
        api::handle_display,
        api::handle_image,
        api::handle_log,
    ),
    components(schemas(
        api::SetupResponse,
        api::DisplayJsonResponse,
        api::LogRequest,
        api::LogResponse,
    )),
    tags(
        (name = "Device", description = "Device registration and management"),
        (name = "Display", description = "Display content retrieval"),
        (name = "Logging", description = "Device log submission")
    )
)]
struct ApiDoc;

/// Application state shared across all handlers
#[derive(Clone)]
struct AppState {
    registry: Arc<InMemoryRegistry>,
    renderer: Arc<RenderService>,
    url_signer: Arc<UrlSigner>,
    content_pipeline: Arc<ContentPipeline>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "byonk=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Configuration
    let svg_dir = std::env::var("SVG_DIR").unwrap_or_else(|_| "./static/svgs".to_string());
    let screens_dir = std::env::var("SCREENS_DIR").unwrap_or_else(|_| "./screens".to_string());
    let config_path = std::env::var("CONFIG_FILE").unwrap_or_else(|_| "./config.yaml".to_string());
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    tracing::info!(svg_dir = %svg_dir, screens_dir = %screens_dir, "Loading content directories");

    // Load application config
    let config = Arc::new(AppConfig::load_or_default(std::path::Path::new(
        &config_path,
    )));

    // Initialize services
    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&svg_dir)?);

    // URL signer for secure image URLs - use URL_SECRET env var or random
    let url_signer = Arc::new(
        std::env::var("URL_SECRET")
            .map(|s| UrlSigner::new(&s))
            .unwrap_or_else(|_| {
                tracing::warn!(
                    "No URL_SECRET set, using random secret (URLs won't survive restarts)"
                );
                UrlSigner::with_random_secret()
            }),
    );

    // Content pipeline for scripted content
    let content_pipeline = Arc::new(
        ContentPipeline::new(
            config.clone(),
            std::path::Path::new(&screens_dir),
            renderer.clone(),
        )
        .expect("Failed to initialize content pipeline"),
    );

    let state = AppState {
        registry: registry.clone(),
        renderer: renderer.clone(),
        url_signer: url_signer.clone(),
        content_pipeline: content_pipeline.clone(),
    };

    // Build router
    let app = Router::new()
        // TRMNL API endpoints
        .route("/api/setup", get(handle_setup))
        .route("/api/display", get(handle_display))
        .route("/api/image/:device_id", get(handle_image))
        .route("/api/log", post(api::handle_log))
        // OpenAPI documentation
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Static file serving
        .nest_service("/static", ServeDir::new("./static"))
        // Health check
        .route("/health", get(|| async { "OK" }))
        // Add state and tracing
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // Bind server
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Byonk server listening");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Wrapper for setup handler with correct state extraction
async fn handle_setup(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, error::ApiError> {
    api::handle_setup(axum::extract::State(state.registry), headers).await
}

/// Wrapper for display handler with correct state extraction
async fn handle_display(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, error::ApiError> {
    api::handle_display(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.url_signer),
        headers,
    )
    .await
}

/// Wrapper for image handler with correct state extraction
async fn handle_image(
    axum::extract::State(state): axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
    query: axum::extract::Query<api::display::ImageQuery>,
) -> Result<axum::response::Response, error::ApiError> {
    api::handle_image(
        axum::extract::State(state.renderer),
        axum::extract::State(state.url_signer),
        axum::extract::State(state.content_pipeline),
        path,
        query,
    )
    .await
}
