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
        max_size_bytes: 200_000, // 200KB (accommodates color palettes)
    };

    /// TRMNL X: 1872x1404
    pub const X: Self = Self {
        width: 1872,
        height: 1404,
        max_size_bytes: 750_000, // 750KB
    };

    /// Determine spec from Width/Height headers.
    /// Uses the exact dimensions provided, with size limits based on display tier.
    pub fn from_dimensions(width: u32, height: u32) -> Result<Self, RenderError> {
        let max_size_bytes = if width <= 800 && height <= 480 {
            Self::OG.max_size_bytes
        } else {
            Self::X.max_size_bytes
        };
        Ok(Self {
            width,
            height,
            max_size_bytes,
        })
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
        assert_eq!(spec.width, 800);
        assert_eq!(spec.height, 480);
        assert_eq!(spec.max_size_bytes, 200_000);
    }

    #[test]
    fn test_x_dimensions() {
        let spec = DisplaySpec::from_dimensions(1872, 1404).unwrap();
        assert_eq!(spec.width, 1872);
        assert_eq!(spec.height, 1404);
        assert_eq!(spec.max_size_bytes, 750_000);
    }

    #[test]
    fn test_custom_dimensions_og_tier() {
        // Small displays use OG size limits but keep exact dimensions
        let spec = DisplaySpec::from_dimensions(296, 128).unwrap();
        assert_eq!(spec.width, 296);
        assert_eq!(spec.height, 128);
        assert_eq!(spec.max_size_bytes, 200_000);

        let spec = DisplaySpec::from_dimensions(400, 300).unwrap();
        assert_eq!(spec.width, 400);
        assert_eq!(spec.height, 300);
        assert_eq!(spec.max_size_bytes, 200_000);
    }

    #[test]
    fn test_custom_dimensions_x_tier() {
        // Large displays use X size limits but keep exact dimensions
        let spec = DisplaySpec::from_dimensions(1024, 768).unwrap();
        assert_eq!(spec.width, 1024);
        assert_eq!(spec.height, 768);
        assert_eq!(spec.max_size_bytes, 750_000);

        let spec = DisplaySpec::from_dimensions(801, 480).unwrap();
        assert_eq!(spec.width, 801);
        assert_eq!(spec.height, 480);
        assert_eq!(spec.max_size_bytes, 750_000);
    }

    #[test]
    fn test_validate_size_ok() {
        let spec = DisplaySpec::OG;
        assert!(spec.validate_size(50_000).is_ok());
        assert!(spec.validate_size(200_000).is_ok());
    }

    #[test]
    fn test_validate_size_too_large() {
        let spec = DisplaySpec::OG;
        let result = spec.validate_size(200_001);
        assert!(result.is_err());

        match result.unwrap_err() {
            RenderError::ImageTooLarge { size, max } => {
                assert_eq!(size, 200_001);
                assert_eq!(max, 200_000);
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
