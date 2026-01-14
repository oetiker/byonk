//! Test application factory for integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

use byonk::assets::AssetLoader;
use byonk::server::{build_router, create_app_state, AppState};
use byonk::services::{ContentCache, DeviceRegistry, InMemoryRegistry};

/// Test application with router and direct access to services
pub struct TestApp {
    router: axum::Router,
    pub registry: Arc<InMemoryRegistry>,
    pub content_cache: Arc<ContentCache>,
}

impl TestApp {
    /// Create a new test application using embedded assets
    pub fn new() -> Self {
        // Create asset loader with embedded assets only (no external paths)
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));

        // Create application state using shared server module
        let state = create_app_state(asset_loader).expect("Failed to create app state");

        // Keep references for test assertions
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();

        // Build router using shared server module (same as production)
        let router = build_router(state);

        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Create a test app and return the state for custom router configuration
    pub fn create_state() -> AppState {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        create_app_state(asset_loader).expect("Failed to create app state")
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
