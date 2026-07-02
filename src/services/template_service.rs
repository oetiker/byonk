use regex::Regex;
use std::sync::{Arc, OnceLock};
use tera::{Context, Tera};

use crate::assets::AssetLoader;
use crate::services::package_loader::{join_rel, PackageSource};

/// Compiled regex for matching image href attributes in SVG.
/// Uses OnceLock to compile once and reuse across all render calls.
fn image_href_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"<image\s+([^>]*?)(?:xlink:)?href\s*=\s*"([^"]+)"([^>]*)>"#)
            .expect("Failed to compile image href regex")
    })
}

/// Format a tera::Error including its full cause chain
fn format_tera_error(e: &tera::Error) -> String {
    // The Debug representation of tera::Error includes line/column info
    // that Display omits, so use Debug format
    format!("{:?}", e)
}

/// Error type for template rendering
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("{}", format_tera_error(.0))]
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

    /// Render a screen template with the given data, scoped to one package.
    ///
    /// Templates are always loaded fresh (to support live editing). Every base
    /// asset is registered under `byonk-base-<v>/…` and every package `.svg` under
    /// its package-relative name, so `{% include %}`/`{% extends %}` can only reach
    /// the screen's own package plus the embedded `byonk-base` library.
    ///
    /// * `template_src` — the resolved `screen.svg` contents.
    /// * `source` — the screen's package source (for sibling includes/parts).
    /// * `screen_path` — the screen's package-relative directory (for image refs
    ///   and the main template name).
    pub fn render(
        &self,
        template_src: &str,
        source: &Arc<dyn PackageSource>,
        screen_path: &str,
        data: &serde_json::Value,
    ) -> Result<String, TemplateError> {
        let mut tera = Tera::default();

        // Register every embedded base asset under `byonk-base-<version-path>`,
        // e.g. "v1/hinting.svg" -> "byonk-base-v1/hinting.svg".
        for p in self.asset_loader.list_base() {
            if let Some(content) = self.asset_loader.read_base_string(&p) {
                let name = format!("byonk-base-{p}");
                if let Err(e) = tera.add_raw_template(&name, &content) {
                    tracing::warn!(template = %name, error = %e, "Failed to load base template");
                }
            }
        }

        // Register every package `.svg` under its package-relative name.
        for p in source.svg_files() {
            if let Some(content) = source.read_string(&p) {
                if let Err(e) = tera.add_raw_template(&p, &content) {
                    tracing::trace!(template = %p, error = %e, "Skipped package template");
                }
            }
        }

        Self::register_filters(&mut tera);

        // Register the main screen template (authoritative source, may differ from
        // the on-disk copy during live editing) and render it.
        let main_name = join_rel(screen_path, "screen.svg");
        tera.add_raw_template(&main_name, template_src)?;

        let context = Context::from_serialize(data)?;
        let svg = tera.render(&main_name, &context)?;

        // Resolve relative image references to data URIs (package-relative to the
        // screen directory).
        let svg = self.resolve_image_refs(&svg, source, screen_path)?;

        Ok(svg)
    }

    /// Resolve relative image href attributes to data URIs
    ///
    /// Scans for `<image ... href="..."/>` elements and replaces relative paths
    /// with base64-encoded data URIs. Paths like `logo.png` are resolved to the
    /// package file at `<screen_dir>/logo.png`.
    fn resolve_image_refs(
        &self,
        svg: &str,
        source: &Arc<dyn PackageSource>,
        screen_dir: &str,
    ) -> Result<String, TemplateError> {
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

            // Build the package-relative asset path: <screen_dir>/<href>
            let asset_rel = join_rel(screen_dir, href);

            // Try to read the asset from the package
            match source.read(&asset_rel) {
                Some(data) => {
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
                        screen = screen_dir,
                        asset = href,
                        "Resolved image reference to data URI"
                    );
                }
                None => {
                    tracing::warn!(
                        screen = screen_dir,
                        asset = href,
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

    #[test]
    fn test_builtin_layouts_loaded() {
        // Test that embedded layouts are accessible
        let loader = Arc::new(crate::assets::AssetLoader::new(None, None, None));
        let screens = loader.list_screens();

        assert!(screens.iter().any(|s| s == "layouts/base.svg"));
    }

    #[test]
    fn test_builtin_components_loaded() {
        // Test that embedded components are accessible
        let loader = Arc::new(crate::assets::AssetLoader::new(None, None, None));
        let screens = loader.list_screens();

        assert!(screens.iter().any(|s| s == "components/header.svg"));
        assert!(screens.iter().any(|s| s == "components/footer.svg"));
        assert!(screens.iter().any(|s| s == "components/status_bar.svg"));
    }

    /// In-memory package source for render tests.
    struct TestSource {
        files: std::collections::HashMap<String, Vec<u8>>,
    }
    impl TestSource {
        fn new(files: &[(&str, &str)]) -> Self {
            TestSource {
                files: files
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                    .collect(),
            }
        }
    }
    impl PackageSource for TestSource {
        fn read(&self, rel: &str) -> Option<Vec<u8>> {
            self.files.get(rel).cloned()
        }
        fn screen_paths(&self) -> Vec<String> {
            vec![]
        }
        fn svg_files(&self) -> Vec<String> {
            let mut v: Vec<String> = self
                .files
                .keys()
                .filter(|k| k.ends_with(".svg"))
                .cloned()
                .collect();
            v.sort();
            v
        }
        fn manifest(&self) -> &crate::models::package_manifest::PackageManifest {
            unreachable!("manifest() not used by render()")
        }
    }

    fn svc() -> TemplateService {
        TemplateService::new(Arc::new(crate::assets::AssetLoader::new(None, None, None))).unwrap()
    }

    #[test]
    fn test_render_uses_byonk_base_include() {
        // screen.svg includes a base asset AND a package-relative part, and
        // interpolates a data value.
        let src: Arc<dyn PackageSource> = Arc::new(TestSource::new(&[(
            "weather/parts/x.svg",
            "<rect id=\"part\"/>",
        )]));
        let template = r#"<svg>{% include "byonk-base-v1/hinting.svg" %}{% include "weather/parts/x.svg" %}<t>{{ data.n }}</t></svg>"#;
        let data = serde_json::json!({ "data": { "n": 42 }, "layout": { "grey_count": 4 } });
        let out = svc().render(template, &src, "weather", &data).unwrap();

        // The package part and the interpolated value both appear.
        assert!(
            out.contains("<rect id=\"part\"/>"),
            "missing package part: {out}"
        );
        assert!(out.contains("<t>42</t>"), "missing interpolation: {out}");
        // The base include resolved to some content from v1/hinting.svg (non-empty
        // expansion — the literal include tag must be gone).
        assert!(!out.contains("{% include"), "include not expanded: {out}");
    }

    #[test]
    fn test_render_package_extends() {
        let src: Arc<dyn PackageSource> = Arc::new(TestSource::new(&[(
            "weather/base.svg",
            "<svg><g>{% block content %}{% endblock %}</g></svg>",
        )]));
        let template = r#"{% extends "weather/base.svg" %}{% block content %}<text>{{ data.msg }}</text>{% endblock %}"#;
        let data = serde_json::json!({ "data": { "msg": "hello" } });
        let out = svc().render(template, &src, "weather", &data).unwrap();
        assert!(out.contains("<text>hello</text>"), "{out}");
    }

    #[test]
    fn test_render_missing_include_fails() {
        let src: Arc<dyn PackageSource> = Arc::new(TestSource::new(&[]));
        let template = r#"<svg>{% include "weather/nope.svg" %}</svg>"#;
        let data = serde_json::json!({});
        assert!(svc().render(template, &src, "weather", &data).is_err());
    }

    #[test]
    fn test_render_resolves_image_ref_from_package() {
        // A 1x1 PNG in the package, referenced relatively from the screen dir.
        let png = b"\x89PNG\r\n\x1a\n";
        let mut files = std::collections::HashMap::new();
        files.insert("weather/logo.png".to_string(), png.to_vec());
        let src: Arc<dyn PackageSource> = Arc::new(TestSource { files });
        let template = r#"<svg><image href="logo.png"/></svg>"#;
        let data = serde_json::json!({});
        let out = svc().render(template, &src, "weather", &data).unwrap();
        assert!(out.contains("data:image/png;base64,"), "{out}");
    }
}
