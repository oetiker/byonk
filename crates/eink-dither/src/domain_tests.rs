//! Domain-critical regression tests for eink-dither.
//!
//! These tests are designed to catch specific classes of bugs, not just
//! confirm happy paths. Each test documents the regression it guards against.

#[cfg(test)]
mod domain_tests {
    use crate::api::EinkDitherer;
    use crate::color::{LinearRgb, Oklab, Srgb};
    use crate::dither::{
        Atkinson, BlueNoiseDither, Dither, DitherOptions, FloydSteinberg, JarvisJudiceNinke,
        Sierra, SierraLite, SierraTwoRow,
    };
    use crate::output::RenderingIntent;
    use crate::palette::Palette;

    // ========================================================================
    // GAP 1: Gamma correctness -- dithering must happen in linear RGB space
    // ========================================================================

    /// If this breaks, it means: the dithering pipeline is operating in sRGB
    /// space instead of linear RGB, causing mid-tones to be reproduced too
    /// brightly. sRGB 186 is approximately linear 0.5; dithering to B&W
    /// should produce ~50% white pixels. sRGB 128 is approximately linear
    /// 0.214; if dithered in sRGB space it would appear as ~50% white instead
    /// of the correct ~21%.
    #[test]
    fn test_gamma_correctness_dither_ratios() {
        let palette = Palette::new(
            &[Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)],
            None,
        )
        .unwrap();
        let options = DitherOptions::new().serpentine(false);
        let size = 32;
        let total = size * size;

        // Test 1: sRGB 186 is approximately linear 0.5 -- should produce ~50% white
        let gray_186 = LinearRgb::from(Srgb::from_u8(186, 186, 186));
        let image_186 = vec![gray_186; total];
        let result_186 = Atkinson.dither(&image_186, size, size, &palette, &options);
        let white_count_186 = result_186.iter().filter(|&&idx| idx == 1).count();
        let ratio_186 = white_count_186 as f64 / total as f64;

        assert!(
            (ratio_186 - 0.5).abs() < 0.15,
            "REGRESSION: sRGB 186 gray produced {:.3} white ratio, expected ~0.50 (linear 0.5). \
             Tolerance is 0.15 for 32x32 error diffusion noise.",
            ratio_186
        );

        // Test 2: sRGB 128 is approximately linear 0.214 -- should be < 0.35
        let gray_128 = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image_128 = vec![gray_128; total];
        let result_128 = Atkinson.dither(&image_128, size, size, &palette, &options);
        let white_count_128 = result_128.iter().filter(|&&idx| idx == 1).count();
        let ratio_128 = white_count_128 as f64 / total as f64;

