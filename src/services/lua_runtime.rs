// Arc<Html> is used in single-threaded Lua context, so Send+Sync not required
#![allow(clippy::arc_with_non_send_sync)]

use mlua::{
    ChunkMode, Lua, LuaOptions, Result as LuaResult, StdLib, Table, UserData, UserDataMethods,
    Value,
};
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::DeviceContext;
use crate::assets::AssetLoader;
use crate::services::screen_repo_loader::ScreenRepoSource;

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
    /// Optional measured-colour override from script (hex RGB strings),
    /// index-parallel to `colors`. Wins the measured chain when its length
    /// matches the resolved official palette; see
    /// `crate::api::display::resolve_measured_colors`.
    pub colors_actual: Option<Vec<String>>,
    /// Optional dither mode from script ("photo" or "graphics")
    pub dither: Option<String>,
    /// Optional error clamp override from script
    pub error_clamp: Option<f32>,
    /// Optional blue noise jitter scale override from script
    pub noise_scale: Option<f32>,
    /// Optional chroma clamp override from script
    pub chroma_clamp: Option<f32>,
    /// Optional dither strength override from script
    pub strength: Option<f32>,
    /// Optional gamut mapping knobs from the script return. Only takes effect
    /// where the SVG marks a region `data-byonk-tone="continuous"`.
    pub gamut: Option<crate::models::GamutTuningValues>,
    /// Font hinting from the script's `font_hinting` directive.
    ///
    /// `None` means the script had no directive at all, so the server's
    /// adaptive default applies untouched. See
    /// [`crate::rendering::font_config::FontHintingDirective`] for how a
    /// present directive is resolved against the panel — in particular why a
    /// directive that only names variants keeps the adaptive default rather
    /// than replacing it.
    pub font_hinting: Option<crate::rendering::font_config::FontHintingDirective>,
    /// Messages captured from `log_info`/`log_warn`/`log_error` calls during
    /// this run, in call order (each prefixed with its level, e.g.
    /// `"[warn] ..."`). In addition to — not a replacement for — the
    /// existing `tracing` calls those hooks make; this is for
    /// authoring-time diagnostics (`ScreenStore::render`) that need the
    /// log output back in the response rather than in the server's log
    /// stream.
    pub logs: Vec<String>,
}

/// Cap on `log_*` lines captured per script run. Guards against a script
/// logging inside a tight loop building an unbounded `Vec<String>` for a
/// single render — the production `/api/display` path captures logs too
/// (via `run_script`'s always-present sink) even though nothing reads
/// `ScriptResult::logs` there today. Once hit, further lines are dropped;
/// a single truncation marker is appended so a caller that *does* read
/// `logs` (i.e. `ScreenStore::render`) can tell output was cut off.
const MAX_LOG_ENTRIES: usize = 500;

/// Push a captured `log_*` line onto `sink`, capped at `MAX_LOG_ENTRIES`
/// (see its doc comment).
fn push_log(sink: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut logs) = sink.lock() {
        if logs.len() < MAX_LOG_ENTRIES {
            logs.push(line);
        } else if logs.len() == MAX_LOG_ENTRIES {
            logs.push(format!(
                "[warn] log capture truncated at {MAX_LOG_ENTRIES} entries"
            ));
        }
    }
}

/// Translate a Lua options table into the three typed structs the pipeline
/// needs. Unknown `preset` and `fit` values are errors, never silent no-ops:
/// a typo that silently does nothing is worse than one that fails loudly.
/// Everything one Lua HTTP call needs, as plain owned data.
///
/// It is collected in the Lua callback and then handed to a thread of our
/// own, because `reqwest::blocking` builds a private tokio runtime and tokio
/// panics if such a runtime is dropped while a tokio context is active.
///
/// Byonk's server paths happen to call Lua from inside `spawn_blocking`,
/// where blocking is permitted, so they never hit it. `byonk render` drives
/// the same code straight from `#[tokio::main]`, where it is not — so no
/// screen that fetched anything had ever rendered from the command line.
/// Doing the request off-runtime fixes it once, here, rather than leaving
/// every caller to remember a rule that fails silently when forgotten.
struct HttpRequestSpec {
    url: String,
    method: String,
    timeout_secs: u64,
    follow_redirects: bool,
    max_redirects: usize,
    danger_accept_invalid_certs: bool,
    ca_cert_path: Option<String>,
    client_cert_path: Option<String>,
    client_key_path: Option<String>,
    params: Option<Vec<(String, String)>>,
    headers: Option<Vec<(String, String)>>,
    basic_auth: Option<(String, String)>,
    body: Option<String>,
    /// The body came from the `json` option, so it carries a JSON content type.
    body_is_json: bool,
}

/// A reply that arrived. The status is kept because a script otherwise cannot
/// tell an error page from data — `http_get` returns only the body.
struct HttpOutcome {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// Build the client, send the request, read the body. Runs on the worker
/// thread, never on a tokio thread.
fn send_http_request(spec: HttpRequestSpec) -> Result<HttpOutcome, String> {
    let mut client_builder = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(spec.timeout_secs));

    client_builder = if spec.follow_redirects {
        client_builder.redirect(reqwest::redirect::Policy::limited(spec.max_redirects))
    } else {
        client_builder.redirect(reqwest::redirect::Policy::none())
    };

    if spec.danger_accept_invalid_certs {
        tracing::warn!(url = %spec.url, "Accepting invalid TLS certificates - this is insecure!");
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }

    if let Some(ref ca_path) = spec.ca_cert_path {
        let ca_data = std::fs::read(ca_path)
            .map_err(|e| format!("Failed to read CA certificate file '{ca_path}': {e}"))?;
        let ca_cert = reqwest::Certificate::from_pem(&ca_data)
            .map_err(|e| format!("Failed to parse CA certificate: {e}"))?;
        client_builder = client_builder.add_root_certificate(ca_cert);
        tracing::debug!(ca_cert = %ca_path, "Added custom CA certificate");
    }

