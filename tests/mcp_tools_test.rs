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

/// Read `config.yaml`'s `devices` map directly. The admin `/api/admin/devices`
/// listing synthesizes one merged row per registry-seen device keyed by MAC
/// regardless of which key its config entry actually lives under, so it
/// cannot be used to prove how many (or which) `config.devices` entries
/// really exist — go to the source of truth instead.
fn read_config_devices(
    config_path: &std::path::Path,
) -> serde_json::Map<String, serde_json::Value> {
    let yaml = std::fs::read_to_string(config_path).expect("read config.yaml");
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("parse config.yaml");
    let json = serde_json::to_value(value).expect("yaml to json");
    json.get("devices")
        .and_then(|d| d.as_object())
        .cloned()
        .unwrap_or_default()
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

#[tokio::test]
async fn test_copy_then_edit_a_builtin_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    // Fork the read-only builtin into the writable local repo.
    let copied = client
        .call_tool(
            "copy_screen",
            serde_json::json!({
                "from_ref": "byonk-builtin/default",
                "to_handle": "local",
                "to_name": "mine"
            }),
        )
        .await;
    assert_eq!(structured(&copied)["screen_ref"], "local/mine");

    // Read it, then write it back with its etag.
    let read = client
        .call_tool(
            "read_screen_file",
            serde_json::json!({ "screen_ref": "local/mine", "file": "script.lua" }),
        )
        .await;
    let etag = structured(&read)["etag"].as_str().unwrap().to_string();

    let written = client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/mine",
                "file": "script.lua",
                "content": "return { hello = \"world\" }\n",
                "if_match": etag
            }),
        )
        .await;
    assert_ne!(structured(&written)["etag"], etag, "etag must change");
}

#[tokio::test]
async fn test_write_to_a_read_only_handle_names_copy_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "byonk-builtin/default",
                "file": "script.lua",
                "content": "return {}\n"
            }),
        )
        .await;

    assert_eq!(result["isError"], true);
    let text = serde_json::to_string(&result).unwrap();
    assert!(
        text.contains("copy_screen"),
        "the refusal must tell the agent how to proceed: {text}"
    );
}

#[tokio::test]
async fn test_stale_etag_is_a_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "conflicted" }),
        )
        .await;

    let result = client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/conflicted",
                "file": "script.lua",
                "content": "return {}\n",
                "if_match": "0".repeat(64)
            }),
        )
        .await;

    assert_eq!(result["isError"], true);
    assert!(serde_json::to_string(&result).unwrap().contains("conflict"));
}

#[tokio::test]
async fn test_create_rename_delete_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "tmp1" }),
        )
        .await;
    client
        .call_tool(
            "rename_screen",
            serde_json::json!({ "screen_ref": "local/tmp1", "new_name": "tmp2" }),
        )
        .await;

    let listed = client
        .call_tool("list_screens", serde_json::json!({}))
        .await;
    let refs: Vec<String> = structured(&listed)["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["screen_ref"].as_str().unwrap().to_string())
        .collect();
    assert!(refs.contains(&"local/tmp2".to_string()));
    assert!(!refs.contains(&"local/tmp1".to_string()));

    client
        .call_tool(
            "delete_screen",
            serde_json::json!({ "screen_ref": "local/tmp2" }),
        )
        .await;

    let after = client
        .call_tool("list_screens", serde_json::json!({}))
        .await;
    let refs: Vec<String> = structured(&after)["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["screen_ref"].as_str().unwrap().to_string())
        .collect();
    assert!(!refs.contains(&"local/tmp2".to_string()));
}

#[tokio::test]
async fn test_render_screen_returns_an_image_block() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({ "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    let content = result["content"].as_array().unwrap();
    let image = content
        .iter()
        .find(|c| c["type"] == "image")
        .expect("render must return an image content block");
    assert_eq!(image["mimeType"], "image/png");
    // Base64 PNG magic: iVBORw0KGgo
    assert!(image["data"].as_str().unwrap().starts_with("iVBORw0KGgo"));
    // A render that worked must not be flagged as an error.
    assert_ne!(result["isError"], serde_json::json!(true), "{result}");
}

#[tokio::test]
async fn test_render_of_a_broken_script_reports_the_lua_line() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "broken" }),
        )
        .await;
    client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/broken",
                "file": "script.lua",
                // Line 2 indexes a nil value at runtime.
                "content": "local t = nil\nreturn { x = t.y }\n"
            }),
        )
        .await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({ "screen_ref": "local/broken" }),
        )
        .await;

    let s = structured(&result);
    let error = &s["error"];
    assert!(!error.is_null(), "a broken script must report an error");
    assert_eq!(error["line"], 2, "the Lua error line must be reported");

    // A failed render is `is_error: true` — the diagnostics alone are not
    // enough, since a client that only reads the flag would call it a success.
    assert_eq!(
        result["isError"],
        serde_json::json!(true),
        "a failed render must be flagged is_error: {result}"
    );
    // ...and it must carry NO image block: `png` is empty on failure, so
    // emitting one would hand the agent a zero-byte image.
    let content = result["content"].as_array().unwrap();
    assert!(
        !content.iter().any(|c| c["type"] == "image"),
        "a failed render must not emit an image block: {result}"
    );
    // The diagnostics must still arrive as visible content, not only as
    // structured output.
    assert!(
        content.iter().any(|c| c["type"] == "text"),
        "a failed render must still carry its diagnostics as text: {result}"
    );
}

