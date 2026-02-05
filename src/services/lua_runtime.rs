// Arc<Html> is used in single-threaded Lua context, so Send+Sync not required
#![allow(clippy::arc_with_non_send_sync)]

use mlua::{Lua, Result as LuaResult, Table, UserData, UserDataMethods, Value};
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::sync::Arc;

use super::DeviceContext;
use crate::assets::AssetLoader;

/// Result from running a Lua script
#[derive(Debug)]
pub struct ScriptResult {
    /// Data to pass to the template
    pub data: serde_json::Value,
    /// Refresh rate in seconds
    pub refresh_rate: u32,
    /// If true, skip rendering and just tell device to check back later
    pub skip_update: bool,
    /// Optional color palette override from script (hex RGB strings)
    pub colors: Option<Vec<String>>,
    /// Optional dither mode from script ("photo" or "graphics")
    pub dither: Option<String>,
}

/// Error type for Lua script execution
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    #[error("Script not found: {0}")]
    NotFound(String),
}

/// Information about a single font face, for exposing to Lua
#[derive(Debug, Clone)]
pub struct FontFaceInfo {
    pub style: String,
    pub weight: u16,
    pub stretch: String,
    pub monospaced: bool,
    pub post_script_name: String,
    pub bitmap_strikes: Vec<u16>,
}

/// Lua runtime for executing screen scripts
pub struct LuaRuntime {
    asset_loader: Arc<AssetLoader>,
    /// Font info keyed by family name
    font_families: HashMap<String, Vec<FontFaceInfo>>,
}

impl LuaRuntime {
    pub fn new(asset_loader: Arc<AssetLoader>) -> Self {
        Self {
            asset_loader,
            font_families: HashMap::new(),
        }
    }

    pub fn with_fonts(
        asset_loader: Arc<AssetLoader>,
        font_families: HashMap<String, Vec<FontFaceInfo>>,
    ) -> Self {
        Self {
            asset_loader,
            font_families,
        }
    }

    /// Run a Lua script with the given parameters
    pub fn run_script(
        &self,
        script_path: &std::path::Path,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
        timestamp_override: Option<i64>,
    ) -> Result<ScriptResult, ScriptError> {
        let script_content = self
            .asset_loader
            .read_screen_string(script_path)
            .map_err(|e| ScriptError::NotFound(e.to_string()))?;

        let lua = Lua::new();

        // Derive screen name from script path (e.g., "transit.lua" -> "transit")
        let screen_name = script_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default")
            .to_string();

        // Set up the Lua environment
        self.setup_globals(&lua, params, device_ctx, &screen_name, timestamp_override)?;

        // Execute the script
        let result: Table = lua.load(&script_content).eval()?;

        // Extract data, refresh_rate, skip_update, and colors
        let data = self.table_to_json(&lua, result.get::<Table>("data")?)?;
        let refresh_rate: u32 = result.get("refresh_rate").unwrap_or(900);
        let skip_update: bool = result.get("skip_update").unwrap_or(false);

        // Parse optional colors array from script return
        let colors = result
            .get::<Table>("colors")
            .ok()
            .map(|t| {
                (1..=t.raw_len())
                    .filter_map(|i| t.raw_get::<String>(i).ok())
                    .collect::<Vec<String>>()
            })
            .filter(|v| !v.is_empty());

        // Parse optional dither mode from script return
        let dither = result.get::<String>("dither").ok();

        Ok(ScriptResult {
            data,
            refresh_rate,
            skip_update,
            colors,
            dither,
        })
    }