    match (&spec.client_cert_path, &spec.client_key_path) {
        (Some(cert_path), Some(key_path)) => {
            let cert_data = std::fs::read(cert_path).map_err(|e| {
                format!("Failed to read client certificate file '{cert_path}': {e}")
            })?;
            let key_data = std::fs::read(key_path)
                .map_err(|e| format!("Failed to read client key file '{key_path}': {e}"))?;
            let mut pem_buffer = cert_data;
            pem_buffer.push(b'\n');
            pem_buffer.extend_from_slice(&key_data);
            let identity = reqwest::Identity::from_pem(&pem_buffer)
                .map_err(|e| format!("Failed to create client identity from cert/key: {e}"))?;
            client_builder = client_builder.identity(identity);
            tracing::debug!(client_cert = %cert_path, client_key = %key_path, "Added client certificate for mTLS");
        }
        (None, None) => {}
        _ => {
            return Err(
                "Both client_cert and client_key must be provided together for mTLS".to_string(),
            )
        }
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut request = match spec.method.to_uppercase().as_str() {
        "GET" => client.get(&spec.url),
        "POST" => client.post(&spec.url),
        "PUT" => client.put(&spec.url),
        "DELETE" => client.delete(&spec.url),
        "PATCH" => client.patch(&spec.url),
        "HEAD" => client.head(&spec.url),
        other => return Err(format!("Unsupported HTTP method: {other}")),
    };

    if let Some(ref params) = spec.params {
        request = request.query(params);
    }
    if let Some(ref headers) = spec.headers {
        for (name, value) in headers {
            request = request.header(name, value);
        }
    }
    if let Some((ref username, ref password)) = spec.basic_auth {
        request = request.basic_auth(username, Some(password));
    }
    if let Some(ref body) = spec.body {
        if spec.body_is_json {
            request = request.header("Content-Type", "application/json");
        }
        request = request.body(body.clone());
    }

    let response = request
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = response
        .bytes()
        .map_err(|e| format!("Failed to read response: {e}"))?
        .to_vec();

    Ok(HttpOutcome {
        status,
        headers,
        body,
    })
}

/// Read a Lua options table into an `HttpRequestSpec`, and work out the cache
/// key while the pieces are still to hand. Shared by `http_request` and
/// `http_response` so the two cannot drift on what an option means.
fn build_http_spec(
    lua: &Lua,
    url: String,
    options: Option<Table>,
) -> LuaResult<(HttpRequestSpec, Option<u64>, Option<String>)> {
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

    let method = options
        .as_ref()
        .and_then(|opts| opts.get::<String>("method").ok())
        .unwrap_or_else(|| "GET".to_string());

    tracing::debug!(url = %url, method = %method, "Lua http_request");

    let mut spec = HttpRequestSpec {
        url,
        method,
        timeout_secs: 30,
        follow_redirects: true,
        max_redirects: 10,
        danger_accept_invalid_certs: false,
        ca_cert_path: None,
        client_cert_path: None,
        client_key_path: None,
        params: None,
        headers: None,
        basic_auth: None,
        body: None,
        body_is_json: false,
    };
    let mut cache_ttl: Option<u64> = None;

    if let Some(ref opts) = options {
        for key in opts
            .clone()
            .pairs::<String, Value>()
            .flatten()
            .map(|(k, _)| k)
        {
            if !KNOWN_OPTIONS.contains(&key.as_str()) {
                tracing::warn!(
                    option = %key,
                    "http_request: unknown option (valid options: {})",
                    KNOWN_OPTIONS.join(", ")
                );
            }
        }

        if let Ok(t) = opts.get::<u64>("timeout") {
            spec.timeout_secs = t;
        }
        if let Ok(f) = opts.get::<bool>("follow_redirects") {
            spec.follow_redirects = f;
        }
        if let Ok(m) = opts.get::<usize>("max_redirects") {
            spec.max_redirects = m;
        }
        if let Ok(d) = opts.get::<bool>("danger_accept_invalid_certs") {
            spec.danger_accept_invalid_certs = d;
        }
        if let Ok(ca) = opts.get::<String>("ca_cert") {
            spec.ca_cert_path = Some(ca);
        }
        if let Ok(cert) = opts.get::<String>("client_cert") {
            spec.client_cert_path = Some(cert);
        }
        if let Ok(key) = opts.get::<String>("client_key") {
            spec.client_key_path = Some(key);
        }
        if let Ok(ttl) = opts.get::<u64>("cache_ttl") {
            cache_ttl = Some(ttl);
        }

        if let Ok(params_table) = opts.get::<Table>("params") {
            let params: Vec<(String, String)> = params_table
                .pairs::<String, Value>()
                .flatten()
                .map(|(k, v)| {
                    let v_str = match v {
                        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                        Value::Integer(i) => i.to_string(),
                        Value::Number(n) => n.to_string(),
                        Value::Boolean(b) => b.to_string(),
                        _ => String::new(),
                    };
                    (k, v_str)
                })
                .collect();
            spec.params = Some(params);
        }

        if let Ok(headers_table) = opts.get::<Table>("headers") {
            spec.headers = Some(headers_table.pairs::<String, String>().flatten().collect());
        }

        if let Ok(auth_table) = opts.get::<Table>("basic_auth") {
            let username: String = auth_table.get("username").unwrap_or_default();
            let password: String = auth_table.get("password").unwrap_or_default();
            if !username.is_empty() {
                spec.basic_auth = Some((username, password));
            }
        }

        // `json` takes precedence over `body`.
        if let Ok(json_table) = opts.get::<Table>("json") {
            let json_value = lua_value_to_json(lua, Value::Table(json_table))?;
            let json_str = serde_json::to_string(&json_value)
                .map_err(|e| mlua::Error::external(format!("JSON encode error: {e}")))?;
            spec.body = Some(json_str);
            spec.body_is_json = true;
        } else if let Ok(body) = opts.get::<String>("body") {
            spec.body = Some(body);
        }
    }

    let cache_key = cache_ttl.map(|_| {
        super::http_cache::compute_cache_key(
            &spec.url,
            &spec.method,
            spec.params.as_deref(),
            spec.headers.as_deref(),
            spec.body.as_deref(),
        )
    });

    Ok((spec, cache_ttl, cache_key))
}

/// Run `send_http_request` on a thread with no tokio context, whatever
/// context byonk itself was called from. See `HttpRequestSpec`.
fn send_http_request_off_runtime(spec: HttpRequestSpec) -> Result<HttpOutcome, String> {
    std::thread::spawn(move || send_http_request(spec))
        .join()
        .map_err(|_| "the HTTP worker thread panicked".to_string())?
}

fn parse_image_opts(
    opts: Option<&Table>,
    palette_hex: Option<&[String]>,
    log_sink: &Arc<Mutex<Vec<String>>>,
) -> Result<
    (
        crate::services::image_process::GeometryOpts,
        eink_photo::Params,
        crate::services::image_process::OutputFormat,
    ),
    String,
> {
    use crate::services::image_process::{Fit, GeometryOpts, OutputFormat};

    let Some(t) = opts else {
        return Ok((
            GeometryOpts::default(),
            eink_photo::Params::default(),
            OutputFormat::Png,
        ));
    };

    let num = |k: &str| -> Option<f32> { t.get::<f32>(k).ok() };
    let flag = |k: &str| -> Option<bool> {
        match t.get::<Value>(k) {
            Ok(Value::Boolean(b)) => Some(b),
            _ => None,
        }
    };

    // --- geometry ---
    let crop = match t.get::<Table>("crop") {
        Ok(c) => Some((
            c.get::<f32>("x").unwrap_or(0.0),
            c.get::<f32>("y").unwrap_or(0.0),
            c.get::<f32>("w")
                .map_err(|_| "crop.w is required".to_string())?,
            c.get::<f32>("h")
                .map_err(|_| "crop.h is required".to_string())?,
        )),
        Err(_) => None,
    };
    let fit = match t.get::<String>("fit").ok().as_deref() {
        None | Some("cover") => Fit::Cover,
        Some("contain") => Fit::Contain,
        Some("stretch") => Fit::Stretch,
        Some("none") => Fit::None,
        Some(other) => {
            return Err(format!(
                "unknown fit {other:?}; expected cover, contain, stretch or none"
            ))
        }
    };
    let geometry = GeometryOpts {
        crop,
        fit,
        width: t.get::<u32>("width").ok(),
        height: t.get::<u32>("height").ok(),
    };

    // --- tone params ---
    let preset = match t.get::<String>("preset").ok().as_deref() {
        None | Some("none") => eink_photo::Preset::None,
        Some("eink") => eink_photo::Preset::Eink,
        Some(other) => return Err(format!("unknown preset {other:?}; expected eink or none")),
    };

    let curve = match t.get::<Table>("curve") {
        Ok(c) => {
            let mut pts = Vec::new();
            for i in 1..=c.raw_len() {
                let pair: Table = c
                    .raw_get(i)
                    .map_err(|_| "curve entries must be {input, output} pairs".to_string())?;
                let x: f32 = pair
                    .raw_get(1)
                    .map_err(|_| "curve point missing input".to_string())?;
                let y: f32 = pair
                    .raw_get(2)
                    .map_err(|_| "curve point missing output".to_string())?;
                pts.push((x, y));
            }
            Some(pts)
        }
        Err(_) => None,
    };

    let sharpen = match t.get::<Table>("sharpen") {
        Ok(s) => Some(eink_photo::Sharpen {
            amount: s.get::<f32>("amount").unwrap_or(40.0),
            radius: s.get::<f32>("radius").unwrap_or(1.0),
        }),
        Err(_) => None,
    };

    // --- palette_aware ---
    let output_endpoints = if flag("palette_aware").unwrap_or(false) {
        match palette_hex {
            Some(hex) if !hex.is_empty() => {
                let rgb = crate::api::display::parse_colors_header(&hex.join(","));
                eink_photo::palette_endpoints(&rgb)
            }
            _ => {
                push_log(
                    log_sink,
                    "[warn] image_process: palette_aware was requested but this device \
                     has no palette; ignoring it"
                        .to_string(),
                );
                None
            }
        }
    } else {
        None
    };

    let params = eink_photo::Params {
        preset,
        exposure: num("exposure"),
        temperature: num("temperature"),
        tint: num("tint"),
        auto_levels: flag("auto_levels"),
        blacks: num("blacks"),
        whites: num("whites"),
        highlights: num("highlights"),
        shadows: num("shadows"),
        contrast: num("contrast"),
        curve,
        clarity: num("clarity"),
        vibrance: num("vibrance"),
        saturation: num("saturation"),
        grayscale: flag("grayscale"),
        invert: flag("invert"),
        sharpen,
        output_endpoints,
    };

    // --- output format ---
    let format = match t.get::<String>("format").ok().as_deref() {
        None | Some("png") => OutputFormat::Png,
        Some("jpeg") | Some("jpg") => OutputFormat::Jpeg {
            quality: t.get::<u8>("quality").unwrap_or(90),
        },
        Some(other) => return Err(format!("unknown format {other:?}; expected png or jpeg")),
    };

    Ok((geometry, params, format))
}

/// Error type for Lua script execution
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    #[error("Script not found: {0}")]
    NotFound(String),

