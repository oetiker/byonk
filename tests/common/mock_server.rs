//! Mock HTTP server for testing Lua HTTP functions.

use wiremock::{
    matchers::{header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

/// Wrapper around wiremock MockServer with convenience methods
pub struct MockHttpServer {
    pub server: MockServer,
}

impl MockHttpServer {
    /// Start a new mock HTTP server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Self { server }
    }

    /// Get the base URL of the mock server
    pub fn url(&self) -> String {
        self.server.uri()
    }

    /// Get URL for a specific path
    pub fn url_for(&self, path: &str) -> String {
        format!("{}{}", self.server.uri(), path)
    }

    /// Mock a simple GET endpoint returning JSON
    pub async fn mock_get_json(&self, endpoint: &str, response: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock a GET endpoint returning HTML
    pub async fn mock_get_html(&self, endpoint: &str, html: &str) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(html)
                    .insert_header("content-type", "text/html"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock a GET endpoint with query parameters
    pub async fn mock_get_with_params(
        &self,
        endpoint: &str,
        param_name: &str,
        param_value: &str,
        response: serde_json::Value,
    ) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .and(query_param(param_name, param_value))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock a POST endpoint expecting JSON body
    pub async fn mock_post_json(&self, endpoint: &str, response: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path(endpoint))
            .and(header("content-type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock an endpoint requiring basic auth
    pub async fn mock_with_basic_auth(
        &self,
        endpoint: &str,
        expected_auth: &str,
        response: serde_json::Value,
    ) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .and(header("authorization", format!("Basic {}", expected_auth)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock an endpoint that returns an error
    pub async fn mock_error(&self, endpoint: &str, status: u16, message: &str) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(status).set_body_string(message))
            .mount(&self.server)
            .await;
    }
}
