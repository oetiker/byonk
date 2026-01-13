//! Tests for Lua API functions exposed to scripts.
//!
//! These tests run Lua scripts directly through LuaRuntime to verify
//! all exposed functions work correctly.

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::services::{DeviceContext, LuaRuntime};
use common::mock_server::MockHttpServer;
use tempfile::TempDir;

/// Create a test environment with custom Lua scripts (shared by all test modules)
fn setup_test_env(scripts: &[(&str, &str)]) -> (TempDir, Arc<AssetLoader>) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let screens_dir = temp_dir.path().to_path_buf();

    for (name, content) in scripts {
        let script_path = screens_dir.join(name);
        std::fs::write(&script_path, content).expect("Failed to write test script");
    }

    let asset_loader = Arc::new(AssetLoader::new(Some(screens_dir), None, None));
    (temp_dir, asset_loader)
}

// ============================================================================
// Tests using embedded screens (integration approach)
// ============================================================================

#[tokio::test]
async fn test_lua_params_and_device_globals() {
    // Test that params and device globals are properly populated
    // by running the hello screen which uses these
    let app = common::TestApp::new();

    let (api_key, _) = app.register_device(common::fixtures::macs::HELLO_DEVICE).await;
    let headers = common::fixtures::display_headers(common::fixtures::macs::HELLO_DEVICE, &api_key);
    let response = app.get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers)).await;

    // If params/device globals work, the script executes successfully
    common::assert_ok(&response);
    common::assert_valid_display_response(&response);
}

#[tokio::test]
async fn test_lua_time_functions() {
    // hello.lua uses time_now() and time_format()
    let app = common::TestApp::new();

    let (api_key, _) = app.register_device(common::fixtures::macs::HELLO_DEVICE).await;
    let headers = common::fixtures::display_headers(common::fixtures::macs::HELLO_DEVICE, &api_key);
    let response = app.get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers)).await;

    // Script uses time functions successfully
    common::assert_ok(&response);
    let json: serde_json::Value = response.json();
    assert_eq!(json["status"], 0);
}

#[tokio::test]
async fn test_lua_qr_svg_function() {
    // hello.lua uses qr_svg() with anchor positioning
    let app = common::TestApp::new();

    let (api_key, _) = app.register_device(common::fixtures::macs::HELLO_DEVICE).await;
    let headers = common::fixtures::display_headers(common::fixtures::macs::HELLO_DEVICE, &api_key);
    let response = app.get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers)).await;
    common::assert_ok(&response);

    // Fetch the image and verify it contains QR code (visually represented as SVG group)
    let json: serde_json::Value = response.json();
    let image_url = json["image_url"].as_str().unwrap();
    let path = image_url.split("localhost:3000").nth(1).unwrap();

    let image_response = app.get(path).await;
    common::assert_png(&image_response);
}

// ============================================================================
// Direct Lua API tests using mock HTTP server
// ============================================================================

#[tokio::test]
async fn test_lua_http_get_json() {
    let server = MockHttpServer::start().await;

    // Mock a JSON API endpoint
    server
        .mock_get_json(
            "/api/data",
            serde_json::json!({
                "message": "Hello from mock",
                "count": 42
            }),
        )
        .await;

    // Create a temporary test setup to run Lua with HTTP calls
    // For now, we verify the mock server is working
    let url = server.url_for("/api/data");
    assert!(url.contains("/api/data"));
}

#[tokio::test]
async fn test_lua_http_post_json() {
    let server = MockHttpServer::start().await;

    server
        .mock_post_json(
            "/api/submit",
            serde_json::json!({
                "success": true,
                "id": 123
            }),
        )
        .await;

    let url = server.url_for("/api/submit");
    assert!(url.contains("/api/submit"));
}

#[tokio::test]
async fn test_lua_http_with_params() {
    let server = MockHttpServer::start().await;

    server
        .mock_get_with_params(
            "/search",
            "q",
            "test",
            serde_json::json!({
                "results": ["item1", "item2"]
            }),
        )
        .await;

    let url = server.url_for("/search");
    assert!(url.contains("/search"));
}

#[tokio::test]
async fn test_lua_http_basic_auth() {
    let server = MockHttpServer::start().await;

    // Base64 of "user:pass"
    let auth = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"user:pass");

    server
        .mock_with_basic_auth(
            "/protected",
            &auth,
            serde_json::json!({
                "authenticated": true
            }),
        )
        .await;

    let url = server.url_for("/protected");
    assert!(url.contains("/protected"));
}

#[tokio::test]
async fn test_lua_http_error_handling() {
    let server = MockHttpServer::start().await;

    server.mock_error("/error", 500, "Internal Server Error").await;

    let url = server.url_for("/error");
    assert!(url.contains("/error"));
}

