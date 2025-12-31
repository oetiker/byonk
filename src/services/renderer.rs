use crate::assets::AssetLoader;
use crate::error::RenderError;
use crate::rendering::SvgRenderer;
use std::sync::Arc;

/// High-level render service that provides SVG rendering
pub struct RenderService {
    /// The SVG renderer (public for ContentPipeline access)
    pub svg_renderer: Arc<SvgRenderer>,
}

impl RenderService {
    pub fn new(asset_loader: &AssetLoader) -> Result<Self, RenderError> {
        let fonts = asset_loader.get_fonts();
        Ok(Self {
            svg_renderer: Arc::new(SvgRenderer::with_fonts(fonts)),
        })
    }
}
