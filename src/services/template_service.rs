use regex::Regex;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tera::{Context, Tera};

use crate::assets::AssetLoader;

/// Directories to scan for reusable templates
const LAYOUT_DIR: &str = "layouts";
const COMPONENT_DIR: &str = "components";

/// Compiled regex for matching image href attributes in SVG.
/// Uses OnceLock to compile once and reuse across all render calls.
fn image_href_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"<image\s+([^>]*?)(?:xlink:)?href\s*=\s*"([^"]+)"([^>]*)>"#)
            .expect("Failed to compile image href regex")
    })
}

/// Error type for template rendering
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template error: {0}")]
    Tera(#[from] tera::Error),

    #[error("Template not found: {0}")]
    NotFound(String),

    #[error("Failed to read template: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image resolution error: {0}")]
    ImageResolution(String),
}

/// Service for rendering SVG templates with Tera
pub struct TemplateService {
    asset_loader: Arc<AssetLoader>,
}

impl TemplateService {
    /// Create a new template service
    pub fn new(asset_loader: Arc<AssetLoader>) -> Result<Self, TemplateError> {
        // Count templates for logging
        let template_count = asset_loader
            .list_screens()
            .iter()
            .filter(|s| s.ends_with(".svg"))
            .count();

        tracing::info!(templates = template_count, "Template service initialized");

        Ok(Self { asset_loader })
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

    /// Load all templates from a subdirectory (layouts or components).
    ///
    /// Templates are named as "layouts/name.svg" or "components/name.svg" for use
    /// in `{% extends %}` and `{% include %}` directives.
    fn load_templates_from_dir(&self, tera: &mut Tera, subdir: &str) {
        // List all screen files and filter by subdirectory
        for file in self.asset_loader.list_screens() {
            if file.starts_with(subdir) && file.ends_with(".svg") {
                // Try to load the template
                if let Ok(content) = self.asset_loader.read_screen_string(Path::new(&file)) {
                    if let Err(e) = tera.add_raw_template(&file, &content) {
                        tracing::warn!(
                            template = %file,
                            error = %e,
                            "Failed to load template"
                        );
                    } else {
                        tracing::trace!(template = %file, "Loaded reusable template");
                    }
                }
            }
        }
    }

    /// Render a template with the given data
    /// Templates are always loaded fresh to support live editing
    pub fn render(
        &self,
        template_path: &Path,
        data: &serde_json::Value,
        screen_name: &str,
    ) -> Result<String, TemplateError> {
        let template_name = template_path.to_str().unwrap_or("unknown");

        // Always load template fresh (like Lua scripts)
        let template_content = self
            .asset_loader
            .read_screen_string(template_path)
            .map_err(|e| TemplateError::NotFound(e.to_string()))?;

        let mut tera = Tera::default();

        // Load reusable templates (layouts for extends, components for include)
        self.load_templates_from_dir(&mut tera, LAYOUT_DIR);
        self.load_templates_from_dir(&mut tera, COMPONENT_DIR);

        // Add the main template
        tera.add_raw_template(template_name, &template_content)?;
        Self::register_filters(&mut tera);

        let context = Context::from_serialize(data)?;
        let svg = tera.render(template_name, &context)?;

        // Resolve relative image references to data URIs
        let svg = self.resolve_image_refs(&svg, screen_name)?;

        Ok(svg)
    }

    /// Resolve relative image href attributes to data URIs
    ///
    /// Scans for `<image ... href="..."/>` elements and replaces relative paths
    /// with base64-encoded data URIs. Paths like `logo.png` are resolved to
    /// `screens/<screen_name>/logo.png`.
    fn resolve_image_refs(&self, svg: &str, screen_name: &str) -> Result<String, TemplateError> {
        use base64::Engine;

        // Use pre-compiled regex for matching image href attributes
        let re = image_href_regex();

        let mut result = svg.to_string();

        // Collect all matches first to avoid modifying while iterating
        let matches: Vec<_> = re.captures_iter(svg).collect();

        for cap in matches {
            let full_match = cap.get(0).unwrap().as_str();
            let before_href = cap.get(1).unwrap().as_str();
            let href = cap.get(2).unwrap().as_str();
            let after_href = cap.get(3).unwrap().as_str();

            // Skip if already a data URI or absolute URL
            if href.starts_with("data:")
                || href.starts_with("http://")
                || href.starts_with("https://")
            {
                continue;
            }

            // Build the asset path: screens/<screen_name>/<href>
            let asset_path_str = format!("{screen_name}/{href}");
            let asset_path = std::path::Path::new(&asset_path_str);

            // Try to read the asset
            match self.asset_loader.read_screen(asset_path) {
                Ok(data) => {
                    // Determine MIME type from extension
                    let mime_type = match std::path::Path::new(href)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .as_deref()
                    {
                        Some("png") => "image/png",
                        Some("jpg" | "jpeg") => "image/jpeg",
                        Some("gif") => "image/gif",
                        Some("webp") => "image/webp",
                        Some("svg") => "image/svg+xml",
                        _ => "application/octet-stream",
                    };

                    // Encode to base64
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&*data);
                    let data_uri = format!("data:{mime_type};base64,{encoded}");

                    // Build replacement element
                    let replacement =
                        format!("<image {before_href}href=\"{data_uri}\"{after_href}>");
                    result = result.replace(full_match, &replacement);

                    tracing::trace!(
                        screen = screen_name,
                        asset = href,
                        "Resolved image reference to data URI"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        screen = screen_name,
                        asset = href,
                        error = %e,
                        "Failed to resolve image reference"
                    );
                    // Leave the original href unchanged - resvg might still handle it
                }
            }
        }

        Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape_basic() {
        assert_eq!(html_escape("hello"), "hello");
        assert_eq!(html_escape(""), "");
    }

    #[test]
    fn test_html_escape_special_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(html_escape("it's"), "it&#39;s");
    }

    #[test]
    fn test_html_escape_multiple() {
        assert_eq!(
            html_escape("<a href=\"test\">link</a>"),
            "&lt;a href=&quot;test&quot;&gt;link&lt;/a&gt;"
        );
    }

    #[test]
    fn test_template_error_display() {
        let err = TemplateError::NotFound("test.svg".to_string());
        assert_eq!(err.to_string(), "Template not found: test.svg");

        let err = TemplateError::ImageResolution("failed".to_string());
        assert_eq!(err.to_string(), "Image resolution error: failed");
    }

    #[test]
    fn test_render_error_svg() {
        let loader = Arc::new(crate::assets::AssetLoader::new(None, None, None));
        let service = TemplateService::new(loader).unwrap();

        let error_svg = service.render_error("Test error message");

        assert!(error_svg.contains("<svg"));
        assert!(error_svg.contains("</svg>"));
        assert!(error_svg.contains("Error"));
        assert!(error_svg.contains("Test error message"));
    }

    #[test]
    fn test_render_error_escapes_html() {
        let loader = Arc::new(crate::assets::AssetLoader::new(None, None, None));
        let service = TemplateService::new(loader).unwrap();

        let error_svg = service.render_error("<script>alert('xss')</script>");

        // Should be escaped
        assert!(error_svg.contains("&lt;script&gt;"));
        assert!(!error_svg.contains("<script>alert"));
    }
}