        assert!(
            (ratio_128 - 0.214).abs() < 0.1,
            "REGRESSION: sRGB 128 gray produced {:.3} white ratio, expected ~0.21 (linear). \
             The ratio is outside the 0.1 tolerance band.",
            ratio_128
        );
        assert!(
            ratio_128 < 0.35,
            "REGRESSION: sRGB 128 gray produced {:.3} white pixels, expected ~0.21 (linear). \
             If > 0.35, dithering is likely happening in sRGB space instead of linear RGB.",
            ratio_128
        );
    }

    // ========================================================================
    // GAP 2: All algorithms produce valid palette indices for all palette sizes
    // ========================================================================

    /// If this breaks, it means: a dithering algorithm is producing out-of-bounds
    /// palette indices, which would cause panics or garbage output when looking up
    /// colors from the palette.
    #[test]
    fn test_all_algorithms_valid_palette_indices() {
        let palette_sizes: &[usize] = &[1, 2, 3, 5, 7, 16];
        let options = DitherOptions::new();
        let size = 16;

        // Create a 16x16 varied input image in LinearRgb
        let image: Vec<LinearRgb> = (0..size * size)
            .map(|i| {
                LinearRgb::new(
                    (i as f32 / 255.0).min(1.0),
                    ((i * 3) as f32 % 256.0) / 255.0,
                    ((i * 7) as f32 % 256.0) / 255.0,
                )
            })
            .collect();

        for &pal_size in palette_sizes {
            // Generate unique palette colors for each size
            let colors: Vec<Srgb> = if pal_size == 1 {
                vec![Srgb::from_u8(128, 128, 128)]
            } else {
                (0..pal_size)
                    .map(|i| {
                        // Spread channels to guarantee uniqueness
                        let r = (i * (255 / (pal_size - 1).max(1))) as u8;
                        let g = ((i * 37) % 256) as u8;
                        let b = ((i * 73) % 256) as u8;
                        Srgb::from_u8(r, g, b)
                    })
                    .collect()
            };

            let palette = match Palette::new(&colors, None) {
                Ok(p) => p,
                Err(_) => {
                    // If colors collide, use a simpler spread
                    let colors: Vec<Srgb> = (0..pal_size)
                        .map(|i| {
                            let v = (i * (255 / pal_size.max(1))) as u8;
                            let g = ((i * 97 + 30) % 256) as u8;
                            let b = ((i * 151 + 60) % 256) as u8;
                            Srgb::from_u8(v, g, b)
                        })
                        .collect();
                    Palette::new(&colors, None)
                        .expect("Fallback palette should not have duplicates")
                }
            };

            // Test all 7 algorithms
            let algorithms: Vec<(&str, Box<dyn Dither>)> = vec![
                ("Atkinson", Box::new(Atkinson)),
                ("FloydSteinberg", Box::new(FloydSteinberg)),
                ("JarvisJudiceNinke", Box::new(JarvisJudiceNinke)),
                ("Sierra", Box::new(Sierra)),
                ("SierraTwoRow", Box::new(SierraTwoRow)),
                ("SierraLite", Box::new(SierraLite)),
                ("BlueNoiseDither", Box::new(BlueNoiseDither)),
            ];

            for (name, algorithm) in &algorithms {
                let result = algorithm.dither(&image, size, size, &palette, &options);

                assert_eq!(
                    result.len(),
                    size * size,
                    "REGRESSION: {} produced wrong output length for palette size {}",
                    name,
                    pal_size,
                );

                for (px, &idx) in result.iter().enumerate() {
                    assert!(
                        (idx as usize) < palette.len(),
                        "REGRESSION: {} produced index {} at pixel {} for palette of size {}. \
                         Output indices must be in 0..{}.",
                        name,
                        idx,
                        px,
                        pal_size,
                        pal_size,
                    );
                }
            }
        }
    }

    // ========================================================================
    // GAP 3: Realistic e-ink 7-color palette behavior
    // ========================================================================

    /// If this breaks, it means: the perceptual color matching is mapping
    /// colors to implausible palette entries (e.g., orange input mapped to blue),
    /// or the palette matching is stuck using only a subset of available colors.
    #[test]
    fn test_realistic_eink_7color_palette() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(255, 0, 0),     // 2: red
            Srgb::from_u8(0, 255, 0),     // 3: green
            Srgb::from_u8(0, 0, 255),     // 4: blue
            Srgb::from_u8(255, 255, 0),   // 5: yellow
            Srgb::from_u8(255, 128, 0),   // 6: orange
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Test 1: Orange input should not map to blue
        {
            let orange_pixel = Srgb::from_u8(255, 140, 0);
            let image = vec![orange_pixel; 8 * 8];
            let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Photo)
                .saturation(1.0)
                .contrast(1.0);
            let result = ditherer.dither(&image, 8, 8);
            let indices = result.indices();

            let has_blue = indices.iter().any(|&idx| idx == 4);
            assert!(
                !has_blue,
                "REGRESSION: Orange input mapped to blue on a 7-color e-ink palette. \
                 The perceptual color matching is broken."
            );

            let has_warm = indices.iter().any(|&idx| idx == 2 || idx == 5 || idx == 6);
            assert!(
                has_warm,
                "REGRESSION: Orange input did not use any warm colors (red/yellow/orange). \
                 Palette matching is not selecting perceptually close colors."
            );
        }

        // Test 2: Varied colorful input should use palette breadth (Photo intent)
        {
            let image: Vec<Srgb> = (0..16 * 16)
                .map(|i| {
                    let hue = (i as f32 / 256.0) * 360.0;
                    // Simple HSV-to-RGB with full saturation and value
                    let h = hue / 60.0;
                    let sector = h.floor() as usize % 6;
                    let f = h - h.floor();
                    let q = 1.0 - f;
                    let t = f;
                    let (r, g, b) = match sector {
                        0 => (1.0, t, 0.0),
                        1 => (q, 1.0, 0.0),
                        2 => (0.0, 1.0, t),
                        3 => (0.0, q, 1.0),
                        4 => (t, 0.0, 1.0),
                        _ => (1.0, 0.0, q),
                    };
                    Srgb::new(r, g, b)
                })
                .collect();

            let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Photo)
                .saturation(1.0)
                .contrast(1.0);
            let result = ditherer.dither(&image, 16, 16);
            let indices = result.indices();

            let unique_count = {
                let mut seen = std::collections::HashSet::new();
                for &idx in indices {
                    seen.insert(idx);
                }
                seen.len()
            };

            assert!(
                unique_count >= 3,
                "REGRESSION: Varied colorful input used only {} palette entries out of 7. \
                 Palette matching may be stuck on a subset.",
                unique_count
            );
        }

        // Test 3: Graphics intent also works correctly
        {
            let image: Vec<Srgb> = (0..16 * 16)
                .map(|i| {
                    let hue = (i as f32 / 256.0) * 360.0;
                    let h = hue / 60.0;
                    let sector = h.floor() as usize % 6;
                    let f = h - h.floor();
                    let q = 1.0 - f;
                    let t = f;
                    let (r, g, b) = match sector {
                        0 => (1.0, t, 0.0),
                        1 => (q, 1.0, 0.0),
                        2 => (0.0, 1.0, t),
                        3 => (0.0, q, 1.0),
                        4 => (t, 0.0, 1.0),
                        _ => (1.0, 0.0, q),
                    };
                    Srgb::new(r, g, b)
                })
                .collect();

            let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Graphics);
            let result = ditherer.dither(&image, 16, 16);
            let indices = result.indices();

            // All indices valid
            for &idx in indices {
                assert!(
                    (idx as usize) < palette.len(),
                    "REGRESSION: Graphics intent produced invalid index {} for 7-color palette.",
                    idx
                );
            }

            let unique_count = {
                let mut seen = std::collections::HashSet::new();
                for &idx in indices {
                    seen.insert(idx);
                }
                seen.len()
            };

            assert!(
                unique_count >= 3,
                "REGRESSION: Graphics intent used only {} palette entries on varied input. \
                 Palette matching may be stuck on a subset.",
                unique_count
            );
        }
    }

    // ========================================================================
    // GAP 4: Blue noise spatial uniformity
    // ========================================================================

    /// If this breaks, it means: the blue noise dithering has lost its spatial
    /// uniformity property -- dots are clumping in some regions and sparse in
    /// others, which would produce visible banding or texture artifacts instead
    /// of the smooth, organic pattern expected from blue noise.
    #[test]
    fn test_blue_noise_spatial_uniformity() {
        let palette = Palette::new(
            &[Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)],
            None,
        )
        .unwrap();
        let options = DitherOptions::new();
        let size = 64;

        // 64x64 uniform gray image. We use sRGB 128 converted to linear (~0.214)
        // which produces a meaningful mix of black and white pixels. The blue noise
        // algorithm uses Oklab perceptual distances for blend factor, so the white
        // ratio will not exactly equal the linear brightness value.
        let mid_gray = LinearRgb::from(Srgb::from_u8(128, 128, 128));
        let image = vec![mid_gray; size * size];
        let result = BlueNoiseDither.dither(&image, size, size, &palette, &options);

        // Overall sanity check: should have a reasonable mix (not all one color)
        let total_white = result.iter().filter(|&&idx| idx == 1).count();
        let overall_ratio = total_white as f64 / (size * size) as f64;
        assert!(
            overall_ratio > 0.1 && overall_ratio < 0.9,
            "REGRESSION: Blue noise overall white ratio is {:.3}, expected between 0.1 and 0.9. \
             Basic threshold behavior may be broken.",
            overall_ratio
        );

        // Divide into 16 blocks of 16x16 and compute per-block white ratios
        let block_size = 16;
        let blocks_per_side = size / block_size;
        let mut block_means = Vec::new();

        for by in 0..blocks_per_side {
            for bx in 0..blocks_per_side {
                let mut white_count = 0;
                for y in 0..block_size {
                    for x in 0..block_size {
                        let px = by * block_size + y;
                        let py = bx * block_size + x;
                        if result[px * size + py] == 1 {
                            white_count += 1;
                        }
                    }
                }
                block_means.push(white_count as f64 / (block_size * block_size) as f64);
            }
        }

        // Compute variance of block means
        let mean_of_means: f64 = block_means.iter().sum::<f64>() / block_means.len() as f64;
        let variance: f64 = block_means
            .iter()
            .map(|m| (m - mean_of_means).powi(2))
            .sum::<f64>()
            / block_means.len() as f64;

        assert!(
            variance < 0.02,
            "REGRESSION: Blue noise block variance {:.4} exceeds threshold 0.02. \
             Spatial distribution is not uniform -- possibly replaced with white noise \
             or Bayer matrix. Block means: {:?}",
            variance,
            block_means
        );
    }

    // ========================================================================
    // GAP 5: Out-of-gamut resilience with extreme preprocessing
    // ========================================================================

    /// If this breaks, it means: extreme preprocessing parameters (high
    /// saturation and contrast) are causing the pipeline to produce invalid
    /// output -- either panics from out-of-range values, or palette indices
    /// that exceed the palette size. The clamping and bounds checking in the
    /// preprocessing and dithering stages is not working correctly.
    #[test]
    fn test_out_of_gamut_extreme_preprocessing() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(0, 255, 0),     // green
            Srgb::from_u8(0, 0, 255),     // blue
            Srgb::from_u8(255, 255, 0),   // yellow
            Srgb::from_u8(255, 128, 0),   // orange
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Create 16x16 image with saturated colors and grays
        let image: Vec<Srgb> = (0..16 * 16)
            .map(|i| match i % 5 {
                0 => Srgb::from_u8(255, 0, 0),     // pure red
                1 => Srgb::from_u8(0, 255, 0),     // pure green
                2 => Srgb::from_u8(0, 0, 255),     // pure blue
                3 => Srgb::from_u8(128, 128, 128), // mid-gray
                _ => Srgb::from_u8(200, 100, 50),  // brownish
            })
            .collect();

        // Extreme preprocessing: saturation 3.0, contrast 2.0
        let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Photo)
            .saturation(3.0)
            .contrast(2.0);

        // This should not panic (implicit test)
        let result = ditherer.dither(&image, 16, 16);

        assert_eq!(
            result.indices().len(),
            16 * 16,
            "REGRESSION: Extreme preprocessing produced wrong output length."
        );

        for (px, &idx) in result.indices().iter().enumerate() {
            assert!(
                (idx as usize) < palette.len(),
                "REGRESSION: Extreme preprocessing (sat=3.0, contrast=2.0) produced invalid \
                 index {} at pixel {}. Out-of-gamut clamping is broken.",
                idx,
                px,
            );
        }
    }

    // ========================================================================
    // GAP 6: Large image numerical stability
    // ========================================================================

    /// If this breaks, it means: error diffusion is numerically unstable at
    /// scale -- accumulated floating-point errors are blowing up to produce
    /// NaN, Inf, or garbage palette indices in a 200x200 image. This can
    /// happen if error clamping is removed or if f32 precision issues cascade
    /// through the error buffer over many rows.
    #[test]
    fn test_large_image_numerical_stability() {
        let palette_colors = [Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let width = 200;
        let height = 200;
        let total = width * height;

        // Test via EinkDitherer (full pipeline with sRGB input)
        {
            let image = vec![Srgb::from_u8(128, 128, 128); total];
            let ditherer = EinkDitherer::new(palette.clone(), RenderingIntent::Photo)
                .saturation(1.0)
                .contrast(1.0);
            let result = ditherer.dither(&image, width, height);

            assert_eq!(
                result.indices().len(),
                total,
                "REGRESSION: 200x200 output length mismatch."
            );

            for &idx in result.indices() {
                assert!(
                    idx <= 1,
                    "REGRESSION: 200x200 dithered image has invalid index {}. \
                     Expected only 0 or 1 for B&W palette.",
                    idx
                );
            }

            let white_count = result.indices().iter().filter(|&&idx| idx == 1).count();
            let ratio = white_count as f64 / total as f64;
            assert!(
                ratio > 0.05 && ratio < 0.95,
                "REGRESSION: 200x200 dithered image has {:.3} white ratio. \
                 Expected reasonable distribution. Error diffusion may have numerical blowup.",
                ratio
            );
        }

        // Test FloydSteinberg directly with LinearRgb at 200x200
        {
            let gray_linear = LinearRgb::from(Srgb::from_u8(128, 128, 128));
            let image = vec![gray_linear; total];
            let options = DitherOptions::new();
            let result = FloydSteinberg.dither(&image, width, height, &palette, &options);

            assert_eq!(
                result.len(),
                total,
                "REGRESSION: FloydSteinberg 200x200 output length mismatch."
            );

            for &idx in &result {
                assert!(
                    idx <= 1,
                    "REGRESSION: FloydSteinberg 200x200 produced invalid index {}.",
                    idx
                );
            }

            let white_count = result.iter().filter(|&&idx| idx == 1).count();
            let ratio = white_count as f64 / total as f64;
            assert!(
                ratio > 0.05 && ratio < 0.95,
                "REGRESSION: FloydSteinberg 200x200 has {:.3} white ratio. \
                 Expected reasonable distribution. Error diffusion may have numerical blowup.",
                ratio
            );
        }
    }

    // ========================================================================
    // GAP 7: Edge-case color mapping (TEST-03, TEST-04)
    // ========================================================================

    /// TEST-03: Pastel colors must not lose chroma information during dithering.
    ///
    /// On a BWRGBY palette, pastels like light pink correctly map to WHITE in
    /// per-pixel find_nearest (white is genuinely the closest palette color).
    /// However, the error diffusion must propagate the chroma error to neighbors,
    /// producing SOME chromatic pixels in the output. If the output is 100%
    /// achromatic, chroma information is being lost.
    ///
    /// If this breaks, it means: the chroma coupling penalty is too aggressive
    /// and suppressing all chromatic signal even through error diffusion, OR
    /// the preprocessing is desaturating pastels to pure grey.
    #[test]
    fn test_pastel_produces_chromatic_pixels_in_dither() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(255, 0, 0),     // 2: red
            Srgb::from_u8(0, 255, 0),     // 3: green
            Srgb::from_u8(0, 0, 255),     // 4: blue
            Srgb::from_u8(255, 255, 0),   // 5: yellow
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Light pink (255,182,193) has chroma=0.086 -- clearly chromatic but
        // closer to white (L=1.0) than to red (L=0.628) in lightness.
        let light_pink = Srgb::from_u8(255, 182, 193);
        let image = vec![light_pink; 32 * 32];

        // Use Photo intent with neutral preprocessing to isolate dithering behavior
        let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo)
            .saturation(1.0)
            .contrast(1.0);
        let result = ditherer.dither(&image, 32, 32);
        let indices = result.indices();

        // Must have SOME chromatic pixels from error diffusion
        let chromatic_count = indices.iter().filter(|&&idx| idx >= 2).count();
        assert!(
            chromatic_count > 0,
            "REGRESSION (TEST-03): Light pink (255,182,193) dithered to 100% achromatic. \
             Error diffusion should propagate chroma error to produce some chromatic pixels. \
             Chroma information is being lost."
        );

        // Must also contain some white or black pixels -- a pure uniform image
        // should produce a dithered MIX, not map entirely to one color.
        let achromatic_count = indices.iter().filter(|&&idx| idx <= 1).count();
        assert!(
            achromatic_count > 0,
            "REGRESSION (TEST-03): Light pink dithered to 100% chromatic ({} chromatic, 0 achromatic). \
             Error diffusion should produce a mix of white and chromatic pixels.",
            chromatic_count
        );
    }

    /// TEST-03 extended: Pale blue also preserves chroma through dithering.
    #[test]
    fn test_pale_blue_produces_chromatic_pixels_in_dither() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let pale_blue = Srgb::from_u8(173, 216, 230);
        let image = vec![pale_blue; 32 * 32];

        let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo)
            .saturation(1.0)
            .contrast(1.0);
        let result = ditherer.dither(&image, 32, 32);
        let indices = result.indices();

        let chromatic_count = indices.iter().filter(|&&idx| idx >= 2).count();
        assert!(
            chromatic_count > 0,
            "REGRESSION (TEST-03): Pale blue (173,216,230) dithered to 100% achromatic. \
             Error diffusion should propagate chroma error to produce some blue pixels."
        );
    }

    /// TEST-04: Brown maps to red (nearest warm chromatic) on BWRGBY.
    ///
    /// If this breaks, it means: the HyAB distance metric is not correctly
    /// balancing lightness vs chrominance for dark warm colors.
    #[test]
    fn test_brown_maps_to_red() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let brown = Oklab::from(LinearRgb::from(Srgb::from_u8(139, 69, 19)));
        let (idx, _) = palette.find_nearest(brown);
        assert_eq!(
            idx, 2,
            "REGRESSION (TEST-04): Brown (139,69,19) should map to red (index 2), got index {}",
            idx
        );
    }

    /// TEST-04: Dark chromatic colors map to their chromatic palette entry, not black.
    ///
    /// If this breaks, it means: the lightness weight (kl) is dominating the
    /// distance metric, causing dark chromatic colors to collapse to black
    /// instead of their correct chromatic match.
    #[test]
    fn test_dark_chromatic_maps_correctly() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Dark red should map to red, not black
        let dark_red = Oklab::from(LinearRgb::from(Srgb::from_u8(139, 0, 0)));
        let (idx, _) = palette.find_nearest(dark_red);
        assert_eq!(
            idx, 2,
            "REGRESSION (TEST-04): Dark red (139,0,0) should map to red (idx 2), got {}",
            idx
        );

        // Dark blue should map to blue, not black
        let dark_blue = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 139)));
        let (idx, _) = palette.find_nearest(dark_blue);
        assert_eq!(
            idx, 4,
            "REGRESSION (TEST-04): Dark blue (0,0,139) should map to blue (idx 4), got {}",
            idx
        );

        // Navy should map to blue, not black
        let navy = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 128)));
        let (idx, _) = palette.find_nearest(navy);
        assert_eq!(
            idx, 4,
            "REGRESSION (TEST-04): Navy (0,0,128) should map to blue (idx 4), got {}",
            idx
        );
    }

    /// TEST-04: Skin tone dithering produces warm chromatic pixels.
    ///
    /// Medium skin tone (210,161,109) maps to white in find_nearest (similar
    /// to pastels), but error diffusion should produce warm-toned output.
    #[test]
    fn test_skin_tone_dithering_produces_warm_pixels() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let skin = Srgb::from_u8(210, 161, 109);
        let image = vec![skin; 32 * 32];

        let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo)
            .saturation(1.0)
            .contrast(1.0);
        let result = ditherer.dither(&image, 32, 32);
        let indices = result.indices();

        // Should contain warm chromatic pixels (red=2 or yellow=5)
        let warm_count = indices.iter().filter(|&&idx| idx == 2 || idx == 5).count();
        assert!(
            warm_count > 0,
            "REGRESSION (TEST-04): Skin tone (210,161,109) dithered with no warm chromatic pixels. \
             Error diffusion should produce some red/yellow pixels."
        );

        // Warm pixels should outnumber cold pixels (green=3, blue=4)
        let cold_count = indices.iter().filter(|&&idx| idx == 3 || idx == 4).count();
        assert!(
            warm_count > cold_count,
            "REGRESSION (TEST-04): Skin tone produced {} warm vs {} cold chromatic pixels. \
             Warm input should produce more warm than cold output.",
            warm_count,
            cold_count
        );
    }

    /// TEST-04: Dark green mapping (flagged for investigation).
    ///
    /// Dark green (0,100,0) has chroma=0.148. Research found it might map to
    /// yellow due to combined lightness and chroma distances. This test
    /// documents the actual behavior.
    #[test]
    fn test_dark_green_maps_to_green_or_yellow() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let dark_green = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 100, 0)));
        let (idx, _) = palette.find_nearest(dark_green);

        // Dark green should map to green (3) or possibly yellow (5) -- both are
        // acceptable chromatic mappings. It must NOT map to black (0) or white (1).
        assert!(
            idx == 3 || idx == 5,
            "REGRESSION (TEST-04): Dark green (0,100,0) should map to green (3) or yellow (5), \
             got index {} ({:?})",
            idx,
            palette_colors[idx].to_bytes()
        );
    }
}
