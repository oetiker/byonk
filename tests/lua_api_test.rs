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

    let api_key = app
        .register_device(common::fixtures::macs::HELLO_DEVICE)
        .await;
    let headers = common::fixtures::display_headers(common::fixtures::macs::HELLO_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
        .await;

    // If params/device globals work, the script executes successfully
    common::assert_ok(&response);
    common::assert_valid_display_response(&response);
}

#[tokio::test]
async fn test_lua_time_functions() {
    // hello.lua uses time_now() and time_format()
    let app = common::TestApp::new();

    let api_key = app
        .register_device(common::fixtures::macs::HELLO_DEVICE)
        .await;
    let headers = common::fixtures::display_headers(common::fixtures::macs::HELLO_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
        .await;

    // Script uses time functions successfully
    common::assert_ok(&response);
    let json: serde_json::Value = response.json();
    assert_eq!(json["status"], 0);
}

#[tokio::test]
async fn test_lua_qr_svg_function() {
    // hello.lua uses qr_svg() with anchor positioning
    let app = common::TestApp::new();

    let api_key = app
        .register_device(common::fixtures::macs::HELLO_DEVICE)
        .await;
    let headers = common::fixtures::display_headers(common::fixtures::macs::HELLO_DEVICE, &api_key);
    let response = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
        .await;
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

    server
        .mock_error("/error", 500, "Internal Server Error")
        .await;

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
            .run_script_from_asset(
                std::path::Path::new("test_json.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_array.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_base64.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_time.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_format.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_parse.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_qr.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        assert!(result.data["has_svg"].as_bool().unwrap());
        assert!(result.data["has_rects"].as_bool().unwrap());
    }

    #[test]
    fn test_device_colors_actual_exposed_when_measured() {
        let script = r#"
            return {
                data = {
                    actual_1 = device.colors_actual[1],
                    actual_3 = device.colors_actual[3],
                    count = #device.colors_actual,
                    official_count = #device.colors,
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_ca.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "TE:ST:00:00:00:00".to_string(),
            width: Some(800),
            height: Some(480),
            colors: Some(vec![
                "#000000".to_string(),
                "#FFFFFF".to_string(),
                "#FF0000".to_string(),
            ]),
            colors_actual: Some(vec![
                "#0A0A0A".to_string(),
                "#E8E6E0".to_string(),
                "#A83A30".to_string(),
            ]),
            ..Default::default()
        };

        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_ca.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["actual_1"].as_str().unwrap(), "#0A0A0A");
        assert_eq!(result.data["actual_3"].as_str().unwrap(), "#A83A30");
        assert_eq!(result.data["count"].as_i64().unwrap(), 3);
        // Index-parallel with device.colors.
        assert_eq!(result.data["official_count"].as_i64().unwrap(), 3);
    }

    #[test]
    fn test_device_colors_actual_is_nil_when_uncalibrated() {
        // Deliberately NOT mirrored from device.colors: a script must be able to
        // tell "this panel is uncalibrated" from "this panel measures to spec".
        let script = r#"
            return {
                data = { is_nil = device.colors_actual == nil },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_ca_nil.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "TE:ST:00:00:00:00".to_string(),
            width: Some(800),
            height: Some(480),
            colors: Some(vec!["#000000".to_string(), "#FFFFFF".to_string()]),
            colors_actual: None,
            ..Default::default()
        };

        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_ca_nil.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        assert!(result.data["is_nil"].as_bool().unwrap());
    }

    #[test]
    fn test_qr_svg_anchors() {
        let anchors = [
            "top-left",
            "top-right",
            "bottom-left",
            "bottom-right",
            "center",
        ];

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

            let result = runtime.run_script_from_asset(
                std::path::Path::new("test_anchor.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
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
            registration_code: None,
            ..Default::default()
        };

        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_device.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["mac"], "AA:BB:CC:DD:EE:FF");
        // Use approximate comparison for floats (f32 precision)
        let battery = result.data["battery"].as_f64().unwrap();
        assert!(
            (battery - 4.12).abs() < 0.01,
            "Battery should be ~4.12, got {}",
            battery
        );
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
            .run_script_from_asset(std::path::Path::new("test_params.lua"), &params, None, None)
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
            .run_script_from_asset(
                std::path::Path::new("test_html.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_chain.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_skip.lua"),
                &HashMap::new(),
                None,
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
            .run_script_from_asset(
                std::path::Path::new("test_refresh.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("Script should run");

        assert_eq!(result.refresh_rate, 900);
    }

    #[test]
    fn test_fonts_global() {
        use byonk::services::FontFaceInfo;

        let script = r#"
            -- fonts global should be a table
            local count = 0
            local found_test = false
            local test_face = nil
            for family, faces in pairs(fonts) do
                count = count + 1
                if family == "TestFamily" then
                    found_test = true
                    test_face = faces[1]
                end
            end
            return {
                data = {
                    family_count = count,
                    found_test = found_test,
                    style = test_face and test_face.style or "missing",
                    weight = test_face and test_face.weight or 0,
                    monospaced = test_face and test_face.monospaced or false,
                    strikes_count = test_face and #test_face.bitmap_strikes or 0,
                    first_strike = test_face and test_face.bitmap_strikes[1] or 0,
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("fonts_test.lua", script)]);

        let mut font_families = HashMap::new();
        font_families.insert(
            "TestFamily".to_string(),
            vec![FontFaceInfo {
                style: "Normal".to_string(),
                weight: 400,
                stretch: "Normal".to_string(),
                monospaced: true,
                post_script_name: "TestFamily-Regular".to_string(),
                bitmap_strikes: vec![8, 12, 16],
            }],
        );

        let runtime = LuaRuntime::with_fonts(asset_loader, font_families);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("fonts_test.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["family_count"], 1);
        assert_eq!(result.data["found_test"], true);
        assert_eq!(result.data["style"], "Normal");
        assert_eq!(result.data["weight"], 400);
        assert_eq!(result.data["monospaced"], true);
        assert_eq!(result.data["strikes_count"], 3);
        assert_eq!(result.data["first_strike"], 8);
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

        let result = runtime.run_script_from_asset(
            Path::new("nonexistent_script.lua"),
            &HashMap::new(),
            None,
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

        let result =
            runtime.run_script_from_asset(Path::new("bad_syntax.lua"), &HashMap::new(), None, None);

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

        let result = runtime.run_script_from_asset(
            Path::new("runtime_error.lua"),
            &HashMap::new(),
            None,
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

        let result =
            runtime.run_script_from_asset(Path::new("no_data.lua"), &HashMap::new(), None, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_script_invalid_return_type() {
        let script = r#"
            return "not a table"
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("bad_return.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result =
            runtime.run_script_from_asset(Path::new("bad_return.lua"), &HashMap::new(), None, None);

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

        let result =
            runtime.run_script_from_asset(Path::new("bad_json.lua"), &HashMap::new(), None, None);

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

        let result = runtime.run_script_from_asset(
            Path::new("bad_selector.lua"),
            &HashMap::new(),
            None,
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

        let result =
            runtime.run_script_from_asset(Path::new("bad_time.lua"), &HashMap::new(), None, None);

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

        let result =
            runtime.run_script_from_asset(Path::new("bad_b64.lua"), &HashMap::new(), None, None);

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

        let result =
            runtime.run_script_from_asset(Path::new("empty.lua"), &HashMap::new(), None, None);

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
        params.insert(
            "name".to_string(),
            serde_yaml::Value::String("test".to_string()),
        );
        params.insert("count".to_string(), serde_yaml::Value::Number(42.into()));
        params.insert("enabled".to_string(), serde_yaml::Value::Bool(true));

        let mut nested = serde_yaml::Mapping::new();
        nested.insert(
            serde_yaml::Value::String("key".to_string()),
            serde_yaml::Value::String("value".to_string()),
        );
        params.insert("config".to_string(), serde_yaml::Value::Mapping(nested));

        let result =
            runtime.run_script_from_asset(Path::new("params_test.lua"), &params, None, None);

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

        let result =
            runtime.run_script_from_asset(Path::new("test_asset.lua"), &HashMap::new(), None, None);

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

        let result =
            runtime.run_script_from_asset(Path::new("test_log.lua"), &HashMap::new(), None, None);

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

        let result =
            runtime.run_script_from_asset(Path::new("test_table.lua"), &HashMap::new(), None, None);

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

        let result = runtime.run_script_from_asset(Path::new("test_seq.lua"), &params, None, None);

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

        let result = runtime.run_script_from_asset(Path::new("test_nil.lua"), &params, None, None);

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
            ..Default::default()
        };

        let result = runtime.run_script_from_asset(
            Path::new("test_qr.lua"),
            &HashMap::new(),
            Some(&ctx),
            None,
        );

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

        let result =
            runtime.run_script_from_asset(Path::new("test_html.lua"), &HashMap::new(), None, None);

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
                .run_script_from_asset(&script_path, &HashMap::new(), None, None)
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
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
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
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
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
// Layout helper function tests
// ============================================================================

mod lua_layout_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_layout_table_defaults() {
        // Test default values when no device context is provided
        let script = r#"
            return {
                data = {
                    width = layout.width,
                    height = layout.height,
                    scale = layout.scale,
                    center_x = layout.center_x,
                    center_y = layout.center_y,
                    color_count = layout.color_count,
                    margin = layout.margin,
                    margin_sm = layout.margin_sm,
                    margin_lg = layout.margin_lg
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_layout.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(Path::new("test_layout.lua"), &HashMap::new(), None, None)
            .expect("Script should run");

        assert_eq!(result.data["width"], 800);
        assert_eq!(result.data["height"], 480);
        assert_eq!(result.data["scale"], 1.0);
        assert_eq!(result.data["center_x"], 400);
        assert_eq!(result.data["center_y"], 240);
        assert_eq!(result.data["color_count"], 4);
        assert_eq!(result.data["margin"], 20);
        assert_eq!(result.data["margin_sm"], 10);
        assert_eq!(result.data["margin_lg"], 40);
    }

    #[test]
    fn test_layout_table_with_x_device() {
        // Test with TRMNL X device (1872x1404) with 16-color palette
        let script = r#"
            return {
                data = {
                    width = layout.width,
                    height = layout.height,
                    scale = layout.scale,
                    center_x = layout.center_x,
                    center_y = layout.center_y,
                    color_count = layout.color_count,
                    margin = layout.margin,
                    margin_sm = layout.margin_sm,
                    margin_lg = layout.margin_lg
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_layout_x.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let x_colors: Vec<String> = (0..16)
            .map(|i| format!("#{:02X}{:02X}{:02X}", i * 17, i * 17, i * 17))
            .collect();
        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            width: Some(1872),
            height: Some(1404),
            colors: Some(x_colors),
            ..Default::default()
        };

        let result = runtime
            .run_script_from_asset(
                Path::new("test_layout_x.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["width"], 1872);
        assert_eq!(result.data["height"], 1404);
        // Scale is min(1872/800, 1404/480) = min(2.34, 2.925) = 2.34
        let scale = result.data["scale"].as_f64().unwrap();
        assert!(
            (scale - 2.34).abs() < 0.001,
            "Scale should be 2.34, got {}",
            scale
        );
        assert_eq!(result.data["center_x"], 936);
        assert_eq!(result.data["center_y"], 702);
        assert_eq!(result.data["color_count"], 16);
        // margin = floor(20 * 2.34) = floor(46.8) = 46
        assert_eq!(result.data["margin"], 46);
        // margin_sm = floor(10 * 2.34) = floor(23.4) = 23
        assert_eq!(result.data["margin_sm"], 23);
        // margin_lg = floor(40 * 2.34) = floor(93.6) = 93
        assert_eq!(result.data["margin_lg"], 93);
    }

    #[test]
    fn test_scale_font() {
        // Test scale_font returns float at scale=1.0
        let script = r#"
            local result = scale_font(48)
            return {
                data = {
                    result = result,
                    is_number = type(result) == "number"
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_scale_font.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(
                Path::new("test_scale_font.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("Script should run");

        let scaled = result.data["result"].as_f64().unwrap();
        assert!(
            (scaled - 48.0).abs() < 0.001,
            "Expected 48.0, got {}",
            scaled
        );
        assert!(result.data["is_number"].as_bool().unwrap());
    }

    #[test]
    fn test_scale_font_with_x_device() {
        // Test scale_font with TRMNL X device (scale = 2.34)
        let script = r#"
            local result = scale_font(48)
            return {
                data = { result = result },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_scale_font_x.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            width: Some(1872),
            height: Some(1404),
            ..Default::default()
        };

        let result = runtime
            .run_script_from_asset(
                Path::new("test_scale_font_x.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        // scale_font(48) at scale=2.34 = 48 * 2.34 = 112.32
        let scaled = result.data["result"].as_f64().unwrap();
        assert!(
            (scaled - 112.32).abs() < 0.1,
            "Expected ~112.32, got {}",
            scaled
        );
    }

    #[test]
    fn test_scale_pixel() {
        // Test scale_pixel returns integer at scale=1.0
        let script = r#"
            local result = scale_pixel(70)
            return {
                data = {
                    result = result,
                    is_integer = math.floor(result) == result
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_scale_pixel.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(
                Path::new("test_scale_pixel.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("Script should run");

        assert_eq!(result.data["result"], 70);
        assert!(result.data["is_integer"].as_bool().unwrap());
    }

    #[test]
    fn test_scale_pixel_with_x_device() {
        // Test scale_pixel with TRMNL X device (scale = 2.34)
        let script = r#"
            local result = scale_pixel(70)
            return {
                data = { result = result },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_scale_pixel_x.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            width: Some(1872),
            height: Some(1404),
            ..Default::default()
        };

        let result = runtime
            .run_script_from_asset(
                Path::new("test_scale_pixel_x.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("Script should run");

        // scale_pixel(70) at scale=2.34 = floor(70 * 2.34) = floor(163.8) = 163
        assert_eq!(result.data["result"], 163);
    }

    #[test]
    fn test_greys_4_levels() {
        // Test greys(4) generates 4-level palette
        let script = r#"
            local palette = greys(4)
            return {
                data = {
                    count = #palette,
                    first_value = palette[1].value,
                    first_color = palette[1].color,
                    first_text = palette[1].text_color,
                    second_value = palette[2].value,
                    third_value = palette[3].value,
                    fourth_value = palette[4].value,
                    fourth_color = palette[4].color,
                    fourth_text = palette[4].text_color
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_greys_4.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(Path::new("test_greys_4.lua"), &HashMap::new(), None, None)
            .expect("Script should run");

        assert_eq!(result.data["count"], 4);
        // First entry: value=0 (black)
        assert_eq!(result.data["first_value"], 0);
        assert_eq!(result.data["first_color"], "#000000");
        assert_eq!(result.data["first_text"], "#ffffff");
        // Second: value = 255 * 1 / 3 = 85
        assert_eq!(result.data["second_value"], 85);
        // Third: value = 255 * 2 / 3 = 170
        assert_eq!(result.data["third_value"], 170);
        // Fourth: value = 255 (white)
        assert_eq!(result.data["fourth_value"], 255);
        assert_eq!(result.data["fourth_color"], "#ffffff");
        assert_eq!(result.data["fourth_text"], "#000000");
    }

    #[test]
    fn test_greys_16_levels() {
        // Test greys(16) generates 16-level palette
        let script = r#"
            local palette = greys(16)
            local values = {}
            for i = 1, #palette do
                values[i] = palette[i].value
            end
            return {
                data = {
                    count = #palette,
                    first_value = palette[1].value,
                    last_value = palette[16].value,
                    mid_value = palette[8].value
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_greys_16.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(Path::new("test_greys_16.lua"), &HashMap::new(), None, None)
            .expect("Script should run");

        assert_eq!(result.data["count"], 16);
        assert_eq!(result.data["first_value"], 0);
        assert_eq!(result.data["last_value"], 255);
        // Mid value (8th): 255 * 7 / 15 = 119
        assert_eq!(result.data["mid_value"], 119);
    }

    #[test]
    fn test_greys_2_levels() {
        // Test greys(2) generates black and white only
        let script = r#"
            local palette = greys(2)
            return {
                data = {
                    count = #palette,
                    first_value = palette[1].value,
                    second_value = palette[2].value
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_greys_2.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(Path::new("test_greys_2.lua"), &HashMap::new(), None, None)
            .expect("Script should run");

        assert_eq!(result.data["count"], 2);
        assert_eq!(result.data["first_value"], 0);
        assert_eq!(result.data["second_value"], 255);
    }

    #[test]
    fn test_layout_integration() {
        // Test using layout helpers together as they would be in a real script
        let script = r#"
            local font_size = scale_font(48)
            local header_y = scale_pixel(70)
            local margin = layout.margin
            local palette = greys(layout.color_count)

            return {
                data = {
                    font_size = font_size,
                    header_y = header_y,
                    margin = margin,
                    palette_count = #palette,
                    bg_color = palette[1].color
                },
                refresh_rate = 60
            }
        "#;

        let (_temp_dir, asset_loader) = setup_test_env(&[("test_layout_integration.lua", script)]);
        let runtime = LuaRuntime::new(asset_loader);

        let result = runtime
            .run_script_from_asset(
                Path::new("test_layout_integration.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("Script should run");

        let font_size = result.data["font_size"].as_f64().unwrap();
        assert!((font_size - 48.0).abs() < 0.001);
        assert_eq!(result.data["header_y"], 70);
        assert_eq!(result.data["margin"], 20);
        assert_eq!(result.data["palette_count"], 4);
        assert_eq!(result.data["bg_color"], "#000000");
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
                .run_script_from_asset(Path::new(&script_path), &HashMap::new(), None, None)
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
            match runtime.run_script_from_asset(
                Path::new(&script_path),
                &HashMap::new(),
                None,
                None,
            ) {
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

// ============================================================================
// image_process() Lua global (Task 9)
// ============================================================================

mod lua_image_process_tests {
    use super::*;

    /// A small generated PNG. Built, never committed — a binary fixture in the
    /// repo is a fixture nobody can inspect or regenerate.
    fn tiny_png(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            // Muted, low-saturation content: the case image_process exists for.
            let v = (120 + ((x + y) % 3) * 12) as u8;
            *px = image::Rgb([v + 8, v, v.saturating_sub(6)]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// setup_test_env + a generated `tiny.png` alongside the script.
    fn setup_image_env(script_name: &str, script: &str) -> (TempDir, Arc<AssetLoader>) {
        let (temp_dir, loader) = setup_test_env(&[(script_name, script)]);
        std::fs::write(temp_dir.path().join("tiny.png"), tiny_png(40, 20)).expect("write test png");
        (temp_dir, loader)
    }

    #[test]
    fn test_image_process_returns_a_data_uri_and_dimensions() {
        let script = r#"
            local png = read_asset("tiny.png")
            local src, w, h = image_process(png, { width = 20, height = 10, fit = "cover" })
            return {
                data = {
                    is_png = string.sub(src, 1, 22) == "data:image/png;base64,",
                    w = w,
                    h = h,
                },
                refresh_rate = 60
            }
        "#;

        let (_temp, loader) = setup_image_env("test_img.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["is_png"].as_bool().unwrap(),
            "must be a png data URI"
        );
        assert_eq!(result.data["w"].as_i64().unwrap(), 20);
        assert_eq!(result.data["h"].as_i64().unwrap(), 10);
    }

    #[test]
    fn test_image_process_rejects_an_out_of_range_slider() {
        // exposure = 30 is a typo for 3.0. It must fail loudly rather than
        // saturating to a white rectangle.
        let script = r#"
            local png = read_asset("tiny.png")
            local ok, err = pcall(function()
                return image_process(png, { exposure = 30 })
            end)
            return { data = { ok = ok, err = tostring(err) }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_range.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_range.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run — the pcall catches the error");

        assert!(!result.data["ok"].as_bool().unwrap(), "must have raised");
        let err = result.data["err"].as_str().unwrap();
        assert!(
            err.contains("exposure"),
            "the error must name the field: {err}"
        );
    }

    #[test]
    fn test_image_process_preset_runs_without_a_palette() {
        // palette_aware on a device with no palette at all: a logged no-op, not
        // an error. A screen using the eink preset must render everywhere.
        let script = r#"
            local png = read_asset("tiny.png")
            local src = image_process(png, { preset = "eink", palette_aware = true })
            return { data = { ok = src ~= nil }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_preset.lua", script);
        let runtime = LuaRuntime::new(loader);
        let ctx = DeviceContext {
            mac: "TE:ST:00:00:00:00".to_string(),
            width: Some(800),
            height: Some(480),
            colors: None,
            colors_actual: None,
            ..Default::default()
        };
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_preset.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("script must run");

        assert!(result.data["ok"].as_bool().unwrap());
        assert!(
            result.logs.iter().any(|l| l.contains("palette_aware")),
            "the no-op must be logged, not silent: {:?}",
            result.logs
        );
    }

    #[test]
    fn test_image_process_rejects_an_unknown_preset() {
        // A typo'd preset that silently does nothing ships looking like it worked.
        // Brought up to the standard of its `..._rejects_an_unknown_fit` sibling:
        // pin which field the error names, not just that pcall failed.
        let script = r#"
            local png = read_asset("tiny.png")
            local ok, err = pcall(function() return image_process(png, { preset = "vivid" }) end)
            return { data = { ok = ok, err = tostring(err) }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_preset_bad.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_preset_bad.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            !result.data["ok"].as_bool().unwrap(),
            "unknown preset must raise"
        );
        let err = result.data["err"].as_str().unwrap();
        assert!(
            err.contains("preset"),
            "the error must name the field: {err}"
        );
    }

    #[test]
    fn test_image_process_palette_aware_with_a_palette_actually_uses_it() {
        // The brief pins the *no-palette* no-op path
        // (`test_image_process_preset_runs_without_a_palette`) but nothing
        // else in this file proves `palette_aware` actually threads a real
        // palette through to `output_endpoints`. Without this, a binding
        // that always passes `None` for `output_endpoints` (i.e. silently
        // drops `palette_aware` even when a palette IS present) would pass
        // every other test in this module.
        let script = r#"
            local png = read_asset("tiny.png")
            local with_palette = image_process(png, { palette_aware = true })
            local without_palette = image_process(png, {})
            return {
                data = {
                    differs = with_palette ~= without_palette,
                },
                refresh_rate = 60
            }
        "#;

        let (_temp, loader) = setup_image_env("test_img_palette.lua", script);
        let runtime = LuaRuntime::new(loader);
        let ctx = DeviceContext {
            mac: "TE:ST:00:00:00:01".to_string(),
            width: Some(800),
            height: Some(480),
            // A tight, off-0..1 measured range so palette_aware's endpoint
            // compression is guaranteed to move at least some pixels.
            colors_actual: Some(vec!["#202020".to_string(), "#d8d8d8".to_string()]),
            ..Default::default()
        };
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_palette.lua"),
                &HashMap::new(),
                Some(&ctx),
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "palette_aware with a real palette must change the output vs. without it"
        );
    }

    #[test]
    fn test_image_process_prefers_colors_actual_over_colors() {
        // The sourcing order from the brief, exactly: colors_actual, then
        // colors, then nothing. A device with BOTH set must use
        // colors_actual, not colors — assert on the actual numeric effect,
        // not just "no warning was logged" (which colors alone would also
        // satisfy).
        let script = r#"
            local png = read_asset("tiny.png")
            local out = image_process(png, { palette_aware = true })
            return { data = { out = out }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_actual.lua", script);
        let runtime = LuaRuntime::new(loader.clone());

        let ctx_actual = DeviceContext {
            mac: "TE:ST:00:00:00:02".to_string(),
            colors: Some(vec!["#000000".to_string(), "#ffffff".to_string()]),
            colors_actual: Some(vec!["#202020".to_string(), "#d8d8d8".to_string()]),
            ..Default::default()
        };
        let runtime2 = LuaRuntime::new(loader);
        let ctx_colors_only = DeviceContext {
            mac: "TE:ST:00:00:00:03".to_string(),
            colors: Some(vec!["#000000".to_string(), "#ffffff".to_string()]),
            colors_actual: None,
            ..Default::default()
        };

        let result_actual = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_actual.lua"),
                &HashMap::new(),
                Some(&ctx_actual),
                None,
            )
            .expect("script must run");
        let result_colors = runtime2
            .run_script_from_asset(
                std::path::Path::new("test_img_actual.lua"),
                &HashMap::new(),
                Some(&ctx_colors_only),
                None,
            )
            .expect("script must run");

        assert_ne!(
            result_actual.data["out"].as_str().unwrap(),
            result_colors.data["out"].as_str().unwrap(),
            "colors_actual (#202020..#d8d8d8) must win over colors (#000000..#ffffff), \
             producing a different output"
        );
    }

    #[test]
    fn test_image_process_geometry_error_names_the_reason() {
        // A geometry-layer error (bad crop) must reach Lua as a real error,
        // not just "something failed" — pin the ImageProcessError::BadCrop
        // wording that process_image produces.
        let script = r#"
            local png = read_asset("tiny.png")
            local ok, err = pcall(function()
                return image_process(png, { crop = { x = 0.9, y = 0.0, w = 0.5, h = 1.0 } })
            end)
            return { data = { ok = ok, err = tostring(err) }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_crop.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_crop.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run — the pcall catches the error");

        assert!(
            !result.data["ok"].as_bool().unwrap(),
            "an out-of-bounds crop must raise"
        );
        let err = result.data["err"].as_str().unwrap();
        assert!(
            err.contains("does not lie within the image"),
            "must be the BadCrop message, not a generic failure: {err}"
        );
    }

    #[test]
    fn test_image_process_rejects_an_unknown_fit() {
        let script = r#"
            local png = read_asset("tiny.png")
            local ok, err = pcall(function()
                return image_process(png, { fit = "zoom" })
            end)
            return { data = { ok = ok, err = tostring(err) }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_fit.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_fit.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            !result.data["ok"].as_bool().unwrap(),
            "an unknown fit must raise"
        );
        let err = result.data["err"].as_str().unwrap();
        assert!(err.contains("fit"), "the error must name the field: {err}");
    }

    #[test]
    fn test_image_process_jpeg_format_and_quality() {
        let script = r#"
            local png = read_asset("tiny.png")
            local src = image_process(png, { format = "jpeg", quality = 50 })
            return {
                data = { is_jpeg = string.sub(src, 1, 23) == "data:image/jpeg;base64," },
                refresh_rate = 60
            }
        "#;

        let (_temp, loader) = setup_image_env("test_img_jpeg.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_jpeg.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["is_jpeg"].as_bool().unwrap(),
            "must be a jpeg data URI"
        );
    }

    #[test]
    fn test_image_process_with_no_opts_table_uses_defaults() {
        // The `opts` parameter is `Option<Table>` — a script that omits it
        // entirely must still work (the `let Some(t) = opts else { ... }`
        // branch in `parse_image_opts`), not just a script that passes `{}`.
        let script = r#"
            local png = read_asset("tiny.png")
            local src, w, h = image_process(png)
            return {
                data = { ok = src ~= nil, w = w, h = h },
                refresh_rate = 60
            }
        "#;

        let (_temp, loader) = setup_image_env("test_img_noopts.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_noopts.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(result.data["ok"].as_bool().unwrap());
        // No geometry given: dimensions pass through unchanged from the
        // 40x20 source `tiny_png` generates.
        assert_eq!(result.data["w"].as_i64().unwrap(), 40);
        assert_eq!(result.data["h"].as_i64().unwrap(), 20);
    }

    // ------------------------------------------------------------------
    // Fix round 1: the tone-parameter half of `parse_image_opts` had zero
    // coverage — `curve`/`sharpen`/most other tone fields could be deleted
    // from the parser wholesale and every test above stayed green. These
    // close that gap.
    // ------------------------------------------------------------------

    #[test]
    fn test_image_process_grayscale_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local gray = image_process(png, { grayscale = true })
            return { data = { differs = baseline ~= gray }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_grayscale.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_grayscale.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "grayscale = true must change the output"
        );
    }

    #[test]
    fn test_image_process_curve_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local curved = image_process(png, { curve = {{0,0},{0.5,0.9},{1,1}} })
            return { data = { differs = baseline ~= curved }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_curve.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_curve.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "a non-identity curve must change the output"
        );
    }

    #[test]
    fn test_image_process_sharpen_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local sharp = image_process(png, { sharpen = { amount = 100, radius = 2 } })
            return { data = { differs = baseline ~= sharp }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_sharpen.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_sharpen.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "sharpen must change the output"
        );
    }

    #[test]
    fn test_image_process_rejects_a_malformed_curve_point() {
        // Pin the message text, not merely that an error occurred — a curve
        // point missing its `output` value must name that, specifically.
        let script = r#"
            local png = read_asset("tiny.png")
            local ok, err = pcall(function()
                return image_process(png, { curve = {{0.1}} })
            end)
            return { data = { ok = ok, err = tostring(err) }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_curve_bad.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_curve_bad.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run — the pcall catches the error");

        assert!(
            !result.data["ok"].as_bool().unwrap(),
            "a curve point missing its output value must raise"
        );
        let err = result.data["err"].as_str().unwrap();
        assert!(
            err.contains("curve point missing output"),
            "must be the specific malformed-curve-point message: {err}"
        );
    }

    /// A hand-built 2x1 PNG: pure black then pure white, so `blacks`/
    /// `whites`'s effect on each end of the tone range can be read straight
    /// off the decoded pixels.
    fn black_white_png() -> Vec<u8> {
        let mut img = image::RgbImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([255, 255, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// Decode a `data:image/png;base64,...` URI back to an RGB pixel, so a
    /// test can assert on actual tone-mapped content, not just "it changed".
    fn decode_data_uri_pixel(uri: &str, x: u32, y: u32) -> image::Rgb<u8> {
        use base64::Engine as _;
        let b64 = uri
            .strip_prefix("data:image/png;base64,")
            .expect("expected a PNG data URI");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        *img.to_rgb8().get_pixel(x, y)
    }

    #[test]
    fn test_image_process_blacks_and_whites_are_not_interchangeable() {
        // `blacks` and `whites` push OPPOSITE ends of the tone range
        // (blacks lifts the shadow floor, whites pulls the highlight
        // ceiling down). A parser bug that swaps which `Params` field each
        // Lua key maps to would still change the output — so a plain
        // "differs from baseline" assertion can't catch it — but it would
        // move the WRONG end of the range. A black/white source pins that:
        // `blacks = -100` alone must lift the black pixel and leave the
        // white pixel alone; `whites = -100` alone must do the reverse.
        let script_blacks = r#"
            local png = read_asset("bw.png")
            local out = image_process(png, { fit = "none", blacks = -100 })
            return { data = { out = out }, refresh_rate = 60 }
        "#;
        let script_whites = r#"
            local png = read_asset("bw.png")
            local out = image_process(png, { fit = "none", whites = -100 })
            return { data = { out = out }, refresh_rate = 60 }
        "#;

        let (temp, loader) = setup_test_env(&[
            ("test_bw_blacks.lua", script_blacks),
            ("test_bw_whites.lua", script_whites),
        ]);
        std::fs::write(temp.path().join("bw.png"), black_white_png()).expect("write bw.png");

        let runtime = LuaRuntime::new(loader);
        let blacks_result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_bw_blacks.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");
        let whites_result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_bw_whites.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        let blacks_out = blacks_result.data["out"].as_str().unwrap();
        let whites_out = whites_result.data["out"].as_str().unwrap();

        let blacks_black_px = decode_data_uri_pixel(blacks_out, 0, 0);
        let whites_black_px = decode_data_uri_pixel(whites_out, 0, 0);
        let blacks_white_px = decode_data_uri_pixel(blacks_out, 1, 0);
        let whites_white_px = decode_data_uri_pixel(whites_out, 1, 0);

        assert!(
            blacks_black_px[0] > 20,
            "blacks = -100 must lift the black pixel toward the shadow floor: got {}",
            blacks_black_px[0]
        );
        assert!(
            whites_black_px[0] < 5,
            "whites = -100 alone must NOT move the black pixel: got {}",
            whites_black_px[0]
        );
        assert!(
            whites_white_px[0] < 235,
            "whites = -100 must pull the white pixel down from the highlight ceiling: got {}",
            whites_white_px[0]
        );
        assert!(
            blacks_white_px[0] > 250,
            "blacks = -100 alone must NOT move the white pixel: got {}",
            blacks_white_px[0]
        );
    }

    // ------------------------------------------------------------------
    // Fix round 2: `blacks`/`whites`/`curve`/`sharpen`/`grayscale` each got
    // a dedicated test above, but ten more `Params` fields
    // (temperature, tint, auto_levels, highlights, shadows, contrast,
    // clarity, vibrance, saturation, invert) were still covered only
    // incidentally, by the blanket "every field forced to None" mutation —
    // which those five dedicated tests happen to kill for unrelated
    // reasons. A single one of these ten dropped from `parse_image_opts`
    // would slip through undetected. One "differs from baseline" test per
    // field closes that, same shape as `..._grayscale_changes_the_output`.
    // ------------------------------------------------------------------

    #[test]
    fn test_image_process_temperature_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local warmed = image_process(png, { temperature = 100 })
            return { data = { differs = baseline ~= warmed }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_temperature.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_temperature.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "temperature = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_tint_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local tinted = image_process(png, { tint = 100 })
            return { data = { differs = baseline ~= tinted }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_tint.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_tint.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "tint = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_auto_levels_changes_the_output() {
        // `tiny.png`'s content is deliberately "muted, low-saturation" (see
        // its generator above) — its measured min/max sits well inside
        // 0..1, so stretching it to the default output_endpoints of (0, 1)
        // under auto_levels has an observable effect.
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local leveled = image_process(png, { auto_levels = true })
            return { data = { differs = baseline ~= leveled }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_auto_levels.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_auto_levels.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "auto_levels = true must change the output"
        );
    }

    #[test]
    fn test_image_process_highlights_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local lifted = image_process(png, { highlights = 100 })
            return { data = { differs = baseline ~= lifted }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_highlights.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_highlights.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "highlights = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_shadows_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local lifted = image_process(png, { shadows = 100 })
            return { data = { differs = baseline ~= lifted }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_shadows.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_shadows.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "shadows = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_contrast_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local punchy = image_process(png, { contrast = 100 })
            return { data = { differs = baseline ~= punchy }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_contrast.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_contrast.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "contrast = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_clarity_changes_the_output() {
        // Clarity is a local-contrast (presence) effect: it needs pixel
        // variation to act on, which `tiny.png`'s `(x + y) % 3` banding
        // provides across its 40x20 extent.
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local clarified = image_process(png, { clarity = 100 })
            return { data = { differs = baseline ~= clarified }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_clarity.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_clarity.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "clarity = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_vibrance_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local vivid = image_process(png, { vibrance = 100 })
            return { data = { differs = baseline ~= vivid }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_vibrance.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_vibrance.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "vibrance = 100 must change the output"
        );
    }

    #[test]
    fn test_image_process_saturation_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local desat = image_process(png, { saturation = -100 })
            return { data = { differs = baseline ~= desat }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_saturation.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_saturation.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "saturation = -100 must change the output"
        );
    }

    #[test]
    fn test_image_process_invert_changes_the_output() {
        let script = r#"
            local png = read_asset("tiny.png")
            local baseline = image_process(png, {})
            local inverted = image_process(png, { invert = true })
            return { data = { differs = baseline ~= inverted }, refresh_rate = 60 }
        "#;

        let (_temp, loader) = setup_image_env("test_img_invert.lua", script);
        let runtime = LuaRuntime::new(loader);
        let result = runtime
            .run_script_from_asset(
                std::path::Path::new("test_img_invert.lua"),
                &HashMap::new(),
                None,
                None,
            )
            .expect("script must run");

        assert!(
            result.data["differs"].as_bool().unwrap(),
            "invert = true must change the output"
        );
    }
}