    /// Set up Lua global functions and variables
    fn setup_globals(
        &self,
        lua: &Lua,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
        screen_name: &str,
        timestamp_override: Option<i64>,
    ) -> LuaResult<()> {
        let globals = lua.globals();

        // Add params table
        let params_table = lua.create_table()?;
        for (key, value) in params {
            params_table.set(key.as_str(), self.yaml_to_lua(lua, value)?)?;
        }
        globals.set("params", params_table)?;

        // Add device table
        let device_table = lua.create_table()?;
        if let Some(ctx) = device_ctx {
            device_table.set("mac", ctx.mac.as_str())?;
            if let Some(voltage) = ctx.battery_voltage {
                device_table.set("battery_voltage", voltage)?;
            }
            if let Some(rssi) = ctx.rssi {
                device_table.set("rssi", rssi)?;
            }
            if let Some(ref model) = ctx.model {
                device_table.set("model", model.as_str())?;
            }
            if let Some(ref fw) = ctx.firmware_version {
                device_table.set("firmware_version", fw.as_str())?;
            }
            if let Some(width) = ctx.width {
                device_table.set("width", width)?;
            }
            if let Some(height) = ctx.height {
                device_table.set("height", height)?;
            }
            if let Some(ref code) = ctx.registration_code {
                device_table.set("registration_code", code.as_str())?;
                // Also provide hyphenated version for convenience
                if code.len() == 10 {
                    let hyphenated = format!("{}-{}", &code[..5], &code[5..]);
                    device_table.set("registration_code_hyphenated", hyphenated)?;
                }
            }
            if let Some(ref board) = ctx.board {
                device_table.set("board", board.as_str())?;
            }
            if let Some(ref colors) = ctx.colors {
                let colors_table = lua.create_table()?;
                for (i, color) in colors.iter().enumerate() {
                    colors_table.set(i + 1, color.as_str())?;
                }
                device_table.set("colors", colors_table)?;
            }
        }
        globals.set("device", device_table)?;

        // Add layout table with pre-computed responsive values
        let layout_table = lua.create_table()?;
        let width = device_ctx.and_then(|ctx| ctx.width).unwrap_or(800) as f64;
        let height = device_ctx.and_then(|ctx| ctx.height).unwrap_or(480) as f64;
        let scale = f64::min(width / 800.0, height / 480.0);

        layout_table.set("width", width as i64)?;
        layout_table.set("height", height as i64)?;
        layout_table.set("scale", scale)?;
        layout_table.set("center_x", (width / 2.0).floor() as i64)?;
        layout_table.set("center_y", (height / 2.0).floor() as i64)?;
        // Expose color palette on layout table
        if let Some(colors) = device_ctx.and_then(|ctx| ctx.colors.as_ref()) {
            let colors_table = lua.create_table()?;
            for (i, color) in colors.iter().enumerate() {
                colors_table.set(i + 1, color.as_str())?;
            }
            layout_table.set("colors", colors_table)?;
            layout_table.set("color_count", colors.len() as i64)?;
            // Count grey levels (colors where R=G=B)
            let grey_count = colors
                .iter()
                .filter(|c| {
                    let hex = c.trim_start_matches('#');
                    hex.len() == 6 && hex[0..2] == hex[2..4] && hex[2..4] == hex[4..6]
                })
                .count();
            layout_table.set("grey_count", grey_count as i64)?;
        } else {
            // Default 4-grey when no colors provided
            layout_table.set("color_count", 4i64)?;
            layout_table.set("grey_count", 4i64)?;
        }
        // Pre-floored margins for pixel-aligned positioning
        layout_table.set("margin", (20.0 * scale).floor() as i64)?;
        layout_table.set("margin_sm", (10.0 * scale).floor() as i64)?;
        layout_table.set("margin_lg", (40.0 * scale).floor() as i64)?;
        globals.set("layout", layout_table)?;

        // Store scale in Lua registry for helper functions
        lua.set_named_registry_value("__layout_scale", scale)?;

        // scale_font(value) -> number
        // Returns value * scale (preserves precision for font sizes)
        let scale_font = lua.create_function(|lua, value: f64| {
            let scale: f64 = lua.named_registry_value("__layout_scale")?;
            Ok(value * scale)
        })?;
        globals.set("scale_font", scale_font)?;

        // scale_pixel(value) -> integer
        // Returns floor(value * scale) for pixel-aligned positions/dimensions
        let scale_pixel = lua.create_function(|lua, value: f64| {
            let scale: f64 = lua.named_registry_value("__layout_scale")?;
            Ok((value * scale).floor() as i64)
        })?;
        globals.set("scale_pixel", scale_pixel)?;

        // greys(levels) -> array of {value, color, text_color}
        // Generates a grey palette with the specified number of levels
        let greys = lua.create_function(|lua, levels: u32| {
            let table = lua.create_table()?;
            for i in 0..levels {
                let entry = lua.create_table()?;
                // Calculate grey value (0 = black, 255 = white)
                let value = if levels == 1 {
                    255
                } else {
                    (255 * i / (levels - 1)) as u8
                };
                let hex = format!("#{:02x}{:02x}{:02x}", value, value, value);
                // Text color is white for dark backgrounds, black for light
                let text_color = if value < 128 { "#ffffff" } else { "#000000" };

                entry.set("value", value)?;
                entry.set("color", hex)?;
                entry.set("text_color", text_color)?;
                table.set(i + 1, entry)?;
            }
            Ok(table)
        })?;
        globals.set("greys", greys)?;

        // base64_encode(data) -> string
        let base64_encode = lua.create_function(|_, data: mlua::String| {
            use base64::Engine;
            Ok(base64::engine::general_purpose::STANDARD.encode(data.as_bytes()))
        })?;
        globals.set("base64_encode", base64_encode)?;

        // url_encode(string) -> string
        // URL-encodes a string for use in URLs (query parameters, path segments)
        // Per RFC 3986, unreserved characters (A-Z, a-z, 0-9, -, ., _, ~) are NOT encoded
        let url_encode = lua.create_function(|_, s: String| {
            use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
            // Encode everything except unreserved characters per RFC 3986
            const ENCODE_SET: &AsciiSet = &CONTROLS
                .add(b' ')
                .add(b'!')
                .add(b'"')
                .add(b'#')
                .add(b'$')
                .add(b'%')
                .add(b'&')
                .add(b'\'')
                .add(b'(')
                .add(b')')
                .add(b'*')
                .add(b'+')
                .add(b',')
                .add(b'/')
                .add(b':')
                .add(b';')
                .add(b'<')
                .add(b'=')
                .add(b'>')
                .add(b'?')
                .add(b'@')
                .add(b'[')
                .add(b'\\')
                .add(b']')
                .add(b'^')
                .add(b'`')
                .add(b'{')
                .add(b'|')
                .add(b'}');
            Ok(utf8_percent_encode(&s, ENCODE_SET).to_string())
        })?;
        globals.set("url_encode", url_encode)?;

        // url_decode(string) -> string
        // Decodes a URL-encoded string
        let url_decode = lua.create_function(|_, s: String| {
            use percent_encoding::percent_decode_str;
            percent_decode_str(&s)
                .decode_utf8()
                .map(|cow| cow.into_owned())
                .map_err(|e| mlua::Error::external(format!("URL decode error: {e}")))
        })?;
        globals.set("url_decode", url_decode)?;

        // read_asset(path) -> string (binary data)
        // Reads from screens/<screen_name>/<path>
        let asset_loader = self.asset_loader.clone();
        let screen_prefix = screen_name.to_string();
        let read_asset = lua.create_function(move |lua, path: String| {
            let full_path = format!("{screen_prefix}/{path}");
            let asset_path = std::path::Path::new(&full_path);

            match asset_loader.read_screen(asset_path) {
                Ok(data) => {
                    // Return as Lua string (which can contain binary data)
                    lua.create_string(&*data)
                }
                Err(e) => Err(mlua::Error::external(format!("Failed to read asset: {e}"))),
            }
        })?;
        globals.set("read_asset", read_asset)?;

        // http_request(url, options?) -> string
        // Core HTTP function with method option
        // options:
        //   method: "GET", "POST", "PUT", "DELETE", etc. (default: "GET")
        //   params: table of query parameters (auto URL-encoded)
        //   headers: table of header name -> value pairs
        //   body: string body to send
        //   json: table to send as JSON (auto-serializes and sets Content-Type)
        //   basic_auth: { username = "...", password = "..." }
        //   timeout: number of seconds (default: 30)
        //   follow_redirects: boolean (default: true)
        //   max_redirects: number (default: 10)
        //   danger_accept_invalid_certs: boolean (default: false) - accept self-signed certs
        //   ca_cert: path to CA certificate PEM file for server verification
        //   client_cert: path to client certificate PEM file for mTLS
        //   client_key: path to client private key PEM file for mTLS
        //   cache_ttl: number of seconds to cache the response (default: no caching)
        let http_request =
            lua.create_function(|lua, (url, options): (String, Option<Table>)| {
                use super::http_cache;

                let method = options
                    .as_ref()
                    .and_then(|opts| opts.get::<String>("method").ok())
                    .unwrap_or_else(|| "GET".to_string());

                tracing::debug!(url = %url, method = %method, "Lua http_request");

                let mut client_builder = reqwest::blocking::Client::builder();
                let mut timeout_secs = 30u64;
                let mut follow_redirects = true;
                let mut max_redirects = 10usize;
                let mut danger_accept_invalid_certs = false;
                let mut cache_ttl: Option<u64> = None;

                // Certificate paths (will be parsed from options)
                let mut ca_cert_path: Option<String> = None;
                let mut client_cert_path: Option<String> = None;
                let mut client_key_path: Option<String> = None;

                // For cache key computation
                let mut params_for_cache: Option<Vec<(String, String)>> = None;
                let mut headers_for_cache: Option<Vec<(String, String)>> = None;
                let mut body_for_cache: Option<String> = None;

                // Parse options if provided
                if let Some(ref opts) = options {
                    if let Ok(t) = opts.get::<u64>("timeout") {
                        timeout_secs = t;
                    }
                    if let Ok(f) = opts.get::<bool>("follow_redirects") {
                        follow_redirects = f;
                    }
                    if let Ok(m) = opts.get::<usize>("max_redirects") {
                        max_redirects = m;
                    }
                    if let Ok(d) = opts.get::<bool>("danger_accept_invalid_certs") {
                        danger_accept_invalid_certs = d;
                    }
                    if let Ok(ca) = opts.get::<String>("ca_cert") {
                        ca_cert_path = Some(ca);
                    }
                    if let Ok(cert) = opts.get::<String>("client_cert") {
                        client_cert_path = Some(cert);
                    }
                    if let Ok(key) = opts.get::<String>("client_key") {
                        client_key_path = Some(key);
                    }
                    if let Ok(ttl) = opts.get::<u64>("cache_ttl") {
                        cache_ttl = Some(ttl);
                    }
                }

                client_builder =
                    client_builder.timeout(std::time::Duration::from_secs(timeout_secs));

                // Configure redirect policy
                if follow_redirects {
                    client_builder =
                        client_builder.redirect(reqwest::redirect::Policy::limited(max_redirects));
                } else {
                    client_builder = client_builder.redirect(reqwest::redirect::Policy::none());
                }

                // Configure TLS certificate validation
                if danger_accept_invalid_certs {
                    tracing::warn!(url = %url, "Accepting invalid TLS certificates - this is insecure!");
                    client_builder = client_builder.danger_accept_invalid_certs(true);
                }

                // Add custom CA certificate if provided
                if let Some(ca_path) = ca_cert_path {
                    let ca_data = std::fs::read(&ca_path).map_err(|e| {
                        mlua::Error::external(format!("Failed to read CA certificate file '{}': {}", ca_path, e))
                    })?;
                    let ca_cert = reqwest::Certificate::from_pem(&ca_data).map_err(|e| {
                        mlua::Error::external(format!("Failed to parse CA certificate: {}", e))
                    })?;
                    client_builder = client_builder.add_root_certificate(ca_cert);
                    tracing::debug!(ca_cert = %ca_path, "Added custom CA certificate");
                }

                // Add client certificate for mTLS if both cert and key are provided
                if let (Some(cert_path), Some(key_path)) = (client_cert_path.clone(), client_key_path.clone()) {
                    // Read and combine certificate and key into a single PEM buffer
                    let cert_data = std::fs::read(&cert_path).map_err(|e| {
                        mlua::Error::external(format!("Failed to read client certificate file '{}': {}", cert_path, e))
                    })?;
                    let key_data = std::fs::read(&key_path).map_err(|e| {
                        mlua::Error::external(format!("Failed to read client key file '{}': {}", key_path, e))
                    })?;

                    // Create combined PEM buffer (cert + key)
                    let mut pem_buffer = cert_data.clone();
                    pem_buffer.push(b'\n');
                    pem_buffer.extend_from_slice(&key_data);

                    let identity = reqwest::Identity::from_pem(&pem_buffer).map_err(|e| {
                        mlua::Error::external(format!("Failed to create client identity from cert/key: {}", e))
                    })?;
                    client_builder = client_builder.identity(identity);
                    tracing::debug!(client_cert = %cert_path, client_key = %key_path, "Added client certificate for mTLS");
                } else if client_cert_path.is_some() || client_key_path.is_some() {
                    return Err(mlua::Error::external(
                        "Both client_cert and client_key must be provided together for mTLS"
                    ));
                }

                let client = client_builder.build().map_err(|e| {
                    mlua::Error::external(format!("Failed to build HTTP client: {e}"))
                })?;

                let mut request = match method.to_uppercase().as_str() {
                    "GET" => client.get(&url),
                    "POST" => client.post(&url),
                    "PUT" => client.put(&url),
                    "DELETE" => client.delete(&url),
                    "PATCH" => client.patch(&url),
                    "HEAD" => client.head(&url),
                    _ => {
                        return Err(mlua::Error::external(format!(
                            "Unsupported HTTP method: {method}"
                        )))
                    }
                };

                if let Some(ref opts) = options {
                    // Warn about unknown options
                    const KNOWN_OPTIONS: &[&str] = &[
                        "method",
                        "params",
                        "headers",
                        "body",
                        "json",
                        "basic_auth",
                        "timeout",
                        "follow_redirects",
                        "max_redirects",
                        "danger_accept_invalid_certs",
                        "ca_cert",
                        "client_cert",
                        "client_key",
                        "cache_ttl",
                    ];
                    for key in opts.clone().pairs::<String, Value>().flatten() {
                        if !KNOWN_OPTIONS.contains(&key.0.as_str()) {
                            tracing::warn!(
                                option = %key.0,
                                "http_request: unknown option (valid options: {})",
                                KNOWN_OPTIONS.join(", ")
                            );
                        }
                    }

                    // Add query parameters
                    if let Ok(params_table) = opts.get::<Table>("params") {
                        let params: Vec<(String, String)> = params_table
                            .pairs::<String, Value>()
                            .flatten()
                            .map(|(k, v)| {
                                let v_str = match v {
                                    Value::String(s) => {
                                        s.to_str().map(|s| s.to_string()).unwrap_or_default()
                                    }
                                    Value::Integer(i) => i.to_string(),
                                    Value::Number(n) => n.to_string(),
                                    Value::Boolean(b) => b.to_string(),
                                    _ => String::new(),
                                };
                                (k, v_str)
                            })
                            .collect();
                        params_for_cache = Some(params.clone());
                        request = request.query(&params);
                    }

                    // Add custom headers
                    if let Ok(headers_table) = opts.get::<Table>("headers") {
                        let mut headers_vec = Vec::new();
                        for (name, value) in headers_table.pairs::<String, String>().flatten() {
                            headers_vec.push((name.clone(), value.clone()));
                            request = request.header(&name, &value);
                        }
                        headers_for_cache = Some(headers_vec);
                    }

                    // Add basic auth
                    if let Ok(auth_table) = opts.get::<Table>("basic_auth") {
                        let username: String = auth_table.get("username").unwrap_or_default();
                        let password: String = auth_table.get("password").unwrap_or_default();
                        if !username.is_empty() {
                            request = request.basic_auth(username, Some(password));
                        }
                    }

                    // Add body - json takes precedence over body
                    if let Ok(json_table) = opts.get::<Table>("json") {
                        let json_value = lua_value_to_json(lua, Value::Table(json_table))?;
                        let json_str = serde_json::to_string(&json_value).map_err(|e| {
                            mlua::Error::external(format!("JSON encode error: {e}"))
                        })?;
                        body_for_cache = Some(json_str.clone());
                        request = request
                            .header("Content-Type", "application/json")
                            .body(json_str);
                    } else if let Ok(body) = opts.get::<String>("body") {
                        body_for_cache = Some(body.clone());
                        request = request.body(body);
                    }
                }

                // Compute cache key and check cache if caching is enabled
                let cache_key = cache_ttl.map(|_| {
                    http_cache::compute_cache_key(
                        &url,
                        &method,
                        params_for_cache.as_deref(),
                        headers_for_cache.as_deref(),
                        body_for_cache.as_deref(),
                    )
                });

                // Check cache first if caching is enabled
                if let Some(ref key) = cache_key {
                    if let Some(cached_response) = http_cache::get_cached(key) {
                        return lua.create_string(&cached_response);
                    }
                }

                // Make the actual request
                let response_bytes = match request.send() {
                    Ok(response) => match response.bytes() {
                        Ok(bytes) => bytes.to_vec(),
                        Err(e) => {
                            return Err(mlua::Error::external(format!(
                                "Failed to read response: {e}"
                            )))
                        }
                    },
                    Err(e) => return Err(mlua::Error::external(format!("HTTP request failed: {e}"))),
                };

                // Store in cache if caching is enabled
                if let (Some(key), Some(ttl)) = (cache_key, cache_ttl) {
                    http_cache::store_cached(key, response_bytes.clone(), ttl);
                }

                lua.create_string(&response_bytes)
            })?;
        globals.set("http_request", http_request.clone())?;

        // http_get(url, options?) - convenience wrapper for GET requests
        let http_get = http_request.clone();
        globals.set("http_get", http_get)?;

        // http_post(url, options?) - convenience wrapper for POST requests
        let http_post =
            lua.create_function(move |lua, (url, options): (String, Option<Table>)| {
                // Create options table with method = "POST"
                let opts = match options {
                    Some(t) => t,
                    None => lua.create_table()?,
                };
                opts.set("method", "POST")?;
                http_request.call::<String>((url, Some(opts)))
            })?;
        globals.set("http_post", http_post)?;

        // html_parse(html) -> Document
        let html_parse = lua.create_function(|_, html: String| {
            Ok(LuaDocument {
                doc: Arc::new(Html::parse_document(&html)),
            })
        })?;
        globals.set("html_parse", html_parse)?;

        // time_now() -> number (Unix timestamp)
        // Uses override timestamp if provided (for dev mode time simulation)
        let time_now = lua.create_function(move |_, ()| {
            Ok(timestamp_override.unwrap_or_else(|| chrono::Utc::now().timestamp()))
        })?;
        globals.set("time_now", time_now)?;

        // time_format(timestamp, format) -> string (uses local time)
        let time_format = lua.create_function(|_, (ts, fmt): (i64, String)| {
            use chrono::{Local, TimeZone};
            let dt = Local
                .timestamp_opt(ts, 0)
                .single()
                .ok_or_else(|| mlua::Error::external("Invalid timestamp"))?;
            Ok(dt.format(&fmt).to_string())
        })?;
        globals.set("time_format", time_format)?;

        // time_parse(str, format) -> number
        let time_parse = lua.create_function(|_, (s, fmt): (String, String)| {
            use chrono::NaiveDateTime;
            let dt = NaiveDateTime::parse_from_str(&s, &fmt)
                .map_err(|e| mlua::Error::external(format!("Failed to parse time: {e}")))?;
            Ok(dt.and_utc().timestamp())
        })?;
        globals.set("time_parse", time_parse)?;

        // json_decode(json_string) -> table
        let json_decode = lua.create_function(|lua, json_str: String| {
            let value: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| mlua::Error::external(format!("JSON parse error: {e}")))?;
            json_to_lua(lua, &value)
        })?;
        globals.set("json_decode", json_decode)?;