    /// The script's `font_hinting` directive could not be understood.
    ///
    /// Deliberately an error rather than a silent default: the neighbouring
    /// `error_clamp`/`noise_scale` parsers use `.ok()`, which swallows a
    /// malformed value, and a mistyped hinting target would then render as
    /// something the author never asked for with nothing said about it.
    #[error("font_hinting: {0}")]
    FontHinting(String),
}

/// Parses the optional `font_hinting` directive off a script's return table.
///
/// Every failure is an error naming the offending value. That is a deliberate
/// break from the neighbouring `error_clamp` / `noise_scale` parsers, which use
/// `.ok()` and silently drop a malformed value: a mistyped hinting target would
/// otherwise render as something the author never asked for, with nothing said.
///
/// `known_families` is what the renderer's fontdb actually holds. Variant base
/// families are checked against it here because the font resolver cannot report
/// a miss — `select_font` falls through to the default selector when
/// `db.query` finds nothing, so an unresolvable base family silently lands
/// wherever unresolved families land, which is the generic mapping.
fn parse_font_hinting(
    result: &Table,
    known_families: &HashMap<String, Vec<FontFaceInfo>>,
) -> Result<Option<crate::rendering::font_config::FontHintingDirective>, String> {
    use crate::rendering::font_config::FontHintingDirective;

    let raw: Value = result
        .get("font_hinting")
        .map_err(|e| format!("could not be read: {e}"))?;

    let table = match raw {
        Value::Nil => return Ok(None),
        Value::Boolean(false) => {
            return Ok(Some(FontHintingDirective {
                default: Some(None),
                variants: Default::default(),
            }))
        }
        Value::Boolean(true) => {
            return Err(
                "`true` says nothing about how to hint. Omit font_hinting to get the \
                        server's adaptive default, use `false` to turn hinting off, or give a \
                        table."
                    .to_string(),
            )
        }
        Value::Table(t) => t,
        other => {
            return Err(format!(
                "expected a table or false, got {}",
                other.type_name()
            ))
        }
    };

    let engine = match table.get::<Value>("engine").map_err(|e| e.to_string())? {
        Value::Nil => crate::rendering::font_config::HintingEngine::Auto,
        Value::String(s) => parse_engine(&s.to_string_lossy())?,
        other => {
            return Err(format!(
                "engine must be a string, got {}",
                other.type_name()
            ))
        }
    };

    // A directive that says nothing about the target leaves the adaptive
    // default in place; only an explicit target replaces it.
    let default = match table.get::<Value>("target").map_err(|e| e.to_string())? {
        Value::Nil => None,
        Value::Boolean(false) => Some(None),
        v => Some(Some(crate::rendering::font_config::HintingSpec {
            engine,
            target: parse_target(v)?,
        })),
    };

    let variants = parse_variants(&table, known_families)?;

    Ok(Some(FontHintingDirective { default, variants }))
}

