//! Blue noise kernel weight jitter variants for JJN and Sierra algorithms.
//!
//! Each struct delegates to [`dither_with_kernel_noise`] with the appropriate
//! kernel constant, applying per-pixel blue noise jitter to the "right" and
//! "below" diffusion weights.

use crate::color::LinearRgb;
use crate::palette::Palette;

use super::{
    dither_with_kernel_noise, Dither, DitherOptions, JARVIS_JUDICE_NINKE, SIERRA, SIERRA_LITE,
    SIERRA_TWO_ROW,
};

/// Jarvis-Judice-Ninke with blue noise kernel weight jitter.
///
/// Shifts weight between the `(1,0)` and `(0,1)` kernel entries per pixel
/// using blue noise, breaking directional "worm" artifacts while preserving
/// 100% error propagation across the full 12-neighbor kernel.
pub struct JarvisJudiceNinkeNoise;

impl Dither for JarvisJudiceNinkeNoise {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel_noise(image, width, height, palette, &JARVIS_JUDICE_NINKE, options)
    }
}

/// Sierra (full) with blue noise kernel weight jitter.
///
/// Shifts weight between the `(1,0)` and `(0,1)` kernel entries per pixel
/// using blue noise, breaking directional "worm" artifacts while preserving
/// 100% error propagation across the full 10-neighbor kernel.
pub struct SierraNoise;

impl Dither for SierraNoise {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel_noise(image, width, height, palette, &SIERRA, options)
    }
}

/// Sierra Two-Row with blue noise kernel weight jitter.
///
/// Shifts weight between the `(1,0)` and `(0,1)` kernel entries per pixel
/// using blue noise, breaking directional "worm" artifacts while preserving
/// 100% error propagation across the 7-neighbor kernel.
pub struct SierraTwoRowNoise;

impl Dither for SierraTwoRowNoise {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel_noise(image, width, height, palette, &SIERRA_TWO_ROW, options)
    }
}

/// Sierra Lite with blue noise kernel weight jitter.
///
/// Shifts weight between the `(1,0)` and `(0,1)` kernel entries per pixel
/// using blue noise, breaking directional "worm" artifacts while preserving
/// 100% error propagation across the 3-neighbor kernel.
pub struct SierraLiteNoise;

impl Dither for SierraLiteNoise {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        dither_with_kernel_noise(image, width, height, palette, &SIERRA_LITE, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Srgb;
    use crate::dither::{JarvisJudiceNinke, Sierra, SierraLite, SierraTwoRow};

    fn make_bw_palette() -> Palette {
        let colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        Palette::new(&colors, None).unwrap()
    }

    #[test]
    fn test_jjn_noise_basic() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 64];

        let result = JarvisJudiceNinkeNoise.dither(&image, 8, 8, &palette, &options);
        assert_eq!(result.len(), 64);
        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();
        assert!(blacks > 0 && whites > 0);
    }

    #[test]
    fn test_jjn_noise_differs_from_plain() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 256];

        let plain = JarvisJudiceNinke.dither(&image, 16, 16, &palette, &options);
        let noise = JarvisJudiceNinkeNoise.dither(&image, 16, 16, &palette, &options);
        assert_ne!(plain, noise, "Jittered JJN should differ from plain JJN");
    }

    #[test]
    fn test_sierra_noise_basic() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 64];

        let result = SierraNoise.dither(&image, 8, 8, &palette, &options);
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_sierra_noise_differs_from_plain() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 256];

        let plain = Sierra.dither(&image, 16, 16, &palette, &options);
        let noise = SierraNoise.dither(&image, 16, 16, &palette, &options);
        assert_ne!(
            plain, noise,
            "Jittered Sierra should differ from plain Sierra"
        );
    }

    #[test]
    fn test_sierra_two_row_noise_basic() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 64];

        let result = SierraTwoRowNoise.dither(&image, 8, 8, &palette, &options);
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_sierra_two_row_noise_differs_from_plain() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 256];

        let plain = SierraTwoRow.dither(&image, 16, 16, &palette, &options);
        let noise = SierraTwoRowNoise.dither(&image, 16, 16, &palette, &options);
        assert_ne!(
            plain, noise,
            "Jittered Sierra Two-Row should differ from plain"
        );
    }

    #[test]
    fn test_sierra_lite_noise_basic() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 64];

        let result = SierraLiteNoise.dither(&image, 8, 8, &palette, &options);
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_sierra_lite_noise_differs_from_plain() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 256];

        let plain = SierraLite.dither(&image, 16, 16, &palette, &options);
        let noise = SierraLiteNoise.dither(&image, 16, 16, &palette, &options);
        assert_ne!(
            plain, noise,
            "Jittered Sierra Lite should differ from plain"
        );
    }

    #[test]
    fn test_noise_zero_scale_matches_plain_behavior() {
        // With noise_scale=0, the jitter shift is always 0, so weights
        // should be identical to the base kernel (same as plain variant).
        let palette = make_bw_palette();
        let options = DitherOptions::new().noise_scale(0.0);
        let gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![gray; 256];

        let plain = JarvisJudiceNinke.dither(&image, 16, 16, &palette, &options);
        let noise = JarvisJudiceNinkeNoise.dither(&image, 16, 16, &palette, &options);
        assert_eq!(
            plain, noise,
            "noise_scale=0 should produce same result as plain"
        );
    }

    #[test]
    fn test_all_noise_variants_pure_black() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let black = LinearRgb::new(0.0, 0.0, 0.0);
        let image = vec![black; 16];

        assert!(JarvisJudiceNinkeNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 0));
        assert!(SierraNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 0));
        assert!(SierraTwoRowNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 0));
        assert!(SierraLiteNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 0));
    }

    #[test]
    fn test_all_noise_variants_pure_white() {
        let palette = make_bw_palette();
        let options = DitherOptions::new();
        let white = LinearRgb::new(1.0, 1.0, 1.0);
        let image = vec![white; 16];

        assert!(JarvisJudiceNinkeNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 1));
        assert!(SierraNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 1));
        assert!(SierraTwoRowNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 1));
        assert!(SierraLiteNoise
            .dither(&image, 4, 4, &palette, &options)
            .iter()
            .all(|&x| x == 1));
    }
}
