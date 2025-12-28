use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::RenderError;
use crate::models::{AppConfig, DisplaySpec, ScreenConfig};
use crate::services::{LuaRuntime, RenderService, TemplateService};

/// Result from running the content pipeline
pub struct ContentResult {
    /// Rendered PNG bytes (None if skip_update is true)
    pub png_bytes: Option<Vec<u8>>,
    /// Refresh rate in seconds (from script)
    pub refresh_rate: u32,
    /// If true, no new content - just tell device to check back later
    pub skip_update: bool,
}

/// Device context passed to templates
#[derive(Debug, Clone, Default)]
pub struct DeviceContext {
    /// Battery voltage (if available)
    pub battery_voltage: Option<f32>,
    /// WiFi signal strength (if available)
    pub rssi: Option<i32>,
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
        screens_dir: &Path,
        renderer: Arc<RenderService>,
    ) -> Result<Self, ContentError> {
        let lua_runtime = LuaRuntime::new(screens_dir);
        let template_service = TemplateService::new(screens_dir)?;

        Ok(Self {
            config,
            lua_runtime,
            template_service,
            renderer,
        })
    }

    /// Generate content for a device
    pub fn generate_for_device(
        &self,
        device_mac: &str,
        spec: DisplaySpec,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ContentResult, ContentError> {
        // Look up device config
        let (screen_config, device_config) = self
            .config
            .get_screen_for_device(device_mac)
            .or_else(|| {
                // Fall back to default screen with empty params
                self.config.get_default_screen().map(|sc| {
                    // Create a temporary device config for default screen
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

        self.generate_for_screen(screen_config, &device_config.params, spec, device_ctx)
    }

    /// Generate content for a specific screen
    pub fn generate_for_screen(
        &self,
        screen: &ScreenConfig,
        params: &HashMap<String, serde_yaml::Value>,
        spec: DisplaySpec,
        device_ctx: Option<DeviceContext>,
    ) -> Result<ContentResult, ContentError> {
        // Run the Lua script with device context
        let script_result = self
            .lua_runtime
            .run_script(&screen.script, params, device_ctx.as_ref())?;

        // Use script's refresh rate, or fall back to screen's default
        let refresh_rate = if script_result.refresh_rate > 0 {
            script_result.refresh_rate
        } else {
            screen.default_refresh
        };

        // If script says skip_update, return early without rendering
        if script_result.skip_update {
            tracing::debug!(
                script = %screen.script.display(),
                refresh_rate = refresh_rate,
                "Script returned skip_update, skipping render"
            );
            return Ok(ContentResult {
                png_bytes: None,
                refresh_rate,
                skip_update: true,
            });
        }

        tracing::debug!(
            script = %screen.script.display(),
            refresh_rate = refresh_rate,
            "Script executed successfully"
        );

        // Build namespaced template context:
        // - data.*   : from Lua script
        // - device.* : device info (battery_voltage, rssi)
        // - params.* : config params
        let mut template_context = serde_json::Map::new();

        // Add Lua data under "data" namespace
        template_context.insert("data".to_string(), script_result.data.clone());

        // Add device context under "device" namespace
        let mut device_obj = serde_json::Map::new();
        if let Some(ref ctx) = device_ctx {
            if let Some(voltage) = ctx.battery_voltage {
                device_obj.insert("battery_voltage".to_string(), serde_json::json!(voltage));
            }
            if let Some(rssi) = ctx.rssi {
                device_obj.insert("rssi".to_string(), serde_json::json!(rssi));
            }
        }
        template_context.insert("device".to_string(), serde_json::Value::Object(device_obj));

        // Add params under "params" namespace
        let params_json = serde_json::to_value(params).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        template_context.insert("params".to_string(), params_json);

        let template_data = serde_json::Value::Object(template_context);

        // Render the template
        let svg_content = self
            .template_service
            .render(&screen.template, &template_data)?;

        tracing::debug!(
            template = %screen.template.display(),
            svg_len = svg_content.len(),
            "Template rendered successfully"
        );

        // Render SVG to PNG
        let png_bytes = self
            .renderer
            .svg_renderer
            .render_to_png(svg_content.as_bytes(), spec)?;

        Ok(ContentResult {
            png_bytes: Some(png_bytes),
            refresh_rate,
            skip_update: false,
        })
    }

    /// Generate error content
    pub fn generate_error(&self, error: &str, spec: DisplaySpec) -> Result<Vec<u8>, ContentError> {
        let svg_content = self.template_service.render_error(error);
        let png_bytes = self
            .renderer
            .svg_renderer
            .render_to_png(svg_content.as_bytes(), spec)?;
        Ok(png_bytes)
    }

    /// Get the config
    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}
