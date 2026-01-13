//! Mock HTTP server for testing Lua HTTP functions.

use wiremock::{
    matchers::{method, path, query_param, header, body_string_contains},
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

    /// Mock a GET endpoint returning plain text
    pub async fn mock_get_text(&self, endpoint: &str, body: &str) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
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

    /// Mock a POST endpoint checking for specific body content
    pub async fn mock_post_with_body(
        &self,
        endpoint: &str,
        body_contains: &str,
        response: serde_json::Value,
    ) {
        Mock::given(method("POST"))
            .and(path(endpoint))
            .and(body_string_contains(body_contains))
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

    /// Mock an endpoint with custom headers in response
    pub async fn mock_with_headers(
        &self,
        endpoint: &str,
        response_headers: &[(&str, &str)],
        body: &str,
    ) {
        let mut template = ResponseTemplate::new(200).set_body_string(body);
        for (name, value) in response_headers {
            template = template.insert_header(*name, *value);
        }
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(template)
            .mount(&self.server)
            .await;
    }

    /// Mock an endpoint that times out (responds after delay)
    pub async fn mock_slow(&self, endpoint: &str, delay_ms: u64, response: &str) {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(response)
                    .set_delay(std::time::Duration::from_millis(delay_ms)),
            )
            .mount(&self.server)
            .await;
    }
}
