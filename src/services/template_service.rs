use std::path::Path;
use tera::{Context, Tera};

/// Error type for template rendering
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template error: {0}")]
    Tera(#[from] tera::Error),

    #[error("Template not found: {0}")]
    NotFound(String),

    #[error("Failed to read template: {0}")]
    Io(#[from] std::io::Error),
}

/// Service for rendering SVG templates with Tera
pub struct TemplateService {
    screens_dir: std::path::PathBuf,
}

impl TemplateService {
    /// Create a new template service
    pub fn new(screens_dir: &Path) -> Result<Self, TemplateError> {
        // Count templates for logging
        let glob_pattern = screens_dir.join("**/*.svg");
        let glob_str = glob_pattern.to_str().unwrap_or("screens/**/*.svg");

        let template_count = match Tera::new(glob_str) {
            Ok(t) => t.get_template_names().count(),
            Err(_) => 0,
        };

        tracing::info!(templates = template_count, "Template service initialized");

        Ok(Self {
            screens_dir: screens_dir.to_path_buf(),
        })
    }

    /// Register custom Tera filters
    fn register_filters(tera: &mut Tera) {
        // truncate filter with custom length
        tera.register_filter(
            "truncate",
            |value: &tera::Value, args: &std::collections::HashMap<String, tera::Value>| {
                let s = tera::try_get_value!("truncate", "value", String, value);
                let len = args.get("length").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                if s.len() <= len {
                    Ok(tera::Value::String(s))
                } else {
                    let truncated = s.chars().take(len - 3).collect::<String>() + "...";
                    Ok(tera::Value::String(truncated))
                }
            },
        );

        // format_time filter
        tera.register_filter(
            "format_time",
            |value: &tera::Value, args: &std::collections::HashMap<String, tera::Value>| {
                let ts = tera::try_get_value!("format_time", "value", i64, value);
                let fmt = args
                    .get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("%H:%M");

                use chrono::{TimeZone, Utc};
                if let Some(dt) = Utc.timestamp_opt(ts, 0).single() {
                    Ok(tera::Value::String(dt.format(fmt).to_string()))
                } else {
                    Ok(tera::Value::String("--:--".to_string()))
                }
            },
        );
    }

    /// Render a template with the given data
    /// Templates are always loaded fresh from disk to support live editing
    pub fn render(
        &self,
        template_path: &Path,
        data: &serde_json::Value,
    ) -> Result<String, TemplateError> {
        let template_name = template_path.to_str().unwrap_or("unknown");

        // Always load template fresh from disk (like Lua scripts)
        let full_path = self.screens_dir.join(template_path);
        let template_content = std::fs::read_to_string(&full_path)
            .map_err(|_| TemplateError::NotFound(full_path.display().to_string()))?;

        let mut tera = Tera::default();
        tera.add_raw_template(template_name, &template_content)?;
        Self::register_filters(&mut tera);

        let context = Context::from_serialize(data)?;
        let svg = tera.render(template_name, &context)?;

        Ok(svg)
    }

    /// Render an error screen
    pub fn render_error(&self, error: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 480" width="800" height="480">
  <rect width="800" height="480" fill="white"/>
  <rect x="0" y="0" width="800" height="70" fill="black"/>
  <text x="400" y="45" text-anchor="middle" fill="white" font-family="sans-serif" font-size="28" font-weight="bold">
    Error
  </text>
  <rect x="40" y="100" width="720" height="300" fill="rgb(255,240,240)" stroke="rgb(200,100,100)" stroke-width="2" rx="10"/>
  <text x="400" y="200" text-anchor="middle" fill="black" font-family="monospace" font-size="14">
    {}
  </text>
  <text x="400" y="240" text-anchor="middle" fill="rgb(100,100,100)" font-family="sans-serif" font-size="12">
    Check server logs for details
  </text>
  <text x="400" y="450" text-anchor="middle" fill="rgb(150,150,150)" font-family="sans-serif" font-size="12">
    Will retry in 60 seconds
  </text>
</svg>"#,
            html_escape(error)
        )
    }
}

/// Simple HTML escape for error messages
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
