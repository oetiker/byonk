use crate::error::RenderError;
use crate::models::DisplaySpec;
use crate::rendering::SvgRenderer;
use crate::services::ContentProvider;
use std::path::Path;
use std::sync::Arc;

/// High-level render service that combines content selection and rendering
pub struct RenderService {
    content_provider: ContentProvider,
    /// The SVG renderer (public for ContentPipeline access)
    pub svg_renderer: Arc<SvgRenderer>,
}

impl RenderService {
    pub fn new(svg_dir: impl AsRef<Path>) -> Result<Self, RenderError> {
        Ok(Self {
            content_provider: ContentProvider::new(svg_dir),
            svg_renderer: Arc::new(SvgRenderer::new()),
        })
    }

    /// Render the default display content
    ///
    /// Uses spawn_blocking to avoid blocking the async runtime during
    /// CPU-intensive SVG rendering and dithering operations.
    pub async fn render_default(&self, spec: DisplaySpec) -> Result<Vec<u8>, RenderError> {
        let svg_data = self.content_provider.get_default_svg().await?;
        self.render_in_blocking_context(svg_data, spec).await
    }

    /// Execute CPU-intensive rendering in a blocking context
    async fn render_in_blocking_context(
        &self,
        svg_data: Vec<u8>,
        spec: DisplaySpec,
    ) -> Result<Vec<u8>, RenderError> {
        let renderer = self.svg_renderer.clone();

        tokio::task::spawn_blocking(move || renderer.render_to_png(&svg_data, spec))
            .await
            .map_err(|e| RenderError::SvgParse(format!("Render task failed: {e}")))?
    }
}