        // json_encode(table) -> string
        let json_encode = lua.create_function(|lua, value: Value| {
            let json = lua_value_to_json(lua, value)?;
            serde_json::to_string(&json)
                .map_err(|e| mlua::Error::external(format!("JSON encode error: {e}")))
        })?;
        globals.set("json_encode", json_encode)?;

        // Logging functions
        let log_info = lua.create_function(|_, msg: String| {
            tracing::info!(script = true, "{}", msg);
            Ok(())
        })?;
        globals.set("log_info", log_info)?;

        let log_warn = lua.create_function(|_, msg: String| {
            tracing::warn!(script = true, "{}", msg);
            Ok(())
        })?;
        globals.set("log_warn", log_warn)?;

        let log_error = lua.create_function(|_, msg: String| {
            tracing::error!(script = true, "{}", msg);
            Ok(())
        })?;
        globals.set("log_error", log_error)?;

        // qr_svg(data, options) -> string
        // Generates a pixel-aligned QR code as an SVG fragment
        // Options:
        //   anchor: positioning anchor - "top-left", "top-right", "bottom-left", "bottom-right", "center" (default: "top-left")
        //   top, left, right, bottom: margin from respective edge in pixels (default: 0)
        //   module_size: size of each QR "pixel" (default: 4)
        //   ec_level: error correction level - "L", "M", "Q", "H" (default: "M")
        //   quiet_zone: margin in modules (default: 4)
        let qr_svg = lua.create_function(|lua, (data, options): (String, Table)| {
            use fast_qr::ECL;

            // Get screen dimensions from device context (defaults for TRMNL OG)
            let globals = lua.globals();
            let (screen_width, screen_height) = if let Ok(device) = globals.get::<Table>("device") {
                let w = device.get::<u32>("width").unwrap_or(800);
                let h = device.get::<u32>("height").unwrap_or(480);
                (w as i32, h as i32)
            } else {
                (800, 480)
            };

            // Parse anchor
            let anchor: String = options
                .get::<String>("anchor")
                .unwrap_or_else(|_| "top-left".to_string());

            // Parse margins (default: 0)
            let margin_top: i32 = options.get::<i32>("top").unwrap_or(0);
            let margin_left: i32 = options.get::<i32>("left").unwrap_or(0);
            let margin_right: i32 = options.get::<i32>("right").unwrap_or(0);
            let margin_bottom: i32 = options.get::<i32>("bottom").unwrap_or(0);

            // Parse other options
            let module_size: i32 = options.get::<i32>("module_size").unwrap_or(4);

            let ec_level = options
                .get::<String>("ec_level")
                .ok()
                .map(|s| match s.to_uppercase().as_str() {
                    "L" => ECL::L,
                    "Q" => ECL::Q,
                    "H" => ECL::H,
                    _ => ECL::M,
                })
                .unwrap_or(ECL::M);

            let quiet_zone: i32 = options.get::<i32>("quiet_zone").unwrap_or(4);

            // Generate QR code
            let qr = fast_qr::QRBuilder::new(data)
                .ecl(ec_level)
                .build()
                .map_err(|e| mlua::Error::external(format!("QR code generation failed: {e}")))?;

            let qr_size = qr.size as i32;
            let total_size = (qr_size + 2 * quiet_zone) * module_size;

            // Calculate actual top-left position based on anchor and margins
            let (actual_x, actual_y) = match anchor.to_lowercase().as_str() {
                "top-left" => (margin_left, margin_top),
                "top-right" => (screen_width - total_size - margin_right, margin_top),
                "bottom-left" => (margin_left, screen_height - total_size - margin_bottom),
                "bottom-right" => (screen_width - total_size - margin_right, screen_height - total_size - margin_bottom),
                "center" => ((screen_width - total_size) / 2, (screen_height - total_size) / 2),
                _ => {
                    return Err(mlua::Error::external(format!(
                        "qr_svg: invalid anchor '{anchor}'. Valid values: top-left, top-right, bottom-left, bottom-right, center"
                    )));
                }
            };

            // Build SVG manually for pixel-perfect alignment
            let mut svg = format!(
                r#"<g transform="translate({actual_x},{actual_y})"><rect x="0" y="0" width="{total_size}" height="{total_size}" fill="white"/>"#
            );

            // Add black modules
            for row in 0..qr_size {
                for col in 0..qr_size {
                    // qr[row] returns a slice, qr[row][col] returns the Module
                    // Module::DARK is true, so we check if the module value is true (dark)
                    if qr[row as usize][col as usize].value() {
                        let px = (col + quiet_zone) * module_size;
                        let py = (row + quiet_zone) * module_size;
                        svg.push_str(&format!(
                            r#"<rect x="{px}" y="{py}" width="{module_size}" height="{module_size}" fill="black"/>"#
                        ));
                    }
                }
            }

            svg.push_str("</g>");
            Ok(svg)
        })?;
        globals.set("qr_svg", qr_svg)?;