fn parse_engine(s: &str) -> Result<crate::rendering::font_config::HintingEngine, String> {
    use crate::rendering::font_config::HintingEngine;
    match s {
        "interpreter" => Ok(HintingEngine::Interpreter),
        "auto" => Ok(HintingEngine::Auto),
        "auto_fallback" => Ok(HintingEngine::AutoFallback),
        other => Err(format!(
            "unknown engine {other:?} — expected \"interpreter\", \"auto\" or \"auto_fallback\""
        )),
    }
}

/// Parses a `target`, which is either a shorthand string or a table whose
/// `mode` picks the style. `mode` is the discriminator so that mono's extra
/// knob (`aliased`) and smooth's two (`symmetric`, `preserve_linear_metrics`)
/// each have a home.
fn parse_target(v: Value) -> Result<crate::rendering::font_config::HintingTarget, String> {
    use crate::rendering::font_config::{HintingMode, HintingTarget};

    // Smooth's defaults match what the adaptive default gives a grey panel, so
    // `target = "smooth"` produces the same thing byonk would have chosen.
    const SMOOTH_SYMMETRIC: bool = false;
    const SMOOTH_PRESERVE: bool = true;

    let (mode, table) = match v {
        Value::String(s) => (s.to_string_lossy().to_string(), None),
        Value::Table(t) => {
            let mode = match t.get::<Value>("mode").map_err(|e| e.to_string())? {
                Value::Nil => "smooth".to_string(),
                Value::String(s) => s.to_string_lossy().to_string(),
                other => {
                    return Err(format!(
                        "target mode must be a string, got {}",
                        other.type_name()
                    ))
                }
            };
            (mode, Some(t))
        }
        other => {
            return Err(format!(
                "target must be a string or a table, got {}",
                other.type_name()
            ))
        }
    };

    let get_bool = |key: &str, fallback: bool| -> Result<bool, String> {
        match &table {
            None => Ok(fallback),
            Some(t) => match t.get::<Value>(key).map_err(|e| e.to_string())? {
                Value::Nil => Ok(fallback),
                Value::Boolean(b) => Ok(b),
                other => Err(format!(
                    "target {key} must be a boolean, got {}",
                    other.type_name()
                )),
            },
        }
    };

    let smooth = |mode: HintingMode| -> Result<HintingTarget, String> {
        Ok(HintingTarget::Smooth {
            mode,
            symmetric_rendering: get_bool("symmetric", SMOOTH_SYMMETRIC)?,
            preserve_linear_metrics: get_bool("preserve_linear_metrics", SMOOTH_PRESERVE)?,
        })
    };

    match mode.as_str() {
        // Aliasing defaults on because mono hinting is what makes aliasing
        // safe, and asking for mono on its own is almost always asking for
        // crisp 1-bit text. `aliased = false` is there for a grey panel that
        // still wants stems on the grid.
        "mono" => Ok(HintingTarget::Mono {
            aliased: get_bool("aliased", true)?,
        }),
        "smooth" | "normal" => smooth(HintingMode::Normal),
        "light" => smooth(HintingMode::Light),
        "lcd" => smooth(HintingMode::Lcd),
        "vertical_lcd" => smooth(HintingMode::VerticalLcd),
        other => Err(format!(
            "unknown target {other:?} — expected \"mono\", \"smooth\", \"light\", \"lcd\" or \
             \"vertical_lcd\""
        )),
    }
}

fn parse_variants(
    table: &Table,
    known_families: &HashMap<String, Vec<FontFaceInfo>>,
) -> Result<std::collections::BTreeMap<String, crate::rendering::font_config::FontVariant>, String>
{
    use crate::rendering::font_config::{FontVariant, HintingSpec};

    let variants_table = match table.get::<Value>("variants").map_err(|e| e.to_string())? {
        Value::Nil => return Ok(Default::default()),
        Value::Table(t) => t,
        other => {
            return Err(format!(
                "variants must be a table, got {}",
                other.type_name()
            ))
        }
    };

    let mut out = std::collections::BTreeMap::new();
    for pair in variants_table.pairs::<String, Value>() {
        let (alias, value) = pair.map_err(|e| e.to_string())?;

        // The alias is a name `select_font` intercepts before the default
        // selector runs. If it is also a real family, the interception shadows
        // that family and every element asking for it silently gets something
        // else.
        if known_families.contains_key(&alias) {
            return Err(format!(
                "variant name {alias:?} is already an installed font family. A variant name is \
                 an alias you invent for byonk to intercept, so it must not be a real family — \
                 name it for its purpose instead, e.g. \"Crisp Body\"."
            ));
        }

        let vt = match value {
            Value::Table(t) => t,
            other => {
                return Err(format!(
                    "variant {alias:?} must be a table, got {}",
                    other.type_name()
                ))
            }
        };

        let font = match vt.get::<Value>("font").map_err(|e| e.to_string())? {
            Value::String(s) => s.to_string_lossy().to_string(),
            Value::Nil => {
                return Err(format!(
                    "variant {alias:?} has no `font` — a variant needs the family it is a \
                     variant of"
                ))
            }
            other => {
                return Err(format!(
                    "variant {alias:?} font must be a string, got {}",
                    other.type_name()
                ))
            }
        };
        if !known_families.contains_key(&font) {
            return Err(format!(
                "variant {alias:?} names font {font:?}, which is not installed. \
                 `fonts.families()` lists what this server has."
            ));
        }

        let strikes = match vt.get::<Value>("strikes").map_err(|e| e.to_string())? {
            Value::Nil => None,
            Value::Boolean(b) => Some(b),
            other => {
                return Err(format!(
                    "variant {alias:?} strikes must be a boolean, got {}",
                    other.type_name()
                ))
            }
        };

        // Outer None = inherit the document default; Some(None) = off for this
        // variant only.
        let hinting = match vt.get::<Value>("hinting").map_err(|e| e.to_string())? {
            Value::Nil => None,
            Value::Boolean(false) => Some(None),
            Value::Boolean(true) => {
                return Err(format!(
                    "variant {alias:?} hinting `true` says nothing — omit it to inherit, use \
                     `false` to turn hinting off, or give a table"
                ))
            }
            Value::Table(ht) => {
                let engine = match ht.get::<Value>("engine").map_err(|e| e.to_string())? {
                    Value::Nil => crate::rendering::font_config::HintingEngine::Auto,
                    Value::String(s) => parse_engine(&s.to_string_lossy())?,
                    other => {
                        return Err(format!(
                            "variant {alias:?} engine must be a string, got {}",
                            other.type_name()
                        ))
                    }
                };
                let target = match ht.get::<Value>("target").map_err(|e| e.to_string())? {
                    Value::Nil => {
                        return Err(format!("variant {alias:?} hinting table has no `target`"))
                    }
                    v => parse_target(v)?,
                };
                Some(Some(HintingSpec { engine, target }))
            }
            other => {
                return Err(format!(
                    "variant {alias:?} hinting must be a table or false, got {}",
                    other.type_name()
                ))
            }
        };

        out.insert(
            alias,
            FontVariant {
                font,
                strikes,
                hinting,
            },
        );
    }
    Ok(out)
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

/// A [`ScreenRepoSource`] backed by the flat screens directory of an `AssetLoader`.
/// Used by [`LuaRuntime::run_script_from_asset`] for tests/dev where scripts live
/// directly under `SCREENS_DIR` rather than inside a screen repo.
struct AssetScreensSource {
    loader: Arc<AssetLoader>,
}

impl ScreenRepoSource for AssetScreensSource {
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        self.loader
            .read_screen(std::path::Path::new(rel))
            .ok()
            .map(|b| b.into_owned())
    }

    fn screen_paths(&self) -> Vec<String> {
        Vec::new()
    }

    fn svg_files(&self) -> Vec<String> {
        Vec::new()
    }

    fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
        unreachable!("AssetScreensSource::manifest() is not used")
    }

    fn screen_files(&self, _screen_path: &str) -> Vec<String> {
        Vec::new()
    }
}

