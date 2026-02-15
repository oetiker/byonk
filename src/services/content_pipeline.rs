use std::collections::HashMap;
use std::sync::Arc;

use crate::assets::AssetLoader;
use crate::error::RenderError;
use crate::models::{AppConfig, DisplaySpec, ScreenConfig};
use crate::services::{FontFaceInfo, LuaRuntime, RenderService, TemplateService};

/// Result from running a Lua script (before rendering)
pub struct ScriptResult {
    /// Data returned by the script
    pub data: serde_json::Value,
    /// Refresh rate in seconds
    pub refresh_rate: u32,
    /// If true, no new content - just tell device to check back later
    pub skip_update: bool,
    /// Screen name
    pub screen_name: String,
    /// Template path for rendering
    pub template_path: std::path::PathBuf,
    /// Config params
    pub params: HashMap<String, serde_yaml::Value>,
    /// Optional color palette override from Lua script (hex RGB strings)
    pub script_colors: Option<Vec<String>>,
    /// Optional dither mode from Lua script ("photo" or "graphics")
    pub script_dither: Option<String>,
    /// Optional preserve_exact override from Lua script
    pub script_preserve_exact: Option<bool>,
    /// Optional error clamp override from Lua script
    pub script_error_clamp: Option<f32>,
    /// Optional blue noise jitter scale override from Lua script
    pub script_noise_scale: Option<f32>,
    /// Optional chroma clamp override from Lua script
    pub script_chroma_clamp: Option<f32>,
    /// Optional dither strength override from Lua script
    pub script_strength: Option<f32>,
}

/// Device context passed to templates and Lua scripts
#[derive(Debug, Clone, Default)]
pub struct DeviceContext {
    /// Device MAC address
    pub mac: String,
    /// Battery voltage (if available)
    pub battery_voltage: Option<f32>,
    /// WiFi signal strength (if available)
    pub rssi: Option<i32>,
    /// Device model ("og" or "x")
    pub model: Option<String>,
    /// Firmware version
    pub firmware_version: Option<String>,
    /// Display width in pixels
    pub width: Option<u32>,
    /// Display height in pixels
    pub height: Option<u32>,
    /// Registration code (if device has a Byonk key)
    pub registration_code: Option<String>,
    /// Board identifier (e.g. "trmnl_og_4clr")
    pub board: Option<String>,
    /// Available display colors as hex RGB strings (e.g. ["#000000", "#FFFFFF", "#FF0000"])
    pub colors: Option<Vec<String>>,
    /// Pre-script resolved dither algorithm name
    pub dither_algorithm: Option<String>,
    /// Pre-script resolved error clamp
    pub dither_error_clamp: Option<f32>,
    /// Pre-script resolved noise scale
    pub dither_noise_scale: Option<f32>,
    /// Pre-script resolved chroma clamp
    pub dither_chroma_clamp: Option<f32>,
    /// Pre-script resolved dither strength
    pub dither_strength: Option<f32>,
}

