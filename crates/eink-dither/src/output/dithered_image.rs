//! DitheredImage struct with three output format methods.
//!
//! [`DitheredImage`] wraps dithered palette indices with dimension metadata
//! and an owned [`Palette`], providing indexed, official RGB, and actual RGB
//! output formats. The indexed form is canonical; RGB outputs are computed
//! on demand by looking up palette colors.

use crate::palette::Palette;

/// The canonical output of the dithering pipeline.
///
/// Stores one `u8` palette index per pixel in row-major order, along with
/// image dimensions and the palette used for dithering. Three output formats
/// are available:
///
/// - [`indices()`](DitheredImage::indices): Raw palette indices (OUT-01)
/// - [`to_rgb_official()`](DitheredImage::to_rgb_official): RGB bytes using
///   official device colors, suitable for device upload (OUT-02)
/// - [`to_rgb_actual()`](DitheredImage::to_rgb_actual): RGB bytes using
///   actual measured colors, suitable for preview rendering (OUT-03)
///
/// # Example
///
/// ```
/// use eink_dither::{DitheredImage, Palette, Srgb};
///
/// let official = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
/// let palette = Palette::new(&official, None).unwrap();
///
/// // Simulate a 2x2 checkerboard dither result
/// let indices = vec![0, 1, 1, 0];
/// let image = DitheredImage::new(indices, 2, 2, palette);
///
/// assert_eq!(image.width(), 2);
/// assert_eq!(image.height(), 2);
/// assert_eq!(image.indices(), &[0, 1, 1, 0]);
///
/// // Official RGB for device upload
/// let rgb = image.to_rgb_official();
/// assert_eq!(rgb.len(), 2 * 2 * 3); // 4 pixels * 3 bytes each
/// ```
pub struct DitheredImage {
    /// Palette indices, one per pixel, row-major order.
    indices: Vec<u8>,
    /// Image width in pixels.
    width: usize,
    /// Image height in pixels.
    height: usize,
    /// The palette used for dithering (owned for ergonomic return values).
    palette: Palette,
}

impl DitheredImage {
    /// Create a new `DitheredImage` from dithered palette indices.
    ///
    /// # Arguments
    ///
    /// * `indices` - Palette indices, one `u8` per pixel, in row-major order.
    ///   Each value must be in `0..palette.len()`.
    /// * `width` - Image width in pixels.
    /// * `height` - Image height in pixels.
    /// * `palette` - The palette used for dithering (cloned into the struct).
    ///
    /// # Panics (debug only)
    ///
    /// Debug-asserts that `indices.len() == width * height`.
    pub fn new(indices: Vec<u8>, width: usize, height: usize, palette: Palette) -> Self {
        debug_assert_eq!(
            indices.len(),
            width * height,
            "indices length ({}) must match width * height ({}x{}={})",
            indices.len(),
            width,
            height,
            width * height,
        );
        Self {
            indices,
            width,
            height,
            palette,
        }
    }