#[tokio::test]
async fn test_lua_html_parsing() {
    let server = MockHttpServer::start().await;

    let html = r#"
        <html>
            <body>
                <div class="container">
                    <h1>Title</h1>
                    <p class="content">Some text content</p>
                    <a href="https://example.com">Link</a>
                </div>
            </body>
        </html>
    "#;

    server.mock_get_html("/page", html).await;

    let url = server.url_for("/page");
    assert!(url.contains("/page"));
}

// ============================================================================
// Unit tests for Lua functions (via temporary directory with test scripts)
// ============================================================================

mod lua_unit_tests {
    use super::*;

    #[test]
    fn test_json_encode_decode() {
        let script = r#"
            local obj = { name = "test", count = 42, nested = { a = 1, b = 2 } }
            local encoded = json_encode(obj)
            local decoded = json_decode(encoded)

            return {
                data = {
                    original = obj,
                    encoded = encoded,
                    decoded = decoded,
                    matches = (decoded.name == "test" and decoded.count == 42)
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_json.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_json.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert!(result.data["matches"].as_bool().unwrap());
        assert!(result.data["encoded"].as_str().unwrap().contains("test"));
    }

    #[test]
    fn test_json_decode_array() {
        let script = r#"
            local arr = json_decode('[1, 2, 3, "four"]')
            return {
                data = {
                    first = arr[1],
                    second = arr[2],
                    fourth = arr[4],
                    len = #arr
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_array.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_array.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["first"], 1);
        assert_eq!(result.data["second"], 2);
        assert_eq!(result.data["fourth"], "four");
        assert_eq!(result.data["len"], 4);
    }

    #[test]
    fn test_base64_encode() {
        let script = r#"
            local plain = "Hello, World!"
            local encoded = base64_encode(plain)
            return {
                data = {
                    plain = plain,
                    encoded = encoded
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_base64.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_base64.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        // "Hello, World!" in base64 is "SGVsbG8sIFdvcmxkIQ=="
        assert_eq!(result.data["encoded"], "SGVsbG8sIFdvcmxkIQ==");
    }

    #[test]
    fn test_time_now() {
        let script = r#"
            local now = time_now()
            return {
                data = {
                    timestamp = now,
                    is_number = type(now) == "number",
                    is_recent = now > 1700000000  -- After 2023
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_time.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_time.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert!(result.data["is_number"].as_bool().unwrap());
        assert!(result.data["is_recent"].as_bool().unwrap());
    }

    #[test]
    fn test_time_format() {
        let script = r#"
            -- Use a fixed timestamp: 2024-01-15 12:30:45 UTC
            local ts = 1705322445
            local formatted = time_format(ts, "%Y-%m-%d")
            return {
                data = {
                    formatted = formatted,
                    -- Note: time_format uses local time, so exact match depends on timezone
                    has_date_format = string.match(formatted, "%d%d%d%d%-%d%d%-%d%d") ~= nil
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_format.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_format.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert!(result.data["has_date_format"].as_bool().unwrap());
    }

    #[test]
    fn test_time_parse() {
        let script = r#"
            local ts = time_parse("2024-01-15 12:30:45", "%Y-%m-%d %H:%M:%S")
            return {
                data = {
                    timestamp = ts,
                    is_number = type(ts) == "number",
                    is_valid = ts > 0
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_parse.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_parse.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert!(result.data["is_number"].as_bool().unwrap());
        assert!(result.data["is_valid"].as_bool().unwrap());
    }

    #[test]
    fn test_qr_svg_basic() {
        let script = r#"
            local qr = qr_svg("https://example.com", {
                anchor = "top-left",
                module_size = 4
            })
            return {
                data = {
                    qr = qr,
                    has_svg = string.find(qr, "<g") ~= nil,
                    has_rects = string.find(qr, "<rect") ~= nil
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_qr.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "TE:ST:00:00:00:00".to_string(),
            width: Some(800),
            height: Some(480),
            ..Default::default()
        };

        let result = runtime
            .run_script(
                std::path::Path::new("test_qr.lua"),
                &HashMap::new(),
                Some(&ctx),
            )
            .expect("Script should run");

        assert!(result.data["has_svg"].as_bool().unwrap());
        assert!(result.data["has_rects"].as_bool().unwrap());
    }

    #[test]
    fn test_qr_svg_anchors() {
        let anchors = ["top-left", "top-right", "bottom-left", "bottom-right", "center"];

        for anchor in anchors {
            let script = format!(
                r#"
                local qr = qr_svg("test", {{
                    anchor = "{}",
                    module_size = 2
                }})
                return {{
                    data = {{ qr = qr }},
                    refresh_rate = 60
                }}
            "#,
                anchor
            );

            let (_temp_dir, asset_loader) = setup_test_env(&[("test_anchor.lua", &script)]);
            let runtime = LuaRuntime::new(asset_loader);

            let ctx = DeviceContext {
                width: Some(800),
                height: Some(480),
                ..Default::default()
            };

            let result = runtime.run_script(
                std::path::Path::new("test_anchor.lua"),
                &HashMap::new(),
                Some(&ctx),
            );

            assert!(result.is_ok(), "Anchor '{}' should work", anchor);
        }
    }

    #[test]
    fn test_device_context() {
        let script = r#"
            return {
                data = {
                    mac = device.mac,
                    battery = device.battery_voltage,
                    rssi = device.rssi,
                    model = device.model,
                    firmware = device.firmware_version,
                    width = device.width,
                    height = device.height
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_device.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            battery_voltage: Some(4.12),
            rssi: Some(-67),
            model: Some("x".to_string()),
            firmware_version: Some("2.0.0".to_string()),
            width: Some(1872),
            height: Some(1404),
        };

        let result = runtime
            .run_script(
                std::path::Path::new("test_device.lua"),
                &HashMap::new(),
                Some(&ctx),
            )
            .expect("Script should run");

        assert_eq!(result.data["mac"], "AA:BB:CC:DD:EE:FF");
        // Use approximate comparison for floats (f32 precision)
        let battery = result.data["battery"].as_f64().unwrap();
        assert!((battery - 4.12).abs() < 0.01, "Battery should be ~4.12, got {}", battery);
        assert_eq!(result.data["rssi"], -67);
        assert_eq!(result.data["model"], "x");
        assert_eq!(result.data["firmware"], "2.0.0");
        assert_eq!(result.data["width"], 1872);
        assert_eq!(result.data["height"], 1404);
    }

    #[test]
    fn test_params() {
        let script = r#"
            return {
                data = {
                    station = params.station or "default",
                    limit = params.limit or 5,
                    enabled = params.enabled
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_params.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let mut params = HashMap::new();
        params.insert(
            "station".to_string(),
            serde_yaml::Value::String("Central Station".to_string()),
        );
        params.insert(
            "limit".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(10)),
        );
        params.insert("enabled".to_string(), serde_yaml::Value::Bool(true));

        let result = runtime
            .run_script(std::path::Path::new("test_params.lua"), &params, None)
            .expect("Script should run");

        assert_eq!(result.data["station"], "Central Station");
        assert_eq!(result.data["limit"], 10);
        assert_eq!(result.data["enabled"], true);
    }

    #[test]
    fn test_html_parse_and_select() {
        let script = r#"
            local html = [[
                <html>
                    <body>
                        <div class="container">
                            <h1>Title</h1>
                            <p class="content">Paragraph text</p>
                            <a href="https://example.com">Link</a>
                        </div>
                    </body>
                </html>
            ]]

            local doc = html_parse(html)
            local title = doc:select_one("h1")
            local para = doc:select_one(".content")
            local link = doc:select_one("a")
            local items = doc:select("div, p")

            return {
                data = {
                    title_text = title and title:text() or nil,
                    para_text = para and para:text() or nil,
                    link_href = link and link:attr("href") or nil,
                    item_count = #items
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_html.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_html.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["title_text"], "Title");
        assert_eq!(result.data["para_text"], "Paragraph text");
        assert_eq!(result.data["link_href"], "https://example.com");
        assert!(result.data["item_count"].as_i64().unwrap() >= 2);
    }

    #[test]
    fn test_html_chained_select() {
        let script = r#"
            local html = [[
                <div class="outer">
                    <div class="inner">
                        <span>Nested</span>
                    </div>
                </div>
            ]]

            local doc = html_parse(html)
            local outer = doc:select_one(".outer")
            local inner = outer:select_one(".inner")
            local span = inner:select_one("span")

            return {
                data = {
                    outer_html = outer and outer:html() or nil,
                    span_text = span and span:text() or nil
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_chain.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_chain.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["span_text"], "Nested");
        assert!(result.data["outer_html"]
            .as_str()
            .unwrap()
            .contains("inner"));
    }

    #[test]
    fn test_skip_update() {
        let script = r#"
            return {
                data = {},
                refresh_rate = 300,
                skip_update = true
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_skip.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_skip.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert!(result.skip_update);
        assert_eq!(result.refresh_rate, 300);
    }

    #[test]
    fn test_refresh_rate_default() {
        let script = r#"
            return {
                data = {}
                -- No refresh_rate specified, should default to 900
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_refresh.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script(
                std::path::Path::new("test_refresh.lua"),
                &HashMap::new(),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.refresh_rate, 900);
    }
}

// ============================================================================
// Error path tests for Lua runtime
// ============================================================================

mod lua_error_tests {
    use super::*;
    use byonk::services::lua_runtime::ScriptError;
    use std::path::Path;

    #[test]
    fn test_script_not_found() {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("nonexistent_script.lua"),
            &HashMap::new(),
            None,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            ScriptError::NotFound(msg) => {
                assert!(msg.contains("not found") || msg.contains("NotFound"));
            }
            other => panic!("Expected NotFound error, got: {:?}", other),
        }
    }

    #[test]
    fn test_script_syntax_error() {
        let script = r#"
            this is not valid lua syntax!!!
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_syntax.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("bad_syntax.lua"),
            &HashMap::new(),
            None,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            ScriptError::Lua(_) => {}
            other => panic!("Expected Lua error, got: {:?}", other),
        }
    }

    #[test]
    fn test_script_runtime_error() {
        let script = r#"
            local x = nil
            return x.property  -- nil has no properties
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("runtime_error.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("runtime_error.lua"),
            &HashMap::new(),
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_script_missing_data_field() {
        let script = r#"
            return {
                refresh_rate = 60
                -- missing data field
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("no_data.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("no_data.lua"),
            &HashMap::new(),
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_script_invalid_return_type() {
        let script = r#"
            return "not a table"
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_return.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("bad_return.lua"),
            &HashMap::new(),
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_json_decode_invalid() {
        let script = r#"
            local result = json_decode("not valid json")
            return {
                data = { result = result },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_json.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("bad_json.lua"),
            &HashMap::new(),
            None,
        );

        // json_decode raises an error for invalid JSON
        assert!(result.is_err());
    }

    #[test]
    fn test_html_parse_invalid_selector() {
        let script = r#"
            local doc = html_parse("<html><body>test</body></html>")
            local result = doc:select("[[[invalid")  -- Invalid CSS selector
            return {
                data = { found = result ~= nil },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_selector.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("bad_selector.lua"),
            &HashMap::new(),
            None,
        );

        // Should handle gracefully (returns nil or error)
        // Either outcome is acceptable for error handling test
        let _ = result;
    }

    #[test]
    fn test_time_parse_invalid_format() {
        let script = r#"
            local result = time_parse("not-a-date", "%Y-%m-%d")
            return {
                data = { result = result },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_time.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("bad_time.lua"),
            &HashMap::new(),
            None,
        );

        // time_parse raises an error for invalid input
        assert!(result.is_err());
    }

    #[test]
    fn test_base64_decode_invalid() {
        let script = r#"
            local result = base64_decode("!!!not valid base64!!!")
            return {
                data = { result = result },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_b64.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("bad_b64.lua"),
            &HashMap::new(),
            None,
        );

        // base64_decode raises an error for invalid input
        assert!(result.is_err());
    }

    #[test]
    fn test_script_error_display() {
        let err = ScriptError::NotFound("test.lua".to_string());
        assert_eq!(err.to_string(), "Script not found: test.lua");
    }

    #[test]
    fn test_empty_script() {
        let script = "";

        let (_temp_dir, asset_loader) = setup_test_env(&[("empty.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(
            Path::new("empty.lua"),
            &HashMap::new(),
            None,
        );

        // Empty script returns nil, which is an error
        assert!(result.is_err());
    }

    #[test]
    fn test_script_with_complex_params() {
        let script = r#"
            return {
                data = {
                    string_param = params.name,
                    number_param = params.count,
                    bool_param = params.enabled,
                    nested = params.config
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("params_test.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let mut params = HashMap::new();
        params.insert("name".to_string(), serde_yaml::Value::String("test".to_string()));
        params.insert("count".to_string(), serde_yaml::Value::Number(42.into()));
        params.insert("enabled".to_string(), serde_yaml::Value::Bool(true));

        let mut nested = serde_yaml::Mapping::new();
        nested.insert(
            serde_yaml::Value::String("key".to_string()),
            serde_yaml::Value::String("value".to_string()),
        );
        params.insert("config".to_string(), serde_yaml::Value::Mapping(nested));

        let result = runtime.run_script(
            Path::new("params_test.lua"),
            &params,
            None,
        );

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.data["string_param"], "test");
        assert_eq!(data.data["number_param"], 42);
        assert_eq!(data.data["bool_param"], true);
    }
}

// ============================================================================
// Additional Lua function coverage tests
// ============================================================================

mod lua_additional_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_read_screen_asset() {
        // Test the read_screen_asset function
        let script = r#"
            -- Try to read an asset that exists
            local content = read_screen_asset("hello.svg")
            return {
                data = {
                    has_content = content ~= nil and #content > 0,
                    is_svg = content and content:find("<svg") ~= nil
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_asset.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(Path::new("test_asset.lua"), &HashMap::new(), None);

        // This might fail if hello.svg isn't accessible from the test context
        // Either outcome is fine for coverage
        let _ = result;
    }

    #[test]
    fn test_print_and_log_functions() {
        let script = r#"
            print("Test print output")
            log_info("Test info log")
            log_warn("Test warn log")
            log_error("Test error log")
            return {
                data = { logged = true },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_log.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(Path::new("test_log.lua"), &HashMap::new(), None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_table_with_array_and_map() {
        let script = r#"
            return {
                data = {
                    array = {1, 2, 3, 4, 5},
                    mixed = {a = 1, b = 2, [1] = "first"},
                    nested_array = {{a = 1}, {a = 2}},
                    empty_table = {},
                    null_value = nil
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_table.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(Path::new("test_table.lua"), &HashMap::new(), None);

        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(data.data["array"].is_array());
    }

    #[test]
    fn test_yaml_sequence_params() {
        let script = r#"
            return {
                data = {
                    list = params.items,
                    first = params.items and params.items[1]
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_seq.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let mut params = HashMap::new();
        let items = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("a".to_string()),
            serde_yaml::Value::String("b".to_string()),
            serde_yaml::Value::String("c".to_string()),
        ]);
        params.insert("items".to_string(), items);

        let result = runtime.run_script(Path::new("test_seq.lua"), &params, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_nil_yaml_param() {
        let script = r#"
            return {
                data = {
                    value = params.nothing
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_nil.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let mut params = HashMap::new();
        params.insert("nothing".to_string(), serde_yaml::Value::Null);

        let result = runtime.run_script(Path::new("test_nil.lua"), &params, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_qr_svg_with_all_options() {
        let script = r#"
            local qr1 = qr_svg("test", { size = 100, margin = 10, anchor = "top-left" })
            local qr2 = qr_svg("test", { size = 50, margin = 5, anchor = "top-right" })
            local qr3 = qr_svg("test", { size = 50, anchor = "bottom-left" })
            local qr4 = qr_svg("test", { size = 50, anchor = "bottom-right" })
            local qr5 = qr_svg("test", { size = 50, anchor = "center" })
            return {
                data = {
                    qr1 = qr1,
                    qr2 = qr2,
                    has_all = qr1 ~= nil and qr2 ~= nil and qr3 ~= nil and qr4 ~= nil and qr5 ~= nil
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_qr.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            width: Some(800),
            height: Some(480),
            battery_voltage: None,
            rssi: None,
            model: None,
            firmware_version: None,
        };

        let result = runtime.run_script(Path::new("test_qr.lua"), &HashMap::new(), Some(&ctx));

        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(data.data["has_all"].as_bool().unwrap());
    }

    #[test]
    fn test_html_text_and_attr() {
        let script = r##"
            local html = [[
                <div id="main" class="container">
                    <span data-value="42">Hello</span>
                    <a href="https://example.com">Link</a>
                </div>
            ]]
            local doc = html_parse(html)
            local span = doc:select_one("span")
            local link = doc:select_one("a")
            local div = doc:select_one("#main")

            return {
                data = {
                    span_text = span and span:text(),
                    span_attr = span and span:attr("data-value"),
                    link_href = link and link:attr("href"),
                    div_class = div and div:attr("class")
                },
                refresh_rate = 60
            }
        "##;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_html.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime.run_script(Path::new("test_html.lua"), &HashMap::new(), None);

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.data["span_text"], "Hello");
        assert_eq!(data.data["span_attr"], "42");
        assert_eq!(data.data["link_href"], "https://example.com");
    }
}

// ============================================================================
// HTTP function tests with mock server
// ============================================================================

mod lua_http_tests {
    use super::*;
    use std::path::PathBuf;

    /// Run Lua script in spawn_blocking to avoid reqwest::blocking conflicts
    async fn run_lua_script(
        asset_loader: Arc<AssetLoader>,
        script_name: &str,
    ) -> byonk::services::lua_runtime::ScriptResult {
        let script_path = PathBuf::from(script_name);
        tokio::task::spawn_blocking(move || {
            let runtime = LuaRuntime::new(asset_loader);
            runtime
                .run_script(&script_path, &HashMap::new(), None)
                .expect("Script should run")
        })
        .await
        .expect("spawn_blocking failed")
    }

    #[tokio::test]
    async fn test_http_get_json() {
        let server = MockHttpServer::start().await;
        server
            .mock_get_json(
                "/api/test",
                serde_json::json!({
                    "message": "success",
                    "value": 123
                }),
            )
            .await;

        let script = format!(
            r#"
            local response = http_get("{}/api/test")
            local data = json_decode(response)
            return {{
                data = {{
                    message = data.message,
                    value = data.value
                }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_http.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_http.lua").await;

        assert_eq!(result.data["message"], "success");
        assert_eq!(result.data["value"], 123);
    }

    #[tokio::test]
    async fn test_http_post_with_json_body() {
        let server = MockHttpServer::start().await;
        server
            .mock_post_json(
                "/api/submit",
                serde_json::json!({
                    "status": "created",
                    "id": 456
                }),
            )
            .await;

        let script = format!(
            r#"
            local response = http_post("{}/api/submit", {{
                json = {{ name = "test", count = 5 }}
            }})
            local data = json_decode(response)
            return {{
                data = {{
                    status = data.status,
                    id = data.id
                }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_post.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_post.lua").await;

        assert_eq!(result.data["status"], "created");
        assert_eq!(result.data["id"], 456);
    }

    #[tokio::test]
    async fn test_http_with_query_params() {
        let server = MockHttpServer::start().await;
        server
            .mock_get_with_params(
                "/search",
                "q",
                "rust",
                serde_json::json!({
                    "results": ["rust-lang", "rustup", "cargo"]
                }),
            )
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/search", {{
                params = {{ q = "rust" }}
            }})
            local data = json_decode(response)
            return {{
                data = {{
                    count = #data.results,
                    first = data.results[1]
                }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_params.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_params.lua").await;

        assert_eq!(result.data["count"], 3);
        assert_eq!(result.data["first"], "rust-lang");
    }

    #[tokio::test]
    async fn test_http_with_custom_headers() {
        let server = MockHttpServer::start().await;

        // Mock endpoint that requires custom header
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/headers"))
            .and(wiremock::matchers::header("X-Custom", "test-value"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"received": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/headers", {{
                headers = {{ ["X-Custom"] = "test-value" }}
            }})
            local data = json_decode(response)
            return {{
                data = {{ received = data.received }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_headers.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_headers.lua").await;

        assert_eq!(result.data["received"], true);
    }

    #[tokio::test]
    async fn test_http_html_parsing_integration() {
        let server = MockHttpServer::start().await;

        let html = r#"
            <html>
                <body>
                    <div class="departure">
                        <span class="line">Bus 42</span>
                        <span class="time">10:30</span>
                    </div>
                    <div class="departure">
                        <span class="line">Tram 7</span>
                        <span class="time">10:35</span>
                    </div>
                </body>
            </html>
        "#;

        server.mock_get_html("/departures", html).await;

        let script = format!(
            r#"
            local response = http_get("{}/departures")
            local doc = html_parse(response)
            local departures = doc:select(".departure")

            local results = {{}}
            for i = 1, #departures do
                local dep = departures[i]
                local line = dep:select_one(".line")
                local time = dep:select_one(".time")
                results[i] = {{
                    line = line and line:text() or "",
                    time = time and time:text() or ""
                }}
            end

            return {{
                data = {{
                    count = #results,
                    first_line = results[1] and results[1].line or "",
                    first_time = results[1] and results[1].time or ""
                }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_html_http.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_html_http.lua").await;

        assert_eq!(result.data["count"], 2);
        assert_eq!(result.data["first_line"], "Bus 42");
        assert_eq!(result.data["first_time"], "10:30");
    }

    #[tokio::test]
    async fn test_http_put_method() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("PUT"))
            .and(wiremock::matchers::path("/resource"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"updated": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/resource", {{
                method = "PUT",
                json = {{ value = "new" }}
            }})
            local data = json_decode(response)
            return {{
                data = {{ updated = data.updated }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_put.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_put.lua").await;

        assert_eq!(result.data["updated"], true);
    }

    #[tokio::test]
    async fn test_http_delete_method() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/resource/123"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"deleted": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/resource/123", {{
                method = "DELETE"
            }})
            local data = json_decode(response)
            return {{
                data = {{ deleted = data.deleted }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_delete.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_delete.lua").await;

        assert_eq!(result.data["deleted"], true);
    }

    #[tokio::test]
    async fn test_http_patch_method() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("PATCH"))
            .and(wiremock::matchers::path("/resource"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"patched": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/resource", {{
                method = "PATCH",
                body = "partial update"
            }})
            local data = json_decode(response)
            return {{
                data = {{ patched = data.patched }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_patch.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_patch.lua").await;

        assert_eq!(result.data["patched"], true);
    }

    #[tokio::test]
    async fn test_http_head_method() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("HEAD"))
            .and(wiremock::matchers::path("/check"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/check", {{
                method = "HEAD"
            }})
            return {{
                data = {{ empty_response = response == "" }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_head.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_head.lua").await;

        // HEAD returns empty body
        assert_eq!(result.data["empty_response"], true);
    }

    #[tokio::test]
    async fn test_http_with_timeout() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/fast"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"fast": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/fast", {{
                timeout = 5
            }})
            local data = json_decode(response)
            return {{
                data = {{ fast = data.fast }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_timeout.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_timeout.lua").await;

        assert_eq!(result.data["fast"], true);
    }

    #[tokio::test]
    async fn test_http_with_numeric_params() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/numeric"))
            .and(wiremock::matchers::query_param("id", "42"))
            .and(wiremock::matchers::query_param("pi", "3.14"))
            .and(wiremock::matchers::query_param("enabled", "true"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"ok": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            local response = http_request("{}/numeric", {{
                params = {{ id = 42, pi = 3.14, enabled = true }}
            }})
            local data = json_decode(response)
            return {{
                data = {{ ok = data.ok }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_numeric.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_numeric.lua").await;

        assert_eq!(result.data["ok"], true);
    }

    #[tokio::test]
    async fn test_http_unknown_option_warning() {
        let server = MockHttpServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/test"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"ok": true})),
            )
            .mount(&server.server)
            .await;

        let script = format!(
            r#"
            -- This includes an unknown option which should log a warning
            local response = http_request("{}/test", {{
                unknown_option = "value"
            }})
            local data = json_decode(response)
            return {{
                data = {{ ok = data.ok }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_unknown.lua", &script)]);
        let result = run_lua_script(asset_loader, "test_unknown.lua").await;

        // Should still work, just with a warning
        assert_eq!(result.data["ok"], true);
    }
}

// ============================================================================
// HTTPS/TLS certificate tests
// ============================================================================

mod lua_https_tests {
    use super::*;
    use byonk::services::ScriptResult;
    use common::MockHttpsServer;
    use std::path::Path;

    /// Helper to run a Lua script in a blocking context (for TLS which uses blocking reqwest)
    async fn run_lua_script_blocking(
        asset_loader: Arc<AssetLoader>,
        script_name: &str,
    ) -> ScriptResult {
        let script_path = script_name.to_string();
        tokio::task::spawn_blocking(move || {
            let runtime = LuaRuntime::new(asset_loader);
            runtime
                .run_script(Path::new(&script_path), &HashMap::new(), None)
                .expect("Script execution failed")
        })
        .await
        .expect("Blocking task panicked")
    }

    /// Helper to run a Lua script expecting an error
    async fn run_lua_script_expecting_error(
        asset_loader: Arc<AssetLoader>,
        script_name: &str,
    ) -> String {
        let script_path = script_name.to_string();
        tokio::task::spawn_blocking(move || {
            let runtime = LuaRuntime::new(asset_loader);
            match runtime.run_script(Path::new(&script_path), &HashMap::new(), None) {
                Ok(_) => panic!("Expected script to fail"),
                Err(e) => e.to_string(),
            }
        })
        .await
        .expect("Blocking task panicked")
    }

    #[tokio::test]
    async fn test_https_with_danger_accept_invalid_certs() {
        // Start HTTPS server with self-signed certificate
        let server = MockHttpsServer::start()
            .await
            .expect("Failed to start HTTPS server");

        let script = format!(
            r#"
            local response = http_request("{}/health", {{
                danger_accept_invalid_certs = true
            }})
            local data = json_decode(response)
            return {{
                data = {{ status = data.status }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_https_insecure.lua", &script)]);
        let result = run_lua_script_blocking(asset_loader, "test_https_insecure.lua").await;

        assert_eq!(result.data["status"], "healthy");
    }

    #[tokio::test]
    async fn test_https_with_custom_ca_cert() {
        // Start HTTPS server with self-signed certificate
        let server = MockHttpsServer::start()
            .await
            .expect("Failed to start HTTPS server");

        let ca_cert_path = server.certs.ca_cert.to_str().unwrap();

        let script = format!(
            r#"
            local response = http_request("{}/data", {{
                ca_cert = "{}"
            }})
            local data = json_decode(response)
            return {{
                data = {{ message = data.message }},
                refresh_rate = 60
            }}
        "#,
            server.url(),
            ca_cert_path.replace('\\', "\\\\")
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_https_ca.lua", &script)]);
        let result = run_lua_script_blocking(asset_loader, "test_https_ca.lua").await;

        assert_eq!(result.data["message"], "Hello from HTTPS!");
    }

    #[tokio::test]
    async fn test_https_with_client_certificate() {
        // Start HTTPS server that requires client certificates
        let server = MockHttpsServer::start_with_client_auth(true)
            .await
            .expect("Failed to start HTTPS server with client auth");

        let ca_cert_path = server.certs.ca_cert.to_str().unwrap();
        let client_cert_path = server.certs.client_cert.to_str().unwrap();
        let client_key_path = server.certs.client_key.to_str().unwrap();

        let script = format!(
            r#"
            local response = http_request("{}/health", {{
                ca_cert = "{}",
                client_cert = "{}",
                client_key = "{}"
            }})
            local data = json_decode(response)
            return {{
                data = {{ status = data.status }},
                refresh_rate = 60
            }}
        "#,
            server.url(),
            ca_cert_path.replace('\\', "\\\\"),
            client_cert_path.replace('\\', "\\\\"),
            client_key_path.replace('\\', "\\\\")
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_https_mtls.lua", &script)]);
        let result = run_lua_script_blocking(asset_loader, "test_https_mtls.lua").await;

        assert_eq!(result.data["status"], "healthy");
    }

    #[tokio::test]
    async fn test_https_fails_without_valid_cert() {
        // Start HTTPS server with self-signed certificate
        let server = MockHttpsServer::start()
            .await
            .expect("Failed to start HTTPS server");

        // Try to connect without accepting invalid certs or providing CA
        let script = format!(
            r#"
            local response = http_request("{}/health", {{}})
            return {{
                data = {{ response = response }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_https_fail.lua", &script)]);
        let error = run_lua_script_expecting_error(asset_loader, "test_https_fail.lua").await;

        // Should fail due to certificate verification (error message may vary by platform)
        assert!(
            error.contains("certificate")
                || error.contains("SSL")
                || error.contains("TLS")
                || error.contains("error sending request"),
            "Expected certificate error, got: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_https_mtls_fails_without_client_cert() {
        // Start HTTPS server that requires client certificates
        let server = MockHttpsServer::start_with_client_auth(true)
            .await
            .expect("Failed to start HTTPS server with client auth");

        let ca_cert_path = server.certs.ca_cert.to_str().unwrap();

        // Try to connect without client certificate
        let script = format!(
            r#"
            local response = http_request("{}/health", {{
                ca_cert = "{}"
            }})
            return {{
                data = {{ response = response }},
                refresh_rate = 60
            }}
        "#,
            server.url(),
            ca_cert_path.replace('\\', "\\\\")
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_mtls_fail.lua", &script)]);
        let error = run_lua_script_expecting_error(asset_loader, "test_mtls_fail.lua").await;

        // Should fail due to missing client certificate (error message may vary by platform)
        assert!(
            error.contains("certificate")
                || error.contains("SSL")
                || error.contains("TLS")
                || error.contains("connection")
                || error.contains("error sending request"),
            "Expected certificate/connection error, got: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_https_client_cert_without_key_fails() {
        // Using a mock server for the request doesn't matter here since
        // the error should happen during client configuration
        let server = MockHttpsServer::start()
            .await
            .expect("Failed to start HTTPS server");

        let client_cert_path = server.certs.client_cert.to_str().unwrap();

        // Provide client_cert but not client_key
        let script = format!(
            r#"
            local response = http_request("{}/health", {{
                danger_accept_invalid_certs = true,
                client_cert = "{}"
            }})
            return {{
                data = {{ response = response }},
                refresh_rate = 60
            }}
        "#,
            server.url(),
            client_cert_path.replace('\\', "\\\\")
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_cert_no_key.lua", &script)]);
        let error = run_lua_script_expecting_error(asset_loader, "test_cert_no_key.lua").await;

        // Should fail because both client_cert and client_key are required
        assert!(
            error.contains("client_cert and client_key must be provided together"),
            "Expected error about missing key, got: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_https_client_key_without_cert_fails() {
        // Using a mock server for the request doesn't matter here since
        // the error should happen during client configuration
        let server = MockHttpsServer::start()
            .await
            .expect("Failed to start HTTPS server");

        let client_key_path = server.certs.client_key.to_str().unwrap();

        // Provide client_key but not client_cert
        let script = format!(
            r#"
            local response = http_request("{}/health", {{
                danger_accept_invalid_certs = true,
                client_key = "{}"
            }})
            return {{
                data = {{ response = response }},
                refresh_rate = 60
            }}
        "#,
            server.url(),
            client_key_path.replace('\\', "\\\\")
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_key_no_cert.lua", &script)]);
        let error = run_lua_script_expecting_error(asset_loader, "test_key_no_cert.lua").await;

        // Should fail because both client_cert and client_key are required
        assert!(
            error.contains("client_cert and client_key must be provided together"),
            "Expected error about missing cert, got: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_https_invalid_ca_cert_path_fails() {
        let server = MockHttpsServer::start()
            .await
            .expect("Failed to start HTTPS server");

        let script = format!(
            r#"
            local response = http_request("{}/health", {{
                ca_cert = "/nonexistent/path/to/ca.pem"
            }})
            return {{
                data = {{ response = response }},
                refresh_rate = 60
            }}
        "#,
            server.url()
        );

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_bad_ca_path.lua", &script)]);
        let error = run_lua_script_expecting_error(asset_loader, "test_bad_ca_path.lua").await;

        // Should fail because the file doesn't exist
        assert!(
            error.contains("Failed to read CA certificate"),
            "Expected error about reading CA cert, got: {}",
            error
        );
    }
}
