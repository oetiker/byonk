//! Test application factory for integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Router,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

use byonk::api::{handle_display, handle_image, handle_log, handle_setup};
use byonk::assets::AssetLoader;
use byonk::error::ApiError;
use byonk::models::AppConfig;
use byonk::services::{ContentCache, ContentPipeline, InMemoryRegistry, RenderService};

/// Application state shared across all handlers (mirrors main.rs)
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<InMemoryRegistry>,
    pub renderer: Arc<RenderService>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
}

/// Test application with router and direct access to services
pub struct TestApp {
    router: Router,
    pub registry: Arc<InMemoryRegistry>,
    pub content_cache: Arc<ContentCache>,
}

impl TestApp {
    /// Create a new test application using embedded assets
    pub fn new() -> Self {
        // Create asset loader with embedded assets only (no external paths)
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));

        // Load application config
        let config = Arc::new(AppConfig::load_from_assets(&asset_loader));

        // Initialize services
        let registry = Arc::new(InMemoryRegistry::new());
        let renderer =
            Arc::new(RenderService::new(&asset_loader).expect("Failed to create render service"));
        let content_pipeline = Arc::new(
            ContentPipeline::new(config, asset_loader, renderer.clone())
                .expect("Failed to create content pipeline"),
        );
        let content_cache = Arc::new(ContentCache::new());

        let state = AppState {
            registry: registry.clone(),
            renderer: renderer.clone(),
            content_pipeline: content_pipeline.clone(),
            content_cache: content_cache.clone(),
        };

        // Build router (mirrors main.rs structure)
        let router = Router::new()
            .route("/api/setup", get(wrap_setup))
            .route("/api/display", get(wrap_display))
            .route("/api/image/:hash", get(wrap_image))
            .route("/api/log", post(handle_log))
            .route("/health", get(|| async { "OK" }))
            .with_state(state);

        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Make a GET request to the given path
    pub async fn get(&self, path: &str) -> TestResponse {
        self.request(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    /// Make a GET request with custom headers
    pub async fn get_with_headers(&self, path: &str, headers: &[(&str, &str)]) -> TestResponse {
        let mut builder = Request::get(path);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.request(builder.body(Body::empty()).unwrap()).await
    }

    /// Make a POST request with JSON body
    pub async fn post_json(
        &self,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> TestResponse {
        let mut builder = Request::post(path).header("Content-Type", "application/json");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.request(builder.body(Body::from(body.to_string())).unwrap())
            .await
    }

    /// Send a request to the router
    async fn request(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("Request failed");

        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to collect body")
            .to_bytes()
            .to_vec();

        TestResponse {
            status,
            headers,
            body,
        }
    }

    /// Register a device and return (api_key, friendly_id)
    pub async fn register_device(&self, mac: &str) -> (String, String) {
        let headers = [("ID", mac), ("FW-Version", "1.0.0"), ("Model", "og")];
        let response = self.get_with_headers("/api/setup", &headers).await;
        assert_eq!(response.status, StatusCode::OK);

        let json: serde_json::Value = response.json();
        let api_key = json["api_key"].as_str().unwrap().to_string();
        let friendly_id = json["friendly_id"].as_str().unwrap().to_string();
        (api_key, friendly_id)
    }
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Test response with convenience methods
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    /// Parse body as JSON
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("Failed to parse JSON response")
    }

    /// Get body as string
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Get raw body bytes
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Check if response is a PNG image
    pub fn is_png(&self) -> bool {
        self.body.len() >= 8 && &self.body[0..8] == b"\x89PNG\r\n\x1a\n"
    }
}

// Wrapper handlers to match state extraction pattern from main.rs
async fn wrap_setup(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    handle_setup(axum::extract::State(state.registry), headers).await
}

async fn wrap_display(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    handle_display(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_pipeline),
        axum::extract::State(state.content_cache),
        headers,
    )
    .await
}

async fn wrap_image(
    axum::extract::State(state): axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
) -> Result<axum::response::Response, ApiError> {
    handle_image(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_cache),
        axum::extract::State(state.content_pipeline),
        path,
    )
    .await
}
