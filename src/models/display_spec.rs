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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_og_dimensions() {
        let spec = DisplaySpec::from_dimensions(800, 480).unwrap();
        assert_eq!(spec, DisplaySpec::OG);
        assert_eq!(spec.width, 800);
        assert_eq!(spec.height, 480);
        assert_eq!(spec.max_size_bytes, 90_000);
    }

    #[test]
    fn test_x_dimensions() {
        let spec = DisplaySpec::from_dimensions(1872, 1404).unwrap();
        assert_eq!(spec, DisplaySpec::X);
        assert_eq!(spec.width, 1872);
        assert_eq!(spec.height, 1404);
        assert_eq!(spec.max_size_bytes, 750_000);
    }

    #[test]
    fn test_smaller_dimensions_fallback_to_og() {
        // Dimensions smaller than OG should fall back to OG
        let spec = DisplaySpec::from_dimensions(640, 480).unwrap();
        assert_eq!(spec, DisplaySpec::OG);

        let spec = DisplaySpec::from_dimensions(400, 300).unwrap();
        assert_eq!(spec, DisplaySpec::OG);

        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        assert_eq!(spec, DisplaySpec::OG);
    }

    #[test]
    fn test_larger_dimensions_fallback_to_x() {
        // Dimensions larger than OG should fall back to X
        let spec = DisplaySpec::from_dimensions(1024, 768).unwrap();
        assert_eq!(spec, DisplaySpec::X);

        let spec = DisplaySpec::from_dimensions(801, 480).unwrap();
        assert_eq!(spec, DisplaySpec::X);

        let spec = DisplaySpec::from_dimensions(800, 481).unwrap();
        assert_eq!(spec, DisplaySpec::X);
    }

    #[test]
    fn test_validate_size_ok() {
        let spec = DisplaySpec::OG;
        assert!(spec.validate_size(50_000).is_ok());
        assert!(spec.validate_size(90_000).is_ok());
    }

    #[test]
    fn test_validate_size_too_large() {
        let spec = DisplaySpec::OG;
        let result = spec.validate_size(90_001);
        assert!(result.is_err());

        match result.unwrap_err() {
            RenderError::ImageTooLarge { size, max } => {
                assert_eq!(size, 90_001);
                assert_eq!(max, 90_000);
            }
            _ => panic!("Expected ImageTooLarge error"),
        }
    }

    #[test]
    fn test_validate_size_x_model() {
        let spec = DisplaySpec::X;
        assert!(spec.validate_size(500_000).is_ok());
        assert!(spec.validate_size(750_000).is_ok());
        assert!(spec.validate_size(750_001).is_err());
    }
}