/// Error from the content pipeline
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("Script error: {0}")]
    Script(#[from] super::ScriptError),

    #[error("Template error: {0}")]
    Template(#[from] super::TemplateError),

    #[error("Render error: {0}")]
    Render(#[from] RenderError),

    #[error("Screen not found: {0}")]
    ScreenNotFound(String),
}

/// Content pipeline that orchestrates script → template → render
pub struct ContentPipeline {
    config: Arc<AppConfig>,
    asset_loader: Arc<AssetLoader>,
    lua_runtime: LuaRuntime,
    template_service: TemplateService,
    renderer: Arc<RenderService>,
}

impl ContentPipeline {
    pub fn new(
        config: Arc<AppConfig>,
        asset_loader: Arc<AssetLoader>,
        renderer: Arc<RenderService>,
    ) -> Result<Self, ContentError> {
        // Build font info from the renderer's fontdb
        let mut font_families: HashMap<String, Vec<FontFaceInfo>> = HashMap::new();
        for face in renderer.svg_renderer.font_faces() {
            if let Some((family_name, _)) = face.families.first() {
                let info = FontFaceInfo {
                    style: format!("{:?}", face.style),
                    weight: face.weight.0,
                    stretch: format!("{:?}", face.stretch),
                    monospaced: face.monospaced,
                    post_script_name: face.post_script_name.clone(),
                    bitmap_strikes: face.bitmap_strikes.clone(),
                };
                font_families
                    .entry(family_name.clone())
                    .or_default()
                    .push(info);
            }
        }

        let lua_runtime = LuaRuntime::with_fonts(asset_loader.clone(), font_families);
        let template_service = TemplateService::new(asset_loader.clone())?;

        Ok(Self {
            config,
            asset_loader,
            lua_runtime,
            template_service,
            renderer,
        })
    }

    /// Resolve a screen by name from config or filesystem auto-discovery
    fn resolve_screen(&self, screen_name: &str) -> Option<ScreenConfig> {
        self.config.screens.get(screen_name).cloned().or_else(|| {
            let all_files = self.asset_loader.list_screens();
            let lua_file = format!("{}.lua", screen_name);
            let svg_file = format!("{}.svg", screen_name);
            if all_files.iter().any(|f| f == &lua_file) && all_files.iter().any(|f| f == &svg_file)
            {
                Some(ScreenConfig {
                    script: std::path::PathBuf::from(lua_file),
                    template: std::path::PathBuf::from(svg_file),
                    default_refresh: 900,
                })
            } else {
                None
            }
        })
    }

    /// Run script for a device (without rendering)
    pub fn run_script_for_device(
        &self,
        device_mac: &str,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ScriptResult, ContentError> {
        // Look up device config - try registration code first, then MAC address
        let device_config = device_ctx
            .as_ref()
            .and_then(|ctx| ctx.registration_code.as_deref())
            .and_then(|code| self.config.get_device_config_for_code(code))
            .or_else(|| self.config.get_device_config(device_mac));

        if let Some(device_config) = device_config {
            // Found device config — resolve screen from config or filesystem
            if let Some(screen_config) = self.resolve_screen(&device_config.screen) {
                return self.run_script_for_screen(
                    &screen_config,
                    &device_config.params,
                    device_ctx,
                );
            }
            // Device config exists but screen not found — fall through to default
        }

        // Fall back to default screen with empty params
        let screen_config = self
            .resolve_screen(self.config.default_screen.as_deref().unwrap_or("default"))
            .ok_or_else(|| ContentError::ScreenNotFound(device_mac.to_string()))?;

        static EMPTY_DEVICE: std::sync::OnceLock<crate::models::DeviceConfig> =
            std::sync::OnceLock::new();
        let dc = EMPTY_DEVICE.get_or_init(|| crate::models::DeviceConfig {
            screen: "default".to_string(),
            params: HashMap::new(),
            colors: None,
            dither: None,
            panel: None,
            error_clamp: None,
            noise_scale: None,
            chroma_clamp: None,
            strength: None,
        });

        self.run_script_for_screen(&screen_config, &dc.params, device_ctx)
    }

    /// Run script for a screen by name with custom params (without rendering)
    ///
    /// This is used for running custom registration screens where params.code
    /// contains the registration code.
    pub fn run_screen_by_name(
        &self,
        screen_name: &str,
        params: HashMap<String, serde_yaml::Value>,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ScriptResult, ContentError> {
        let screen = self
            .resolve_screen(screen_name)
            .ok_or_else(|| ContentError::ScreenNotFound(screen_name.to_string()))?;

        self.run_script_for_screen(&screen, &params, device_ctx)
    }

    /// Run script for a specific screen (without rendering)
    fn run_script_for_screen(
        &self,
        screen: &ScreenConfig,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ScriptResult, ContentError> {
        // Run the Lua script (no timestamp override for normal operation)
        let lua_result =
            self.lua_runtime
                .run_script(&screen.script, params, device_ctx.as_ref(), None)?;

        // Use script's refresh rate, or fall back to screen's default
        let refresh_rate = if lua_result.refresh_rate > 0 {
            lua_result.refresh_rate
        } else {
            screen.default_refresh
        };

        if lua_result.skip_update {
            tracing::debug!(
                script = %screen.script.display(),
                refresh_rate = refresh_rate,
                "Script returned skip_update"
            );
        } else {
            tracing::debug!(
                script = %screen.script.display(),
                refresh_rate = refresh_rate,
                "Script executed successfully"
            );
        }

        Ok(ScriptResult {
            data: lua_result.data,
            refresh_rate,
            skip_update: lua_result.skip_update,
            screen_name: screen
                .script
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            template_path: screen.template.clone(),
            params: params.clone(),
            script_colors: lua_result.colors,
            script_dither: lua_result.dither,
            script_preserve_exact: lua_result.preserve_exact,
            script_error_clamp: lua_result.error_clamp,
            script_noise_scale: lua_result.noise_scale,
            script_chroma_clamp: lua_result.chroma_clamp,
            script_strength: lua_result.strength,
        })
    }

    /// Render SVG from script result (without PNG conversion)
    /// This is called during /api/display to pre-render the template
    pub fn render_svg_from_script(
        &self,
        result: &ScriptResult,
        device_ctx: Option<&DeviceContext>,
    ) -> Result<String, ContentError> {
        // Build namespaced template context:
        // - data.*   : from Lua script
        // - device.* : device info (battery_voltage, rssi)
        // - params.* : config params
        let mut template_context = serde_json::Map::new();

        // Add Lua data under "data" namespace
        template_context.insert("data".to_string(), result.data.clone());

        // Add device context under "device" namespace
        let mut device_obj = serde_json::Map::new();
        if let Some(ctx) = device_ctx {
            device_obj.insert("mac".to_string(), serde_json::json!(ctx.mac));
            if let Some(voltage) = ctx.battery_voltage {
                device_obj.insert("battery_voltage".to_string(), serde_json::json!(voltage));
            }
            if let Some(rssi) = ctx.rssi {
                device_obj.insert("rssi".to_string(), serde_json::json!(rssi));
            }
            if let Some(ref model) = ctx.model {
                device_obj.insert("model".to_string(), serde_json::json!(model));
            }
            if let Some(ref fw) = ctx.firmware_version {
                device_obj.insert("firmware_version".to_string(), serde_json::json!(fw));
            }
            if let Some(width) = ctx.width {
                device_obj.insert("width".to_string(), serde_json::json!(width));
            }
            if let Some(height) = ctx.height {
                device_obj.insert("height".to_string(), serde_json::json!(height));
            }
            if let Some(ref code) = ctx.registration_code {
                device_obj.insert("registration_code".to_string(), serde_json::json!(code));
                // Also provide hyphenated version for convenience
                if code.len() == 10 {
                    let hyphenated = format!("{}-{}", &code[..5], &code[5..]);
                    device_obj.insert(
                        "registration_code_hyphenated".to_string(),
                        serde_json::json!(hyphenated),
                    );
                }
            }
            if let Some(ref board) = ctx.board {
                device_obj.insert("board".to_string(), serde_json::json!(board));
            }
            if let Some(ref colors) = ctx.colors {
                device_obj.insert("colors".to_string(), serde_json::json!(colors));
            }
        }
        template_context.insert("device".to_string(), serde_json::Value::Object(device_obj));

        // Add params under "params" namespace
        let params_json = serde_json::to_value(&result.params)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        template_context.insert("params".to_string(), params_json);

        // Add layout under "layout" namespace (mirrors lua_runtime layout_table)
        template_context.insert("layout".to_string(), Self::build_layout_context(device_ctx));

        let template_data = serde_json::Value::Object(template_context);

        // Render the template to SVG (with image reference resolution)
        let svg_content = self.template_service.render(
            &result.template_path,
            &template_data,
            &result.screen_name,
        )?;

        tracing::debug!(
            template = %result.template_path.display(),
            svg_len = svg_content.len(),
            "Template rendered to SVG"
        );

        Ok(svg_content)
    }

    /// Build layout context for SVG templates (mirrors lua_runtime layout_table).
    fn build_layout_context(device_ctx: Option<&DeviceContext>) -> serde_json::Value {
        let width = device_ctx.and_then(|ctx| ctx.width).unwrap_or(800) as f64;
        let height = device_ctx.and_then(|ctx| ctx.height).unwrap_or(480) as f64;
        let scale = f64::min(width / 800.0, height / 480.0);
        let mut obj = serde_json::Map::new();
        obj.insert("width".to_string(), serde_json::json!(width as i64));
        obj.insert("height".to_string(), serde_json::json!(height as i64));
        obj.insert("scale".to_string(), serde_json::json!(scale));
        obj.insert(
            "center_x".to_string(),
            serde_json::json!((width / 2.0).floor() as i64),
        );
        obj.insert(
            "center_y".to_string(),
            serde_json::json!((height / 2.0).floor() as i64),
        );
        obj.insert(
            "margin".to_string(),
            serde_json::json!((20.0 * scale).floor() as i64),
        );
        obj.insert(
            "margin_sm".to_string(),
            serde_json::json!((10.0 * scale).floor() as i64),
        );
        obj.insert(
            "margin_lg".to_string(),
            serde_json::json!((40.0 * scale).floor() as i64),
        );
        if let Some(ctx) = device_ctx {
            if let Some(ref colors) = ctx.colors {
                obj.insert("colors".to_string(), serde_json::json!(colors));
                obj.insert("color_count".to_string(), serde_json::json!(colors.len()));
                let grey_count = colors
                    .iter()
                    .filter(|c| {
                        let hex = c.trim_start_matches('#');
                        hex.len() == 6 && hex[0..2] == hex[2..4] && hex[2..4] == hex[4..6]
                    })
                    .count();
                obj.insert("grey_count".to_string(), serde_json::json!(grey_count));
            } else {
                obj.insert("color_count".to_string(), serde_json::json!(4));
                obj.insert("grey_count".to_string(), serde_json::json!(4));
            }
        } else {
            obj.insert("color_count".to_string(), serde_json::json!(4));
            obj.insert("grey_count".to_string(), serde_json::json!(4));
        }
        serde_json::Value::Object(obj)
    }

    /// Render PNG from cached SVG content using the given color palette.
    ///
    /// The palette determines both dithering targets and PNG output format
    /// (native grayscale for grey palettes, indexed PNG for color palettes).
    /// When `actual` measured colors are provided, the ditherer models what
    /// the panel really displays. When `use_actual` is true, the PNG output
    /// uses measured colors (for dev mode preview).
    #[allow(clippy::too_many_arguments)]
    pub fn render_png_from_svg(
        &self,
        svg: &str,
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
        actual: Option<&[(u8, u8, u8)]>,
        use_actual: bool,
        dither: Option<&str>,
        preserve_exact: bool,
        tuning: Option<&crate::rendering::svg_to_png::DitherTuning>,
    ) -> Result<Vec<u8>, ContentError> {
        let png_bytes = self.renderer.svg_renderer.render_to_palette_png(
            svg.as_bytes(),
            spec,
            palette,
            actual,
            use_actual,
            dither,
            preserve_exact,
            tuning,
        )?;
        Ok(png_bytes)
    }

    /// Render error SVG (without PNG conversion)
    pub fn render_error_svg(&self, error: &str) -> String {
        self.template_service.render_error(error)
    }

    /// Render registration screen showing the device's 10-character registration code
    ///
    /// The code is displayed in 2x5 format (two rows of 5 characters) for easy reading
    /// from an e-ink display. Instructions show how to add the code to config.yaml.
    pub fn render_registration_screen(&self, code: &str, width: u32, height: u32) -> String {
        // Calculate responsive sizing based on display dimensions
        let scale = (width as f32 / 800.0).min(height as f32 / 480.0);
        let code_font_size = (72.0 * scale).round() as u32;
        let title_font_size = (32.0 * scale).round() as u32;
        let subtitle_font_size = (18.0 * scale).round() as u32;
        let center_x = width / 2;
        let center_y = height / 2;

        // Split 10-char code into two rows of 5, spaced for readability
        let chars: Vec<char> = code.chars().collect();
        let row1: String = chars
            .iter()
            .take(5)
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let row2: String = chars
            .iter()
            .skip(5)
            .take(5)
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let code_line_height = (code_font_size as f32 * 1.2).round() as u32;

        // Format code with hyphen for config instructions (ABCDE-FGHJK)
        let hyphenated_code = if code.len() == 10 {
            format!("{}-{}", &code[..5], &code[5..])
        } else {
            code.to_string()
        };

        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">
  <defs>
    <style>
      text {{ text-anchor: middle; font-family: Outfit, sans-serif; }}
      .title {{ font-weight: 700; }}
      .code {{ font-weight: 900; letter-spacing: 0.3em; }}
      .subtitle {{ font-weight: 400; }}
    </style>
  </defs>

  <!-- Background -->
  <rect width="{width}" height="{height}" fill="#ffffff"/>

  <!-- Border -->
  <rect x="10" y="10" width="{border_width}" height="{border_height}" fill="none" stroke="#000000" stroke-width="4" rx="8"/>

  <!-- Title -->
  <text x="{center_x}" y="{title_y}" font-size="{title_font_size}" class="title" fill="#000000">DEVICE REGISTRATION</text>

  <!-- Subtitle -->
  <text x="{center_x}" y="{subtitle_y}" font-size="{subtitle_font_size}" class="subtitle" fill="#666666">Add this code to config.yaml devices section:</text>

  <!-- Registration Code (2 rows of 5 chars) -->
  <text x="{center_x}" y="{code_row1_y}" font-size="{code_font_size}" class="code" fill="#000000">{row1}</text>
  <text x="{center_x}" y="{code_row2_y}" font-size="{code_font_size}" class="code" fill="#000000">{row2}</text>

  <!-- Instructions -->
  <text x="{center_x}" y="{inst1_y}" font-size="{subtitle_font_size}" class="subtitle" fill="#666666">devices:</text>
  <text x="{center_x}" y="{inst2_y}" font-size="{subtitle_font_size}" class="subtitle" fill="#666666">  "{hyphenated_code}":</text>
  <text x="{center_x}" y="{inst3_y}" font-size="{subtitle_font_size}" class="subtitle" fill="#666666">    screen: your_screen_name</text>
</svg>"##,
            width = width,
            height = height,
            border_width = width - 20,
            border_height = height - 20,
            center_x = center_x,
            title_y = (center_y as f32 * 0.30).round() as u32,
            subtitle_y = (center_y as f32 * 0.45).round() as u32,
            code_row1_y = (center_y as f32 * 0.65).round() as u32,
            code_row2_y = (center_y as f32 * 0.65).round() as u32 + code_line_height,
            inst1_y = (center_y as f32 * 1.20).round() as u32,
            inst2_y = (center_y as f32 * 1.35).round() as u32,
            inst3_y = (center_y as f32 * 1.50).round() as u32,
            title_font_size = title_font_size,
            subtitle_font_size = subtitle_font_size,
            code_font_size = code_font_size,
            row1 = row1,
            row2 = row2,
            hyphenated_code = hyphenated_code,
        )
    }

    /// Run script directly with explicit paths (for dev mode)
    pub fn run_script_direct(
        &self,
        script_path: &std::path::Path,
        template_path: &std::path::Path,
        default_refresh: u32,
        params: HashMap<String, serde_json::Value>,
        device_ctx: Option<DeviceContext>,
        timestamp_override: Option<i64>,
    ) -> Result<ScriptResult, String> {
        // Convert JSON params to YAML params for consistency
        let yaml_params: HashMap<String, serde_yaml::Value> = params
            .into_iter()
            .filter_map(|(k, v)| {
                serde_json::to_string(&v)
                    .ok()
                    .and_then(|s| serde_yaml::from_str(&s).ok())
                    .map(|yv| (k, yv))
            })
            .collect();

        // Run the Lua script with optional timestamp override
        let lua_result = self
            .lua_runtime
            .run_script(
                script_path,
                &yaml_params,
                device_ctx.as_ref(),
                timestamp_override,
            )
            .map_err(|e| e.to_string())?;

        // Use script's refresh rate, or fall back to default
        let refresh_rate = if lua_result.refresh_rate > 0 {
            lua_result.refresh_rate
        } else {
            default_refresh
        };

        Ok(ScriptResult {
            data: lua_result.data,
            refresh_rate,
            skip_update: lua_result.skip_update,
            screen_name: script_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            template_path: template_path.to_path_buf(),
            params: yaml_params,
            script_colors: lua_result.colors,
            script_dither: lua_result.dither,
            script_preserve_exact: lua_result.preserve_exact,
            script_error_clamp: lua_result.error_clamp,
            script_noise_scale: lua_result.noise_scale,
            script_chroma_clamp: lua_result.chroma_clamp,
            script_strength: lua_result.strength,
        })
    }
}
