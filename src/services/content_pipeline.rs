use std::collections::HashMap;
use std::sync::Arc;

use crate::assets::AssetLoader;
use crate::error::RenderError;
use crate::models::{AppConfig, DisplaySpec, ScreenConfig};
use crate::services::{LuaRuntime, RenderService, TemplateService};

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
        let lua_runtime = LuaRuntime::new(asset_loader.clone());
        let template_service = TemplateService::new(asset_loader)?;

        Ok(Self {
            config,
            lua_runtime,
            template_service,
            renderer,
        })
    }

    /// Run script for a device (without rendering)
    pub fn run_script_for_device(
        &self,
        device_mac: &str,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ScriptResult, ContentError> {
        // Look up device config
        let (screen_config, device_config) = self
            .config
            .get_screen_for_device(device_mac)
            .or_else(|| {
                // Fall back to default screen with empty params
                self.config.get_default_screen().map(|sc| {
                    static EMPTY_DEVICE: std::sync::OnceLock<crate::models::DeviceConfig> =
                        std::sync::OnceLock::new();
                    let dc = EMPTY_DEVICE.get_or_init(|| crate::models::DeviceConfig {
                        screen: "default".to_string(),
                        params: HashMap::new(),
                    });
                    (sc, dc)
                })
            })
            .ok_or_else(|| ContentError::ScreenNotFound(device_mac.to_string()))?;

        self.run_script_for_screen(screen_config, &device_config.params, device_ctx)
    }

    /// Run script for a specific screen (without rendering)
    fn run_script_for_screen(
        &self,
        screen: &ScreenConfig,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ScriptResult, ContentError> {
        // Run the Lua script
        let lua_result =
            self.lua_runtime
                .run_script(&screen.script, params, device_ctx.as_ref())?;

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
        }
        template_context.insert("device".to_string(), serde_json::Value::Object(device_obj));

        // Add params under "params" namespace
        let params_json = serde_json::to_value(&result.params)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        template_context.insert("params".to_string(), params_json);

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

    /// Render PNG from cached SVG content
    pub fn render_png_from_svg(
        &self,
        svg: &str,
        spec: DisplaySpec,
    ) -> Result<Vec<u8>, ContentError> {
        let png_bytes = self
            .renderer
            .svg_renderer
            .render_to_png(svg.as_bytes(), spec)?;
        Ok(png_bytes)
    }

    /// Render error SVG (without PNG conversion)
    pub fn render_error_svg(&self, error: &str) -> String {
        self.template_service.render_error(error)
    }

    /// Run script directly with explicit paths (for dev mode)
    pub fn run_script_direct(
        &self,
        script_path: &std::path::Path,
        template_path: &std::path::Path,
        default_refresh: u32,
        params: HashMap<String, serde_json::Value>,
        device_ctx: Option<DeviceContext>,
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

        // Run the Lua script
        let lua_result = self
            .lua_runtime
            .run_script(script_path, &yaml_params, device_ctx.as_ref())
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
        })
    }
}
