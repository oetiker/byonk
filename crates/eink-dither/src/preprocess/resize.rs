//! Image resizing stub.
//!
//! The `image` crate dependency has been removed. Resize is a no-op when
//! dimensions already match; panics otherwise (byonk never requests resize).

use crate::color::Srgb;

/// No-op resize: returns input unchanged if dimensions match, panics otherwise.
///
/// # Panics
///
/// Panics if `new_width != width` or `new_height != height`. The vendored
/// eink-dither crate is used without the `image` dependency, so actual
/// resizing is not available. The caller (byonk) never requests a resize.
pub fn resize_lanczos(
    pixels: &[Srgb],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> (Vec<Srgb>, u32, u32) {
    if width == new_width && height == new_height {
        return (pixels.to_vec(), width, height);
    }

    panic!(
        "eink-dither: resize not available (image crate removed). \
         Requested {}x{} -> {}x{}",
        width, height, new_width, new_height
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_image(width: u32, height: u32, color: Srgb) -> Vec<Srgb> {
        vec![color; (width * height) as usize]
    }

    #[test]
    fn test_resize_noop_same_dimensions() {
        let input = solid_image(100, 100, Srgb::from_u8(128, 128, 128));
        let (output, w, h) = resize_lanczos(&input, 100, 100, 100, 100);

        assert_eq!(w, 100, "Width should stay 100");
        assert_eq!(h, 100, "Height should stay 100");
        assert_eq!(output.len(), input.len(), "Length should be unchanged");

        for (out_pixel, in_pixel) in output.iter().zip(input.iter()) {
            assert_eq!(
                out_pixel.to_bytes(),
                in_pixel.to_bytes(),
                "Pixels should be unchanged for no-op resize"
            );
        }
    }

    #[test]
    #[should_panic(expected = "resize not available")]
    fn test_resize_panics_when_dimensions_differ() {
        let input = solid_image(100, 100, Srgb::from_u8(128, 128, 128));
        let _ = resize_lanczos(&input, 100, 100, 50, 50);
    }
}
