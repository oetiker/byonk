mod common;

use common::mcp::McpTestClient;
use common::TestApp;

/// Every tool call returns `content` (human-readable) and, for tools that
/// declare an output schema, `structuredContent`. Assertions read the
/// structured form.
fn structured(result: &serde_json::Value) -> &serde_json::Value {
    result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("no structuredContent in {result}"))
}

#[tokio::test]
async fn test_list_screens_reports_builtin_as_read_only() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool("list_screens", serde_json::json!({}))
        .await;

    let screens = structured(&result)["screens"].as_array().unwrap();
    let builtin = screens
        .iter()
        .find(|s| s["screen_ref"] == "byonk-builtin/default")
        .expect("builtin default must be listed");
    assert_eq!(builtin["writable"], false);
}

#[tokio::test]
async fn test_read_screen_file_returns_content_and_etag() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "read_screen_file",
            serde_json::json!({ "screen_ref": "byonk-builtin/default", "file": "meta.yaml" }),
        )
        .await;

    let s = structured(&result);
    assert!(s["content"].as_str().unwrap().contains("title:"));
    assert_eq!(s["etag"].as_str().unwrap().len(), 64, "blake3 hex etag");
    assert_eq!(s["binary"], false);
}

#[tokio::test]
async fn test_read_screen_file_on_a_missing_screen_is_a_tool_error() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "read_screen_file",
            serde_json::json!({ "screen_ref": "local/nope", "file": "meta.yaml" }),
        )
        .await;

    // A tool-level failure is `isError: true` on the result, not a
    // JSON-RPC error — the agent must be able to read and recover from it.
    assert_eq!(result["isError"], true);
}

#[tokio::test]
async fn test_list_screen_repos_reports_kind() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool("list_screen_repos", serde_json::json!({}))
        .await;

    let repos = structured(&result)["repos"].as_array().unwrap();
    let builtin = repos
        .iter()
        .find(|r| r["handle"] == "byonk-builtin")
        .expect("byonk-builtin must be listed");
    assert_eq!(builtin["kind"], "embedded");
    assert_eq!(builtin["writable"], false);
}

/// A configured screen repo with zero screens (e.g. one an authoring agent
/// just created and hasn't populated yet) must still be listed — otherwise
/// the agent has nowhere to see it is allowed to write. `loader.list_all()`
/// alone would never surface it, since it only enumerates resolved screens;
/// this must come from the union with `config.screen_repos.keys()`, mirroring
/// `src/api/admin/read.rs::screen_repos`.
#[tokio::test]
async fn test_list_screen_repos_includes_a_zero_screen_repo() {
    let dir = tempfile::tempdir().unwrap();
    let repo_dir = dir.path().join("empty_repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: empty\ndescription: d\nauthor: a\nlicense: MIT\n",
    )
    .unwrap();
    let yaml = format!(
        "admin:\n  token: secret\nscreen_repos:\n  empty:\n    path: {}\n",
        repo_dir.display()
    );
    let (app, _cfg) = TestApp::new_with_config_yaml(&yaml, dir.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool("list_screen_repos", serde_json::json!({}))
        .await;

    let repos = structured(&result)["repos"].as_array().unwrap();
    let empty = repos
        .iter()
        .find(|r| r["handle"] == "empty")
        .expect("a zero-screen configured repo must still be listed");
    assert_eq!(empty["screen_count"], 0);
    assert_eq!(empty["kind"], "local");
    assert_eq!(empty["writable"], true);
}

#[tokio::test]
async fn test_get_config_redacts_secrets() {
    // `TestApp::new_admin` only sets the token on the in-memory `AppConfig`
    // — the embedded `default-config.yaml` it reads from disk has no
    // `admin:` section at all, so the token string never appears in
    // `redacted_config`'s input and this test would pass even with every
    // line of redaction deleted. `new_admin_with_file` writes a real config
    // FILE containing `admin:\n  token: secret\n`, so the token is actually
    // present in what `redacted_config` reads and must be stripped.
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_admin_with_file("secret", dir.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client.call_tool("get_config", serde_json::json!({})).await;

    let value = structured(&result);
    let text = serde_json::to_string(value).unwrap();
    assert!(
        !text.contains("secret"),
        "admin token must never appear in get_config output: {text}"
    );
    // Without this, a total read failure collapsing to `{"_error": ...}`
    // would also satisfy the assertion above — prove redaction actually
    // happened, not that the whole config got lost.
    assert!(
        value["admin"].is_object(),
        "admin section must still be present (redacted, not dropped): {value}"
    );
}

#[tokio::test]
async fn test_list_devices_includes_a_registered_device() {
    let app = TestApp::new_admin("secret");
    app.register_device("11:22:33:44:55:66").await;
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool("list_devices", serde_json::json!({}))
        .await;

    let devices = structured(&result)["devices"].as_array().unwrap();
    assert!(devices.iter().any(|d| d["mac"] == "11:22:33:44:55:66"));
}
