mod common;

use common::mcp::McpTestClient;
use common::TestApp;

#[tokio::test]
async fn test_resources_list_includes_the_authoring_contracts() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client.raw("resources/list", serde_json::json!({})).await;
    let v: serde_json::Value = resp.json();
    let uris: Vec<String> = v["result"]["resources"]
        .as_array()
        .expect("resources array")
        .iter()
        .map(|r| r["uri"].as_str().unwrap().to_string())
        .collect();

    for expected in [
        "byonk://reference/lua-api",
        "byonk://reference/svg-templates",
        "byonk://reference/authoring",
        "byonk://schema/meta.yaml",
    ] {
        assert!(
            uris.contains(&expected.to_string()),
            "missing {expected} in {uris:?}"
        );
    }
}

#[tokio::test]
async fn test_reading_the_lua_reference_returns_the_shipped_doc() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://reference/lua-api" }),
        )
        .await;
    let v: serde_json::Value = resp.json();
    let text = v["result"]["contents"][0]["text"].as_str().unwrap();

    assert!(text.contains("log_info"), "Lua reference looks wrong");
    assert!(text.len() > 1000, "Lua reference is suspiciously short");
}

#[tokio::test]
async fn test_reading_the_meta_schema_returns_json() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://schema/meta.yaml" }),
        )
        .await;
    let v: serde_json::Value = resp.json();
    let text = v["result"]["contents"][0]["text"].as_str().unwrap();

    let schema: serde_json::Value = serde_json::from_str(text).expect("must be valid JSON");
    assert!(schema["properties"]["title"].is_object());
}

#[tokio::test]
async fn test_worked_examples_are_listed_and_readable() {
    // The examples repo is seeded on first run, so it needs a real
    // SCREENS_DIR — an embedded-only app has nothing to seed into.
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client.raw("resources/list", serde_json::json!({})).await;
    let v: serde_json::Value = resp.json();
    let examples: Vec<String> = v["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .filter(|u| u.starts_with("byonk://examples/"))
        .map(|u| u.to_string())
        .collect();
    assert!(
        !examples.is_empty(),
        "the shipped examples must be exposed as resources"
    );

    let resp = client
        .raw("resources/read", serde_json::json!({ "uri": examples[0] }))
        .await;
    let v: serde_json::Value = resp.json();
    let text = v["result"]["contents"][0]["text"].as_str().unwrap();
    // A worked example is only useful if it shows the full triple.
    for section in ["meta.yaml", "script.lua", "screen.svg"] {
        assert!(text.contains(section), "example is missing {section}");
    }
}

#[tokio::test]
async fn test_examples_resource_cannot_read_other_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    // `byonk-builtin/default` exists, but not under the examples handle —
    // the prefix must not become a general read primitive.
    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://examples/../byonk-builtin/default" }),
        )
        .await;
    let v: serde_json::Value = resp.json();

    assert!(v.get("error").is_some(), "must not resolve: {v}");
}

#[tokio::test]
async fn test_reading_an_unknown_resource_is_an_error() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://reference/nope" }),
        )
        .await;
    let v: serde_json::Value = resp.json();

    assert!(v.get("error").is_some(), "unknown URI must error: {v}");
}
