use std::path::{Path, PathBuf};

use crate::error::RenderError;

/// Provides SVG content for devices
pub struct ContentProvider {
    svg_dir: PathBuf,
}

impl ContentProvider {
    pub fn new(svg_dir: impl AsRef<Path>) -> Self {
        Self {
            svg_dir: svg_dir.as_ref().to_path_buf(),
        }
    }

    /// Get the default SVG content for display
    pub async fn get_default_svg(&self) -> Result<Vec<u8>, RenderError> {
        self.read_svg_file("default.svg").await
    }

    /// Internal helper to read SVG files with proper error logging
    async fn read_svg_file(&self, filename: &str) -> Result<Vec<u8>, RenderError> {
        let path = self.svg_dir.join(filename);
        tokio::fs::read(&path).await.map_err(|e| {
            tracing::warn!(
                filename = filename,
                error = %e,
                "Failed to read SVG file"
            );
            // Don't expose full path in error message
            RenderError::SvgNotFound(filename.to_string())
        })
    }
}
