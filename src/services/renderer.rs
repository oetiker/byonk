use crate::error::RenderError;
use crate::rendering::SvgRenderer;
use std::path::Path;
use std::sync::Arc;

/// High-level render service that provides SVG rendering
pub struct RenderService {
    /// The SVG renderer (public for ContentPipeline access)
    pub svg_renderer: Arc<SvgRenderer>,
}

impl RenderService {
    pub fn new(_svg_dir: impl AsRef<Path>) -> Result<Self, RenderError> {
        Ok(Self {
            svg_renderer: Arc::new(SvgRenderer::new()),
        })
    }
}