        // Build fonts table from font face info
        let fonts_table = lua.create_table()?;
        for (family, faces) in &self.font_families {
            let family_table = lua.create_table()?;
            for (i, face) in faces.iter().enumerate() {
                let face_table = lua.create_table()?;
                face_table.set("style", face.style.as_str())?;
                face_table.set("weight", face.weight)?;
                face_table.set("stretch", face.stretch.as_str())?;
                face_table.set("monospaced", face.monospaced)?;
                face_table.set("post_script_name", face.post_script_name.as_str())?;
                let strikes_table = lua.create_table()?;
                for (j, &ppem) in face.bitmap_strikes.iter().enumerate() {
                    strikes_table.set(j + 1, ppem)?;
                }
                face_table.set("bitmap_strikes", strikes_table)?;
                family_table.set(i + 1, face_table)?;
            }
            fonts_table.set(family.as_str(), family_table)?;
        }
        globals.set("fonts", fonts_table)?;

        Ok(())
    }

    /// Convert a Lua table to JSON
    fn table_to_json(&self, lua: &Lua, table: Table) -> LuaResult<serde_json::Value> {
        self.lua_to_json(lua, Value::Table(table))
    }

    /// Convert a Lua value to JSON
    #[allow(clippy::only_used_in_recursion)]
    fn lua_to_json(&self, lua: &Lua, value: Value) -> LuaResult<serde_json::Value> {
        match value {
            Value::Nil => Ok(serde_json::Value::Null),
            Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
            Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
            Value::Number(n) => Ok(serde_json::json!(n)),
            Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
            Value::Table(t) => {
                // Check if it's an array (sequential integer keys starting at 1)
                let len = t.raw_len();
                if len > 0 {
                    let mut arr = Vec::new();
                    for i in 1..=len {
                        if let Ok(v) = t.raw_get::<Value>(i) {
                            arr.push(self.lua_to_json(lua, v)?);
                        }
                    }
                    // Verify it's really an array by checking key count
                    let mut key_count = 0;
                    for _ in t.clone().pairs::<Value, Value>() {
                        key_count += 1;
                    }
                    if key_count == len {
                        return Ok(serde_json::Value::Array(arr));
                    }
                }

                // It's an object
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, self.lua_to_json(lua, v)?);
                }
                Ok(serde_json::Value::Object(map))
            }
            Value::UserData(ud) => {
                // Try to extract meaningful data from userdata
                if ud.is::<LuaElement>() {
                    let elem = ud.borrow::<LuaElement>()?;
                    Ok(serde_json::Value::String(elem.text()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    /// Convert YAML value to Lua value
    #[allow(clippy::only_used_in_recursion)]
    fn yaml_to_lua(&self, lua: &Lua, value: &serde_yaml::Value) -> LuaResult<Value> {
        match value {
            serde_yaml::Value::Null => Ok(Value::Nil),
            serde_yaml::Value::Bool(b) => Ok(Value::Boolean(*b)),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Number(f))
                } else {
                    Ok(Value::Nil)
                }
            }
            serde_yaml::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
            serde_yaml::Value::Sequence(arr) => {
                let table = lua.create_table()?;
                for (i, v) in arr.iter().enumerate() {
                    table.set(i + 1, self.yaml_to_lua(lua, v)?)?;
                }
                Ok(Value::Table(table))
            }
            serde_yaml::Value::Mapping(map) => {
                let table = lua.create_table()?;
                for (k, v) in map {
                    if let serde_yaml::Value::String(key) = k {
                        table.set(key.as_str(), self.yaml_to_lua(lua, v)?)?;
                    }
                }
                Ok(Value::Table(table))
            }
            _ => Ok(Value::Nil),
        }
    }
}