    /// Returns the palette indices as a slice.
    ///
    /// Each element is a `u8` index into the palette, in row-major order.
    /// This is the indexed output format (OUT-01).
    #[inline]
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }

    /// Returns the image width in pixels.
    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the image height in pixels.
    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns a reference to the palette used for this image.
    #[inline]
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Convert to RGB bytes using official device colors (OUT-02).
    ///
    /// Maps each palette index through [`Palette::official()`] to produce
    /// a flat byte buffer in `[R, G, B, R, G, B, ...]` layout, suitable
    /// for device upload.
    ///
    /// The returned buffer has length `width * height * 3`.
    pub fn to_rgb_official(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.indices.len() * 3);
        for &idx in &self.indices {
            let [r, g, b] = self.palette.official(idx as usize).to_bytes();
            rgb.push(r);
            rgb.push(g);
            rgb.push(b);
        }
        rgb
    }

    /// Convert to RGB bytes using actual measured colors (OUT-03).
    ///
    /// Maps each palette index through [`Palette::actual()`] to produce
    /// a flat byte buffer in `[R, G, B, R, G, B, ...]` layout, suitable
    /// for preview rendering that shows the true appearance on the display.
    ///
    /// The returned buffer has length `width * height * 3`.
    pub fn to_rgb_actual(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.indices.len() * 3);
        for &idx in &self.indices {
            let [r, g, b] = self.palette.actual(idx as usize).to_bytes();
            rgb.push(r);
            rgb.push(g);
            rgb.push(b);
        }
        rgb
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Srgb;

    /// Helper: create a palette with distinct official and actual colors.
    fn dual_palette() -> Palette {
        let official = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(255, 255, 255), // white
        ];
        let actual = [
            Srgb::from_u8(10, 10, 10),    // near-black
            Srgb::from_u8(200, 50, 50),   // muddy red
            Srgb::from_u8(230, 230, 220), // off-white
        ];
        Palette::new(&official, Some(&actual)).unwrap()
    }

    #[test]
    fn test_new_stores_fields() {
        let palette = dual_palette();
        let indices = vec![0, 1, 2, 0, 1, 2];
        let image = DitheredImage::new(indices.clone(), 3, 2, palette.clone());

        assert_eq!(image.indices(), &[0, 1, 2, 0, 1, 2]);
        assert_eq!(image.width(), 3);
        assert_eq!(image.height(), 2);
        assert_eq!(image.palette().len(), 3);
    }

    #[test]
    fn test_to_rgb_official_uses_official_colors() {
        let palette = dual_palette();
        // Single pixel with index 1 (red)
        let image = DitheredImage::new(vec![1], 1, 1, palette);
        let rgb = image.to_rgb_official();

        // Official color for index 1 is (255, 0, 0)
        assert_eq!(rgb, vec![255, 0, 0]);
    }

    #[test]
    fn test_to_rgb_actual_uses_actual_colors() {
        let palette = dual_palette();
        // Single pixel with index 1 (actual: muddy red)
        let image = DitheredImage::new(vec![1], 1, 1, palette);
        let rgb = image.to_rgb_actual();

        // Actual color for index 1 is (200, 50, 50)
        assert_eq!(rgb, vec![200, 50, 50]);
    }

    #[test]
    fn test_rgb_output_length() {
        let palette = dual_palette();
        let w = 4;
        let h = 3;
        let indices = vec![0; w * h];
        let image = DitheredImage::new(indices, w, h, palette);

        assert_eq!(image.to_rgb_official().len(), w * h * 3);
        assert_eq!(image.to_rgb_actual().len(), w * h * 3);
    }

    #[test]
    fn test_rgb_output_layout() {
        let palette = dual_palette();
        // Two pixels: index 0 (black) then index 2 (white)
        let image = DitheredImage::new(vec![0, 2], 2, 1, palette);

        let rgb = image.to_rgb_official();
        // Expected: [R0, G0, B0, R1, G1, B1]
        // Index 0 official = (0, 0, 0), Index 2 official = (255, 255, 255)
        assert_eq!(rgb, vec![0, 0, 0, 255, 255, 255]);
    }

    #[test]
    fn test_single_palette_color() {
        let colors = [Srgb::from_u8(128, 64, 32)];
        let palette = Palette::new(&colors, None).unwrap();
        let indices = vec![0; 6]; // 3x2 image, all same color
        let image = DitheredImage::new(indices, 3, 2, palette);

        let rgb = image.to_rgb_official();
        assert_eq!(rgb.len(), 18); // 6 pixels * 3 bytes

        // Every pixel should be (128, 64, 32)
        for chunk in rgb.chunks(3) {
            assert_eq!(chunk, &[128, 64, 32]);
        }

        // Official and actual should be identical when no actual provided
        assert_eq!(image.to_rgb_official(), image.to_rgb_actual());
    }
}
