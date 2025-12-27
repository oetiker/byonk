use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::RenderError;
use crate::models::{AppConfig, DisplaySpec, ScreenConfig};
use crate::services::{LuaRuntime, RenderService, TemplateService};

/// Result from running the content pipeline
pub struct ContentResult {
    /// Rendered PNG bytes
    pub png_bytes: Vec<u8>,
    /// Refresh rate in seconds (from script)
    pub refresh_rate: u32,
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

        self.generate_for_screen(screen_config, &device_config.params, spec)
    }

    /// Generate content for a specific screen
    pub fn generate_for_screen(
        &self,
        screen: &ScreenConfig,
        params: &HashMap<String, serde_yaml::Value>,
        spec: DisplaySpec,
    ) -> Result<ContentResult, ContentError> {
        // Run the Lua script
        let script_result = self.lua_runtime.run_script(&screen.script, params)?;

        tracing::debug!(
            script = %screen.script.display(),
            refresh_rate = script_result.refresh_rate,
            "Script executed successfully"
        );

        // Render the template
        let svg_content = self
            .template_service
            .render(&screen.template, &script_result.data)?;

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

        // Use script's refresh rate, or fall back to screen's default
        let refresh_rate = if script_result.refresh_rate > 0 {
            script_result.refresh_rate
        } else {
            screen.default_refresh
        };

        Ok(ContentResult {
            png_bytes,
            refresh_rate,
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