/// Wrapper for scraper's Html document exposed to Lua
#[derive(Clone)]
struct LuaDocument {
    doc: Arc<Html>,
}

impl UserData for LuaDocument {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // select(selector) -> Elements
        methods.add_method("select", |lua, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            let elements: Vec<_> = this
                .doc
                .select(&sel)
                .map(|el| LuaElement::new(el.html()))
                .collect();

            let table = lua.create_table()?;
            for (i, elem) in elements.into_iter().enumerate() {
                table.set(i + 1, elem)?;
            }

            // Add each() method to the table
            // Use raw_len and raw_get to iterate only over array elements (not the "each" key)
            let each_fn = lua.create_function(|_, (tbl, func): (Table, mlua::Function)| {
                let len = tbl.raw_len();
                for i in 1..=len {
                    if let Ok(elem) = tbl.raw_get::<Value>(i) {
                        func.call::<()>(elem)?;
                    }
                }
                Ok(())
            })?;
            table.set("each", each_fn)?;

            Ok(table)
        });

        // select_one(selector) -> Element or nil
        methods.add_method("select_one", |_, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            Ok(this
                .doc
                .select(&sel)
                .next()
                .map(|el| LuaElement::new(el.html())))
        });
    }
}

/// Wrapper for a single HTML element exposed to Lua
#[derive(Clone)]
struct LuaElement {
    html: String,
}