/// Base-library globals a screen script has no business calling. Each one
/// opens a file by name, which is the reach this sandbox exists to deny;
/// `load` additionally compiles attacker-chosen bytecode, and crafted bytecode
/// escapes the VM outright. `require` is not among them — `install_require`
/// replaces it with a resolver confined to the screen repo and `byonk-base`.
const DENIED_BASE_GLOBALS: [&str; 3] = ["dofile", "loadfile", "load"];

/// `os` members a screen script has no business calling. `exit` would take the
/// whole server down with one screen; `getenv` reads the process environment,
/// which under the Home Assistant app is where byonk's own configuration
/// lives. What stays is the clock: `time`, `date`, `clock`, `difftime`.
const DENIED_OS_MEMBERS: [&str; 7] = [
    "execute",
    "exit",
    "getenv",
    "remove",
    "rename",
    "setlocale",
    "tmpname",
];

/// Build the Lua VM a screen script runs in.
///
/// `Lua::new()` loads `StdLib::ALL_SAFE`, and "safe" there means only that
/// `debug` and `ffi` stay out — `io`, `os` and `package` are all in. Screens
/// come from screen repos that byonk re-fetches on a timer, so an upstream
/// change runs new Lua unreviewed; a screen that can call `io.open` can write
/// anywhere the byonk process can, which under the Home Assistant app means
/// the mapped host directories. So the library set is named explicitly and the
/// leftover sharp edges are removed from the globals.
fn new_sandboxed_lua() -> LuaResult<Lua> {
    // `io` and `package` are left out wholesale. `package` costs nothing:
    // `install_require` supplies `require`, and mlua's safe mode already
    // refuses `package.loadlib` and C modules.
    let libs = StdLib::COROUTINE
        | StdLib::TABLE
        | StdLib::STRING
        | StdLib::UTF8
        | StdLib::MATH
        | StdLib::OS;
    let lua = Lua::new_with(libs, LuaOptions::default())?;

    let globals = lua.globals();
    for name in DENIED_BASE_GLOBALS {
        globals.set(name, Value::Nil)?;
    }
    let os: Table = globals.get("os")?;
    for name in DENIED_OS_MEMBERS {
        os.set(name, Value::Nil)?;
    }

    Ok(lua)
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

    /// Run a Lua script with the given parameters.
    ///
    /// `script_src` is the already-read `script.lua` contents. `source` is the
    /// screen's screen repo source, used to resolve `require()` for screen-repo-relative
    /// modules and `read_asset()` for sibling files. `screen_name` (a `handle/path`
    /// ref) is used for logging. `screen_dir` is the screen's screen-repo-relative
    /// directory, against which `read_asset(path)` reads through `source`.
    ///
    /// `caller_log_sink`, if given, is used as the `log_*` capture sink
    /// *instead of* an internally-created one — critically, it is a
    /// reference the caller still owns after this call returns, including
    /// when it returns `Err`. `lua.load(script_src).eval()?` below can
    /// short-circuit *before* `ScriptResult` (and its `logs` field) is
    /// built, which would otherwise discard every `log_*` call the script
    /// made before it failed — exactly the diagnostic an author debugging
    /// that failure needs most. `ScreenStore::render` passes its own sink
    /// and reads it directly in its error branches; every other caller
    /// passes `None` and is unaffected (an internal sink is created and
    /// still drained into `ScriptResult::logs` on success, unchanged from
    /// before).
    #[allow(clippy::too_many_arguments)]
    pub fn run_script(
        &self,
        script_src: &str,
        source: &Arc<dyn ScreenRepoSource>,
        screen_name: &str,
        screen_dir: &str,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
        timestamp_override: Option<i64>,
        caller_log_sink: Option<&Arc<Mutex<Vec<String>>>>,
    ) -> Result<ScriptResult, ScriptError> {
        let lua = new_sandboxed_lua()?;
        // Captures log_info/log_warn/log_error output for this run, in
        // addition to the tracing calls those hooks already make (see
        // `ScriptResult::logs`). Uses the caller's sink when given (see
        // `caller_log_sink` doc above) so logs survive an `Err` return;
        // otherwise creates one that only this call will ever see.
        let owned_sink: Arc<Mutex<Vec<String>>>;
        let log_sink: &Arc<Mutex<Vec<String>>> = match caller_log_sink {
            Some(sink) => sink,
            None => {
                owned_sink = Arc::new(Mutex::new(Vec::new()));
                &owned_sink
            }
        };

        // Set up the Lua environment
        self.setup_globals(
            &lua,
            params,
            device_ctx,
            screen_name,
            source,
            screen_dir,
            timestamp_override,
            log_sink,
        )?;

        // Install the sandboxed `require()` scoped to this screen repo + byonk-base.
        self.install_require(&lua, source)?;

        // Execute the script. `set_mode(Text)` is part of the sandbox, not a
        // formality: mlua sniffs the source and switches to the bytecode
        // loader by itself, and that loader trusts what it is given, so a
        // screen repo could ship crafted bytecode and walk straight out of the
        // VM. A screen's script.lua is source code; refuse to read it as
        // anything else. Same reasoning in `install_require`.
        let result: Table = lua.load(script_src).set_mode(ChunkMode::Text).eval()?;

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

        // Parse optional measured-colour array from script return. Same shape
        // as `colors` above: positive integer keys, empty means None.
        let colors_actual = result
            .get::<Table>("colors_actual")
            .ok()
            .map(|t| {
                (1..=t.raw_len())
                    .filter_map(|i| t.raw_get::<String>(i).ok())
                    .collect::<Vec<String>>()
            })
            .filter(|v| !v.is_empty());

        // Parse optional dither mode from script return
        let dither = result.get::<String>("dither").ok();

        // Parse optional dither tuning parameters from script return
        let error_clamp = result.get::<f32>("error_clamp").ok();
        let noise_scale = result.get::<f32>("noise_scale").ok();
        let chroma_clamp = result.get::<f32>("chroma_clamp").ok();
        let strength = result.get::<f32>("strength").ok();

        // Parse the optional gamut sub-table from the script return.
        let gamut = result
            .get::<Table>("gamut")
            .ok()
            .map(|t| crate::models::GamutTuningValues {
                knee: t.get::<f32>("knee").ok(),
                amount: t.get::<f32>("amount").ok(),
                max_compression: t.get::<f32>("max_compression").ok(),
            });

        let font_hinting =
            parse_font_hinting(&result, &self.font_families).map_err(ScriptError::FontHinting)?;

        // Warn where a variant escapes the document's aliasing. Pushed into the
        // script's own log sink so it reaches the author the same way their
        // `log_warn` calls do.
        if let Some(directive) = font_hinting.as_ref() {
            // The panel is not known here, and the state is only reachable when
            // the document is aliased mono — which the adaptive default gives a
            // black-and-white panel. Checking against grey_count 2 asks
            // "would this be wrong on the panel where it can be wrong?".
            let escaping = directive.variants_escaping_aliasing(2);
            if !escaping.is_empty() {
                if let Ok(mut sink) = log_sink.lock() {
                    sink.push(format!(
                        "[warn] font_hinting: on a black-and-white panel this screen's text is \
                         drawn 1-bit, and variant(s) {} turn off mono hinting. Glyph aliasing is \
                         per-document while hinting is per-face, so on such a panel their stems \
                         can drop out. Set text-rendering=\"optimizeLegibility\" on the elements \
                         using them to restore anti-aliasing while keeping hinting \
                         (geometricPrecision would disable hinting instead).",
                        escaping.join(", ")
                    ));
                }
            }
        }

        let logs = log_sink.lock().map(|g| g.clone()).unwrap_or_default();

        Ok(ScriptResult {
            data,
            refresh_rate,
            skip_update,
            colors,
            colors_actual,
            dither,
            error_clamp,
            noise_scale,
            chroma_clamp,
            strength,
            gamut,
            font_hinting,
            logs,
        })
    }

    /// Test/dev convenience: read a screen script from the `AssetLoader` by path
    /// and run it with a screens-dir-backed screen repo source (no screen repo manifest
    /// required). `require()` resolves sibling files under the screens dir.
    pub fn run_script_from_asset(
        &self,
        script_path: &std::path::Path,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
        timestamp_override: Option<i64>,
    ) -> Result<ScriptResult, ScriptError> {
        let script_src = self
            .asset_loader
            .read_screen_string(script_path)
            .map_err(|e| ScriptError::NotFound(e.to_string()))?;
        let source: Arc<dyn ScreenRepoSource> = Arc::new(AssetScreensSource {
            loader: self.asset_loader.clone(),
        });
        let screen_name = script_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default");
        // Screen dir is the script's parent directory (siblings resolve through it).
        let screen_dir = script_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .replace('\\', "/");
        self.run_script(
            &script_src,
            &source,
            screen_name,
            &screen_dir,
            params,
            device_ctx,
            timestamp_override,
            None,
        )
    }

    /// Install a sandboxed `require(name)` global that can only reach two places:
    /// the screen's own screen repo (screen-repo-relative modules like `require("lib/x")`)
    /// and the embedded `byonk-base` std library (`require("byonk-base-v1/std")`).
    ///
    /// A `package.loaded`-style cache in the Lua registry ensures each module is
    /// evaluated at most once per run and returns the same value on repeat calls.
    fn install_require(&self, lua: &Lua, source: &Arc<dyn ScreenRepoSource>) -> LuaResult<()> {
        // Module cache (package.loaded-style), keyed by the require name.
        let cache = lua.create_table()?;
        lua.set_named_registry_value("__require_cache", cache)?;

        let require_source = source.clone();
        let require_assets = self.asset_loader.clone();
        let require_fn = lua.create_function(move |lua, name: String| -> LuaResult<Value> {
            // 1. Return cached module if already loaded.
            let cache: Table = lua.named_registry_value("__require_cache")?;
            let cached: Value = cache.get(name.clone())?;
            if !matches!(cached, Value::Nil) {
                return Ok(cached);
            }

            // 2/3. Resolve the module source.
            let code = if let Some(base_rel) = name.strip_prefix("byonk-base-") {
                // byonk-base-v1/std -> read_base_string("v1/std.lua")
                let rel = if base_rel.ends_with(".lua") {
                    base_rel.to_string()
                } else {
                    format!("{base_rel}.lua")
                };
                require_assets.read_base_string(&rel)
            } else {
                // screen-repo-relative: lib/util -> read_string("lib/util.lua")
                let rel = if name.ends_with(".lua") {
                    name.clone()
                } else {
                    format!("{name}.lua")
                };
                require_source.read_string(&rel)
            };

            // 4. Miss -> Lua error.
            let code =
                code.ok_or_else(|| mlua::Error::external(format!("module '{name}' not found")))?;

            // 5. Evaluate, cache, return.
            // Text only — see the `set_mode` note in `run_script`.
            let value: Value = lua
                .load(&code)
                .set_name(&name)
                .set_mode(ChunkMode::Text)
                .eval()?;
            cache.set(name.clone(), value.clone())?;
            Ok(value)
        })?;
        lua.globals().set("require", require_fn)?;
        Ok(())
    }

    /// Set up Lua global functions and variables
    #[allow(clippy::too_many_arguments)]
    fn setup_globals(
        &self,
        lua: &Lua,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
        _screen_name: &str,
        source: &Arc<dyn ScreenRepoSource>,
        screen_dir: &str,
        timestamp_override: Option<i64>,
        log_sink: &Arc<Mutex<Vec<String>>>,
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
            // Measured panel colours. Absent (nil in Lua) rather than mirrored
            // from `colors` when uncalibrated — see DeviceContext::colors_actual.
            if let Some(ref actual) = ctx.colors_actual {
                let actual_table = lua.create_table()?;
                for (i, color) in actual.iter().enumerate() {
                    actual_table.set(i + 1, color.as_str())?;
                }
                device_table.set("colors_actual", actual_table)?;
            }
            // Add dither sub-table with pre-script resolved values
            let dither_table = lua.create_table()?;
            if let Some(ref algo) = ctx.dither_algorithm {
                dither_table.set("algorithm", algo.as_str())?;
            }
            if let Some(ec) = ctx.dither_error_clamp {
                dither_table.set("error_clamp", ec)?;
            }
            if let Some(ns) = ctx.dither_noise_scale {
                dither_table.set("noise_scale", ns)?;
            }
            if let Some(cc) = ctx.dither_chroma_clamp {
                dither_table.set("chroma_clamp", cc)?;
            }
            if let Some(st) = ctx.dither_strength {
                dither_table.set("strength", st)?;
            }
            let gamut_table = lua.create_table()?;
            if let Some(v) = ctx.dither_gamut_knee {
                gamut_table.set("knee", v)?;
            }
            if let Some(v) = ctx.dither_gamut_amount {
                gamut_table.set("amount", v)?;
            }
            if let Some(v) = ctx.dither_gamut_max_compression {
                gamut_table.set("max_compression", v)?;
            }
            dither_table.set("gamut", gamut_table)?;
            device_table.set("dither", dither_table)?;
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
        // Reads a file sibling to the screen's `script.lua`, screen-repo-relative to
        // `screen_dir`, through the screen's screen repo source. The source applies the
        // `is_safe_rel` sandbox guard, so `..` traversal is rejected.
        let asset_source = source.clone();
        let asset_dir = screen_dir.to_string();
        let read_asset = lua.create_function(move |lua, path: String| {
            let rel = crate::services::screen_repo_loader::join_rel(&asset_dir, &path);
            match asset_source.read(&rel) {
                Some(data) => {
                    // Return as Lua string (which can contain binary data)
                    lua.create_string(&data)
                }
                None => Err(mlua::Error::external(format!(
                    "Failed to read asset: {rel}"
                ))),
            }
        })?;
        globals.set("read_asset", read_asset)?;

        // image_process(bytes, opts) -> data_uri, width, height
        //
        // One call, fixed order (see the eink-photo crate docs). Raises a Lua
        // error on failure, matching http_get's contract and the `pcall`
        // idiom the examples use.
        let ctx_palette_hex: Option<Vec<String>> =
            device_ctx.and_then(|c| c.colors_actual.clone().or_else(|| c.colors.clone()));
        let img_log_sink = log_sink.clone();
        let image_process =
            lua.create_function(move |_, (bytes, opts): (mlua::String, Option<Table>)| {
                let bytes = bytes.as_bytes();
                let (geometry, params, format) =
                    parse_image_opts(opts.as_ref(), ctx_palette_hex.as_deref(), &img_log_sink)
                        .map_err(mlua::Error::external)?;
                let (uri, w, h) = crate::services::image_process::process_image(
                    &bytes, &geometry, &params, format,
                )
                .map_err(mlua::Error::external)?;
                Ok((uri, w, h))
            })?;
        globals.set("image_process", image_process)?;

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
                let (spec, cache_ttl, cache_key) = build_http_spec(lua, url, options)?;

                if let Some(ref key) = cache_key {
                    if let Some(cached) = super::http_cache::get_cached(key) {
                        return lua.create_string(&cached);
                    }
                }

                let outcome = send_http_request_off_runtime(spec).map_err(mlua::Error::external)?;

                // Only a success is worth remembering. Caching an error page
                // would serve it as data for the whole TTL, and this function
                // gives the script no way to notice.
                if let (Some(key), Some(ttl)) = (cache_key, cache_ttl) {
                    if (200..300).contains(&outcome.status) {
                        super::http_cache::store_cached(key, outcome.body.clone(), ttl);
                    }
                }

                lua.create_string(&outcome.body)
            })?;
        globals.set("http_request", http_request.clone())?;

        // http_response(url, options?) -> table
        //
        // Same options as `http_request`, but returns the whole reply instead
        // of just the body, and does NOT raise when the request fails:
        //   ok      boolean   true for a 2xx status
        //   status  number    the HTTP status, or nil if no reply arrived
        //   body    string    the body, or nil if no reply arrived
        //   headers table      response headers, lowercased names
        //   error   string    why nothing arrived, or nil
        //
        // `http_get` hands back only the body, so a script cannot tell an
        // error page from data. Deciding what a 500 means belongs to the
        // script, so this reports and lets it choose.
        let http_response =
            lua.create_function(|lua, (url, options): (String, Option<Table>)| {
                let (spec, cache_ttl, cache_key) = build_http_spec(lua, url, options)?;
                let result = lua.create_table()?;

                // Only successes are cached, so a hit is always a success.
                if let Some(ref key) = cache_key {
                    if let Some(cached) = super::http_cache::get_cached(key) {
                        result.set("ok", true)?;
                        result.set("status", 200)?;
                        result.set("body", lua.create_string(&cached)?)?;
                        result.set("headers", lua.create_table()?)?;
                        result.set("from_cache", true)?;
                        return Ok(result);
                    }
                }

                match send_http_request_off_runtime(spec) {
                    Ok(outcome) => {
                        let ok = (200..300).contains(&outcome.status);
                        if let (Some(key), Some(ttl)) = (cache_key, cache_ttl) {
                            if ok {
                                super::http_cache::store_cached(key, outcome.body.clone(), ttl);
                            }
                        }
                        let headers = lua.create_table()?;
                        for (name, value) in &outcome.headers {
                            headers.set(name.as_str(), value.as_str())?;
                        }
                        result.set("ok", ok)?;
                        result.set("status", outcome.status)?;
                        result.set("body", lua.create_string(&outcome.body)?)?;
                        result.set("headers", headers)?;
                        result.set("from_cache", false)?;
                    }
                    Err(e) => {
                        // Nothing arrived: no status, no body, and a reason.
                        result.set("ok", false)?;
                        result.set("error", e)?;
                        result.set("headers", lua.create_table()?)?;
                        result.set("from_cache", false)?;
                    }
                }

                Ok(result)
            })?;
        globals.set("http_response", http_response)?;

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

        // Logging functions. Each appends to `log_sink` (for
        // `ScriptResult::logs`, capped — see `push_log`) in addition to its
        // existing tracing call.
        let sink = log_sink.clone();
        let log_info = lua.create_function(move |_, msg: String| {
            tracing::info!(script = true, "{}", msg);
            push_log(&sink, format!("[info] {msg}"));
            Ok(())
        })?;
        globals.set("log_info", log_info)?;

        let sink = log_sink.clone();
        let log_warn = lua.create_function(move |_, msg: String| {
            tracing::warn!(script = true, "{}", msg);
            push_log(&sink, format!("[warn] {msg}"));
            Ok(())
        })?;
        globals.set("log_warn", log_warn)?;

        let sink = log_sink.clone();
        let log_error = lua.create_function(move |_, msg: String| {
            tracing::error!(script = true, "{}", msg);
            push_log(&sink, format!("[error] {msg}"));
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

#[cfg(test)]
mod require_tests {
    use super::*;
    use crate::services::screen_repo_loader::ScreenRepoSource;
    use std::sync::Arc;

    /// Minimal in-memory screen repo source for exercising `require()`.
    struct MockSource;
    impl ScreenRepoSource for MockSource {
        fn read(&self, rel: &str) -> Option<Vec<u8>> {
            match rel {
                "lib/util.lua" => Some(b"return { greet = function() return 'hi' end }".to_vec()),
                _ => None,
            }
        }
        fn screen_paths(&self) -> Vec<String> {
            vec![]
        }
        fn svg_files(&self) -> Vec<String> {
            vec![]
        }
        fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
            unreachable!("manifest() not used by require()")
        }
        fn screen_files(&self, _screen_path: &str) -> Vec<String> {
            vec![]
        }
    }

    #[test]
    fn test_require_resolves_package_relative_module() {
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(MockSource);
        let script = "local u = require('lib/util'); return { data = { m = u.greet() } }";
        let res = rt
            .run_script(script, &src, "t", "", &Default::default(), None, None, None)
            .unwrap();
        assert_eq!(res.data["m"], serde_json::json!("hi"));
    }

    #[test]
    fn test_require_caches_module() {
        // A module with a side-effecting counter must only evaluate once.
        struct CounterSource;
        impl ScreenRepoSource for CounterSource {
            fn read(&self, rel: &str) -> Option<Vec<u8>> {
                match rel {
                    "lib/c.lua" => Some(b"COUNT = (COUNT or 0) + 1; return COUNT".to_vec()),
                    _ => None,
                }
            }
            fn screen_paths(&self) -> Vec<String> {
                vec![]
            }
            fn svg_files(&self) -> Vec<String> {
                vec![]
            }
            fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
                unreachable!()
            }
            fn screen_files(&self, _screen_path: &str) -> Vec<String> {
                vec![]
            }
        }
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(CounterSource);
        let script =
            "local a = require('lib/c'); local b = require('lib/c'); return { data = { a = a, b = b } }";
        let res = rt
            .run_script(script, &src, "t", "", &Default::default(), None, None, None)
            .unwrap();
        assert_eq!(res.data["a"], serde_json::json!(1));
        assert_eq!(res.data["b"], serde_json::json!(1));
    }

    #[test]
    fn test_require_missing_module_errors() {
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(MockSource);
        let script = "local x = require('nope/missing'); return { data = {} }";
        let err = rt
            .run_script(script, &src, "t", "", &Default::default(), None, None, None)
            .unwrap_err();
        assert!(
            err.to_string().contains("module 'nope/missing' not found"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_require_traversal_is_blocked() {
        // A malicious `require("../escape")` must not read host files; the screen repo
        // source rejects the `..` path, so `require` reports "module not found".
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(MockSource);
        let script = "local x = require('../escape'); return { data = {} }";
        let err = rt
            .run_script(script, &src, "t", "", &Default::default(), None, None, None)
            .unwrap_err();
        assert!(
            err.to_string().contains("module '../escape' not found"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_read_asset_reads_sibling_via_source() {
        // read_asset resolves screen-repo-relative to screen_dir through the source.
        struct AssetSource;
        impl ScreenRepoSource for AssetSource {
            fn read(&self, rel: &str) -> Option<Vec<u8>> {
                match rel {
                    "s/data.txt" => Some(b"hello-asset".to_vec()),
                    _ => None,
                }
            }
            fn screen_paths(&self) -> Vec<String> {
                vec![]
            }
            fn svg_files(&self) -> Vec<String> {
                vec![]
            }
            fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
                unreachable!()
            }
            fn screen_files(&self, _screen_path: &str) -> Vec<String> {
                vec![]
            }
        }
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(AssetSource);
        let script = "return { data = { c = read_asset('data.txt') } }";
        let res = rt
            .run_script(
                script,
                &src,
                "acme/s",
                "s",
                &Default::default(),
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(res.data["c"], serde_json::json!("hello-asset"));
    }

    #[test]
    fn test_read_asset_traversal_is_blocked() {
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(MockSource);
        let script = "return { data = { c = read_asset('../../etc/passwd') } }";
        let err = rt
            .run_script(
                script,
                &src,
                "t",
                "s",
                &Default::default(),
                None,
                None,
                None,
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("Failed to read asset"),
            "unexpected error: {err}"
        );
    }
}

#[cfg(test)]
mod gamut_tests {
    use super::*;
    use crate::services::screen_repo_loader::ScreenRepoSource;
    use std::sync::Arc;

    /// Screen repo source with no files: these scripts never `require()`.
    struct EmptySource;
    impl ScreenRepoSource for EmptySource {
        fn read(&self, _rel: &str) -> Option<Vec<u8>> {
            None
        }
        fn screen_paths(&self) -> Vec<String> {
            vec![]
        }
        fn svg_files(&self) -> Vec<String> {
            vec![]
        }
        fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
            unreachable!("manifest() not used by these tests")
        }
        fn screen_files(&self, _screen_path: &str) -> Vec<String> {
            vec![]
        }
    }

    fn run_test_script(script: &str) -> Result<ScriptResult, ScriptError> {
        let rt = LuaRuntime::new(Arc::new(crate::assets::AssetLoader::new(None, None, None)));
        let src: Arc<dyn ScreenRepoSource> = Arc::new(EmptySource);
        rt.run_script(script, &src, "t", "", &Default::default(), None, None, None)
    }

    #[test]
    fn script_can_return_gamut_knobs() {
        let result = run_test_script(
            r#"
            return {
                data = {},
                gamut = { knee = 0.45, amount = 0.8, max_compression = 3.0 },
            }
            "#,
        )
        .expect("script must run");
        let g = result.gamut.expect("gamut table must be parsed");
        assert_eq!(g.knee, Some(0.45));
        assert_eq!(g.amount, Some(0.8));
        assert_eq!(g.max_compression, Some(3.0));
    }

    #[test]
    fn a_partial_gamut_table_leaves_the_rest_unset() {
        let result = run_test_script(r#"return { data = {}, gamut = { amount = 0 } }"#)
            .expect("script must run");
        let g = result.gamut.expect("gamut table must be parsed");
        assert_eq!(g.amount, Some(0.0));
        assert_eq!(g.knee, None);
    }

    #[test]
    fn no_gamut_table_means_none() {
        let result = run_test_script(r#"return { data = {} }"#).expect("script must run");
        assert!(result.gamut.is_none());
    }
}