#[tokio::test]
async fn test_render_captures_script_log_output() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "chatty" }),
        )
        .await;
    client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/chatty",
                "file": "script.lua",
                "content": "log_info(\"hello from lua\")\nreturn {}\n"
            }),
        )
        .await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({ "screen_ref": "local/chatty" }),
        )
        .await;

    let log = serde_json::to_string(&structured(&result)["log"]).unwrap();
    assert!(log.contains("hello from lua"), "log not captured: {log}");
}

#[tokio::test]
async fn test_validate_screen_flags_a_lua_syntax_error() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "syntax" }),
        )
        .await;
    client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/syntax",
                "file": "script.lua",
                "content": "return {\n"
            }),
        )
        .await;

    let result = client
        .call_tool(
            "validate_screen",
            serde_json::json!({ "screen_ref": "local/syntax" }),
        )
        .await;

    let s = structured(&result);
    assert_eq!(s["ok"], false);
    let issues = s["issues"].as_array().unwrap();
    assert!(issues.iter().any(|i| i["location"] == "script.lua"));
}

#[tokio::test]
async fn test_validate_of_a_healthy_builtin_passes() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "validate_screen",
            serde_json::json!({ "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    assert_eq!(structured(&result)["ok"], true);
}

#[tokio::test]
async fn test_assign_screen_updates_the_device_mapping() {
    let tmp = tempfile::tempdir().unwrap();
    // A file-backed config is required — device writes persist to disk.
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    app.register_device(mac).await;

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    assert_eq!(structured(&result)["screen"], "byonk-builtin/default");
    // The mac was only ever seen (never configured) before this call, so
    // this must be a create, not an update.
    assert_eq!(structured(&result)["created"], true);

    // And it is visible through list_devices.
    let devices = client
        .call_tool("list_devices", serde_json::json!({}))
        .await;
    let d = structured(&devices)["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == mac)
        .cloned()
        .expect("device must be listed");
    assert_eq!(d["screen"], "byonk-builtin/default");
}

#[tokio::test]
async fn test_assign_screen_rejects_an_unknown_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    app.register_device(mac).await;

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "local/does-not-exist" }),
        )
        .await;

    assert_eq!(result["isError"], true);
}

#[tokio::test]
async fn test_assign_screen_reassigns_an_already_configured_device() {
    // Exercises the update-in-place path (apply_device_patch succeeding
    // directly), not the seen-but-unconfigured create fallback: assign twice,
    // to two different screens, and confirm the second call updates the
    // existing mapping rather than conflicting with it.
    let tmp = tempfile::tempdir().unwrap();
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    app.register_device(mac).await;

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/calibration/color" }),
        )
        .await;

    assert_ne!(result["isError"], serde_json::json!(true));
    assert_eq!(
        structured(&result)["screen"],
        "byonk-builtin/calibration/color"
    );
    // Second call updates the mapping the first call created — not a create.
    assert_eq!(structured(&result)["created"], false);

    let devices = client
        .call_tool("list_devices", serde_json::json!({}))
        .await;
    let d = structured(&devices)["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == mac)
        .cloned()
        .expect("device must be listed");
    assert_eq!(d["screen"], "byonk-builtin/calibration/color");
}

#[tokio::test]
async fn test_assign_screen_rejects_a_mac_the_registry_has_never_seen() {
    // An arbitrary/typo'd mac must not silently create a phantom device —
    // there is no MCP tool to delete one. Only a mac the registry actually
    // reports (i.e. one that has polled /api/setup) may be auto-created.
    let tmp = tempfile::tempdir().unwrap();
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66"; // never registered

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    assert_eq!(result["isError"], true);

    // And it must not have persisted a phantom device entry as a side effect.
    let auth = ("Authorization", "Bearer secret");
    let listed = app.get_with_headers("/api/admin/devices", &[auth]).await;
    let json: serde_json::Value = listed.json();
    assert!(
        json.as_array().unwrap().iter().all(|d| d["key"] != mac),
        "no device should have been created for an unseen mac: {json}"
    );
}