impl LuaElement {
    fn new(html: String) -> Self {
        Self { html }
    }

    fn text(&self) -> String {
        let fragment = Html::parse_fragment(&self.html);
        fragment
            .root_element()
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string()
    }

    fn get_attr(&self, name: &str) -> Option<String> {
        let fragment = Html::parse_fragment(&self.html);
        fragment
            .root_element()
            .select(&Selector::parse("*").unwrap())
            .next()
            .and_then(|el| el.value().attr(name).map(|s| s.to_string()))
    }
}

impl UserData for LuaElement {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // text() -> string
        methods.add_method("text", |_, this, ()| Ok(this.text()));

        // attr(name) -> string or nil
        methods.add_method("attr", |_, this, name: String| Ok(this.get_attr(&name)));

        // html() -> string
        methods.add_method("html", |_, this, ()| Ok(this.html.clone()));

        // select(selector) -> Elements (for chaining)
        methods.add_method("select", |lua, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            // Parse as fragment and search all elements (not just from root)
            let fragment = Html::parse_fragment(&this.html);
            let elements: Vec<_> = fragment
                .select(&sel)
                .map(|el| LuaElement::new(el.html()))
                .collect();

            let table = lua.create_table()?;
            for (i, elem) in elements.into_iter().enumerate() {
                table.set(i + 1, elem)?;
            }

            // Add each() method
            // Use raw_len and raw_get to iterate only over array elements (not the "each" key)
            let each_fn = lua.create_function(|_, (tbl, func): (Table, mlua::Function)| {
                let len = tbl.raw_len();
                for i in 1..=len {
                    if let Ok(elem) = tbl.raw_get::<Value>(i) {
                        func.call::<()>(elem)?;
                    }
                }
                Ok(())
            })?;
            table.set("each", each_fn)?;

            Ok(table)
        });

        // select_one(selector) -> Element or nil
        methods.add_method("select_one", |_, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            // Parse as fragment and search all elements (not just from root)
            let fragment = Html::parse_fragment(&this.html);
            Ok(fragment
                .select(&sel)
                .next()
                .map(|el| LuaElement::new(el.html())))
        });
    }
}

/// Convert JSON value to Lua value
fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Convert Lua value to JSON (standalone function for use in closures)
fn lua_value_to_json(_lua: &Lua, value: Value) -> LuaResult<serde_json::Value> {
    lua_to_json_inner(value)
}

/// Inner conversion function that doesn't need Lua reference
fn lua_to_json_inner(value: Value) -> LuaResult<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
        Value::Number(n) => Ok(serde_json::json!(n)),
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting at 1)
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::new();
                for i in 1..=len {
                    if let Ok(v) = t.raw_get::<Value>(i) {
                        arr.push(lua_to_json_inner(v)?);
                    }
                }
                // Verify it's really an array by checking key count
                let mut key_count = 0;
                for _ in t.clone().pairs::<Value, Value>() {
                    key_count += 1;
                }
                if key_count == len {
                    return Ok(serde_json::Value::Array(arr));
                }
            }

            // It's an object
            let mut map = serde_json::Map::new();
            for pair in t.pairs::<String, Value>() {
                let (k, v) = pair?;
                map.insert(k, lua_to_json_inner(v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Ok(serde_json::Value::Null),
    }
}
