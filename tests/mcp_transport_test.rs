mod common;

use axum::http::StatusCode;
use common::mcp::McpTestClient;
use common::TestApp;

#[tokio::test]
async fn test_mcp_is_invisible_when_no_admin_token_is_configured() {
    // Matches admin-route behaviour: no token ⇒ 404, not 401. The endpoint
    // must not advertise its own existence.
    let app = TestApp::new();
    let client = McpTestClient::new(&app, None);

    let resp = client.raw("initialize", serde_json::json!({})).await;

    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_rejects_a_missing_token() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, None);

    let resp = client.raw("initialize", serde_json::json!({})).await;

    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mcp_rejects_a_wrong_token() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("wrong"));

    let resp = client.raw("initialize", serde_json::json!({})).await;

    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mcp_handshake_reports_byonk_as_the_server() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));

    let result = client.initialize().await;

    // Must be byonk's own identity — `Implementation::from_build_env()`
    // would report rmcp's crate name and version instead.
    assert_eq!(result["serverInfo"]["name"], "byonk");
    assert_eq!(
        result["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION"),
        "server version must track byonk's Cargo version"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "tools capability must be advertised"
    );
}

#[tokio::test]
async fn test_mcp_requires_the_streaming_accept_header() {
    let app = TestApp::new_admin("secret");

    let resp = app
        .post_json(
            "/mcp",
            &[
                ("Accept", "application/json"),
                ("Authorization", "Bearer secret"),
                // See the comment in `common::mcp::McpTestClient::raw` — rmcp's
                // DNS-rebinding guard requires a Host header on every request.
                ("Host", "localhost"),
            ],
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        )
        .await;

    assert_eq!(resp.status, StatusCode::NOT_ACCEPTABLE);
}