#[tokio::test]
async fn test_assign_screen_patches_a_device_configured_by_registration_code() {
    // A device may be configured in config.yaml under its registration code
    // rather than its MAC (the documented HA onboarding path). assign_screen
    // is only ever given the MAC (as list_devices reports it), so it must
    // resolve that back to the code-keyed entry and patch it in place —
    // not miss and create a second, MAC-keyed entry that shadows it.
    let tmp = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    let api_key = app.register_device(mac).await;
    let code = byonk::models::ApiKey::new(api_key).registration_code();

    // Pre-create the device's config entry under the registration code, with
    // an existing name that must survive the assign_screen call untouched.
    let auth = ("Authorization", "Bearer secret");
    let create = app
        .post_json(
            "/api/admin/devices",
            &[auth],
            &format!(r#"{{"key":"{code}","screen":"byonk-builtin/default","name":"living-room"}}"#),
        )
        .await;
    assert_eq!(create.status, axum::http::StatusCode::OK);

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/calibration/color" }),
        )
        .await;
    assert_ne!(result["isError"], serde_json::json!(true), "{result}");
    assert_eq!(structured(&result)["created"], false);
    assert_eq!(structured(&result)["key"], code);

    // Inspect config.yaml directly — the admin listing endpoint synthesizes
    // a merged row per registry-seen device keyed by MAC even when the real
    // config entry lives under a different key, so it can't be used to prove
    // a second config.devices entry wasn't created.
    let devices = read_config_devices(&config_path);
    assert!(
        devices.contains_key(&code) && !devices.contains_key(mac),
        "the code-keyed entry must be updated in place, no MAC-keyed entry created: {devices:?}"
    );
    let entry = &devices[&code];
    assert_eq!(entry["screen"], "byonk-builtin/calibration/color");
    assert_eq!(
        entry["name"], "living-room",
        "other fields on the pre-existing entry must survive"
    );
}

#[tokio::test]
async fn test_assign_screen_patches_a_device_configured_under_a_differently_cased_mac() {
    // config.yaml may key a device under an upper-cased MAC while the device
    // itself (and list_devices) report it lower-case — get_device_config
    // deliberately retries uppercased. assign_screen must patch that same
    // entry, not create a new lower-case-keyed one that shadows it.
    let tmp = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    // Must contain hex letters — an all-digit MAC is identical upper/lower
    // case and wouldn't exercise the case-differing lookup at all.
    let mac_lower = "aa:bb:cc:dd:ee:ff";
    let mac_upper = mac_lower.to_uppercase();
    app.register_device(mac_lower).await;

    let auth = ("Authorization", "Bearer secret");
    let create = app
        .post_json(
            "/api/admin/devices",
            &[auth],
            &format!(
                r#"{{"key":"{mac_upper}","screen":"byonk-builtin/default","name":"kitchen"}}"#
            ),
        )
        .await;
    assert_eq!(create.status, axum::http::StatusCode::OK);

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac_lower, "screen_ref": "byonk-builtin/calibration/color" }),
        )
        .await;
    assert_ne!(result["isError"], serde_json::json!(true), "{result}");
    assert_eq!(structured(&result)["created"], false);
    assert_eq!(structured(&result)["key"], mac_upper);

    // Inspect config.yaml directly — see the comment in the registration-code
    // variant of this test for why the admin listing endpoint can't be used
    // to prove no second entry was created.
    let devices = read_config_devices(&config_path);
    assert!(
        devices.contains_key(&mac_upper) && !devices.contains_key(mac_lower),
        "the upper-case-keyed entry must be updated in place, no lower-case entry created: {devices:?}"
    );
    let entry = &devices[&mac_upper];
    assert_eq!(entry["screen"], "byonk-builtin/calibration/color");
    assert_eq!(
        entry["name"], "kitchen",
        "other fields on the pre-existing entry must survive"
    );
}

#[tokio::test]
async fn test_assign_screen_reassignment_preserves_existing_params() {
    // The tool description says a screen reassignment carries the device's
    // existing params over unchanged rather than resetting them to the new
    // screen's defaults. Pin that down so a future refactor can't silently
    // flip it while the suite stays green.
    let tmp = tempfile::tempdir().unwrap();
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    app.register_device(mac).await;

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    // First assignment creates the mapping.
    client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    // Set a custom param out-of-band (assign_screen itself takes no params).
    let auth = ("Authorization", "Bearer secret");
    let resp = app
        .patch_json(
            &format!("/api/admin/devices/{mac}"),
            &[auth],
            r#"{"params":{"keep_me":"still-here"}}"#,
        )
        .await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);

    // Reassign to a different screen with no params in the call.
    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/calibration/color" }),
        )
        .await;
    assert_ne!(result["isError"], serde_json::json!(true), "{result}");

    let listed = app.get_with_headers("/api/admin/devices", &[auth]).await;
    let json: serde_json::Value = listed.json();
    let row = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == mac)
        .cloned()
        .expect("device row present");
    assert_eq!(row["screen"], "byonk-builtin/calibration/color");
    assert_eq!(
        row["params"]["keep_me"], "still-here",
        "reassignment must not reset params to the new screen's defaults"
    );
}
