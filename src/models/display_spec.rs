use crate::error::RenderError;

/// Display specifications for different TRMNL models
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySpec {
    pub width: u32,
    pub height: u32,
    pub max_size_bytes: usize,
}

impl DisplaySpec {
    /// Original TRMNL: 800x480
    pub const OG: Self = Self {
        width: 800,
        height: 480,
        max_size_bytes: 90_000, // 90KB
    };

    /// TRMNL X: 1872x1404
    pub const X: Self = Self {
        width: 1872,
        height: 1404,
        max_size_bytes: 750_000, // 750KB
    };

    /// Determine spec from Width/Height headers
    pub fn from_dimensions(width: u32, height: u32) -> Result<Self, RenderError> {
        match (width, height) {
            (800, 480) => Ok(Self::OG),
            (1872, 1404) => Ok(Self::X),
            // Fallback: if dimensions are close to OG, use OG; otherwise X
            (w, h) if w <= 800 && h <= 480 => Ok(Self::OG),
            (w, h) if w > 800 || h > 480 => Ok(Self::X),
            _ => Err(RenderError::UnsupportedDimensions { width, height }),
        }
    }

    /// Validate that the image size is within limits
    pub fn validate_size(&self, bytes: usize) -> Result<(), RenderError> {
        if bytes > self.max_size_bytes {
            Err(RenderError::ImageTooLarge {
                size: bytes,
                max: self.max_size_bytes,
            })
        } else {
            Ok(())
        }
    }
}
