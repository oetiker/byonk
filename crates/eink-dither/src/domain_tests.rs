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
    use crate::preprocess::Oklch;

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

    /// TEST-03: Pastel colors should reproduce with correct average color.
    ///
    /// On a BWRGBY palette, pastels like light pink are muted but still
    /// chromatic. Error diffusion with Euclidean OKLab matching should
    /// produce a mix of palette colors whose average is perceptually close
    /// to the input.
    ///
    /// If this breaks, it means: the palette matching or error diffusion
    /// is producing wrong chromatic averages — either wrong hue or
    /// excessive lightness error.
    #[test]
    fn test_pastel_color_accuracy_in_photo_mode() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(255, 0, 0),     // 2: red
            Srgb::from_u8(0, 255, 0),     // 3: green
            Srgb::from_u8(0, 0, 255),     // 4: blue
            Srgb::from_u8(255, 255, 0),   // 5: yellow
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let light_pink = Srgb::from_u8(255, 182, 193);
        let r = dither_perceptual_accuracy(light_pink, &palette, RenderingIntent::Photo);
        assert!(
            r.delta_e < 0.10,
            "Light pink: DeltaE={:.4} should be <0.10 for color accuracy",
            r.delta_e
        );
    }

    /// TEST-03 extended: Pale blue reproduces with correct average color.
    #[test]
    fn test_pale_blue_color_accuracy_in_photo_mode() {
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
        let r = dither_perceptual_accuracy(pale_blue, &palette, RenderingIntent::Photo);
        assert!(
            r.delta_e < 0.10,
            "Pale blue: DeltaE={:.4} should be <0.10 for color accuracy",
            r.delta_e
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

    /// TEST-04: Skin tone reproduces with correct average color.
    ///
    /// Medium skin tone (210,161,109) is muted but chromatic. Error diffusion
    /// should produce an output whose average is perceptually close to the input.
    #[test]
    fn test_skin_tone_color_accuracy_in_photo_mode() {
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
        let r = dither_perceptual_accuracy(skin, &palette, RenderingIntent::Photo);
        assert!(
            r.delta_e < 0.10,
            "Skin tone: DeltaE={:.4} should be <0.10 for color accuracy",
            r.delta_e
        );
    }

    /// Dither a uniform 255x255 block of a single color, then compute the
    /// perceived average of the result by averaging the actual palette
    /// colors in linear RGB (physically correct light mixing). The OKLab
    /// distance between the input color and the perceived average measures
    /// how faithfully the dithering reproduces the original.
    ///
    /// This is the gold-standard test for dithering quality: viewed from a
    /// distance, a dithered block of a uniform color should look like the
    /// original color.
    struct DitherAccuracyResult {
        input_lab: Oklab,
        avg_lab: Oklab,
        delta_e: f32,
        /// Fraction of output pixels that are chromatic (not black or white)
        chromatic_fraction: f32,
        /// Chroma of the averaged output color
        output_chroma: f32,
        /// Count of pixels using each palette entry
        palette_counts: Vec<u32>,
    }

    fn dither_perceptual_accuracy(
        input: Srgb,
        palette: &Palette,
        intent: RenderingIntent,
    ) -> DitherAccuracyResult {
        let image = vec![input; 255 * 255];
        let ditherer = EinkDitherer::new(palette.clone(), intent)
            .saturation(1.0)
            .contrast(1.0);
        let result = ditherer.dither(&image, 255, 255);
        let indices = result.indices();

        // Average the ACTUAL palette colors in linear RGB (correct light mixing)
        let n = indices.len() as f32;
        let mut sum_r = 0.0f32;
        let mut sum_g = 0.0f32;
        let mut sum_b = 0.0f32;
        let mut chromatic_count = 0u32;
        let mut palette_counts = vec![0u32; palette.len()];
        for &idx in indices {
            let lin = palette.actual_linear(idx as usize);
            sum_r += lin.r;
            sum_g += lin.g;
            sum_b += lin.b;
            palette_counts[idx as usize] += 1;
            // Indices 0 (black) and 1 (white) are achromatic
            if idx > 1 {
                chromatic_count += 1;
            }
        }
        let avg_linear = LinearRgb::new(sum_r / n, sum_g / n, sum_b / n);
        let avg_oklab = Oklab::from(avg_linear);
        let input_oklab = Oklab::from(LinearRgb::from(input));

        // DeltaE in OKLab: Euclidean distance (not squared)
        let dl = input_oklab.l - avg_oklab.l;
        let da = input_oklab.a - avg_oklab.a;
        let db = input_oklab.b - avg_oklab.b;
        let delta_e = (dl * dl + da * da + db * db).sqrt();

        let output_chroma = (avg_oklab.a * avg_oklab.a + avg_oklab.b * avg_oklab.b).sqrt();

        DitherAccuracyResult {
            input_lab: input_oklab,
            avg_lab: avg_oklab,
            delta_e,
            chromatic_fraction: chromatic_count as f32 / n,
            output_chroma,
            palette_counts,
        }
    }

    /// Perceptual accuracy: dithered uniform blocks should average back
    /// to the original color. Tests a range of achromatic, chromatic, and
    /// muted real-world colors against the 6-color BWRGBY palette.
    ///
    /// Checks BOTH overall DeltaE AND chroma preservation. A dithered
    /// color block that comes back as greyscale when the input was
    /// chromatic is a failure even if the lightness is correct.
    ///
    /// If this breaks, it means: the dithering pipeline is losing color
    /// information — either error diffusion isn't propagating chroma
    /// correctly, or the distance metric is forcing pixels to wrong
    /// palette entries.
    #[test]
    fn test_dither_perceptual_accuracy_photo() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Test colors from actual photo samples + palette primaries.
        // max_delta_e: acceptable OKLab Euclidean distance between input and
        //   perceived (linear-averaged) dithered output.
        // min_chromatic_pct: minimum % of output pixels that must be chromatic
        //   (palette indices > 1). For achromatic inputs this is 0.0.
        //   For chromatic inputs this catches the "looks grey" failure mode.
        //
        // Photo colors sampled from real camera shots — these are the muted
        // tones that pop-art if damping doesn't work correctly.
        // Thresholds set ~30% above measured values to catch regressions.
        let test_colors: &[(&str, Srgb, f32, f32)] = &[
            //                                           max_de  min_chr%
            // Achromatic — no chromatic pixels expected
            ("mid grey", Srgb::from_u8(128, 128, 128), 0.06, 0.0),
            ("dark grey", Srgb::from_u8(64, 64, 64), 0.06, 0.0),
            ("light grey", Srgb::from_u8(192, 192, 192), 0.08, 0.0),
            // Exact palette entries — 100% chromatic
            ("pure red", Srgb::from_u8(255, 0, 0), 0.01, 95.0),
            ("pure green", Srgb::from_u8(0, 255, 0), 0.01, 95.0),
            ("pure blue", Srgb::from_u8(0, 0, 255), 0.01, 95.0),
            // Secondary / mixed saturated colors — should be heavily chromatic
            ("cyan", Srgb::from_u8(0, 255, 255), 0.30, 50.0),
            ("magenta", Srgb::from_u8(255, 0, 255), 0.40, 50.0),
            ("orange", Srgb::from_u8(255, 165, 0), 0.04, 50.0),
            // Real photo colors — sampled from outdoor portrait (overcast sky,
            // skin tones, muted clothing). These are the colors that cause
            // pop-art blowout if chromatic damping isn't working.
            // OKLab chroma for all of these is 0.01-0.06 — well below the
            // 0.12 damping threshold, so they should dither mostly to B&W.
            ("overcast sky", Srgb::from_u8(175, 198, 230), 0.10, 0.0),
            ("sky left", Srgb::from_u8(168, 192, 227), 0.10, 0.0),
            ("skin light", Srgb::from_u8(163, 171, 197), 0.10, 0.0),
            ("skin cheek", Srgb::from_u8(147, 144, 163), 0.08, 0.0),
            ("skin dark", Srgb::from_u8(105, 76, 86), 0.08, 0.0),
            ("skin warm", Srgb::from_u8(137, 102, 102), 0.08, 0.0),
            ("dark hair", Srgb::from_u8(107, 99, 107), 0.05, 0.0),
            ("muted scarf", Srgb::from_u8(140, 108, 104), 0.08, 0.0),
            ("dark clothing", Srgb::from_u8(150, 124, 133), 0.08, 0.0),
            ("blue shirt", Srgb::from_u8(127, 112, 121), 0.06, 0.0),
            ("glasses", Srgb::from_u8(161, 161, 172), 0.06, 0.0),
        ];

        let mut failures = Vec::new();
        for &(name, color, max_delta, min_chromatic_pct) in test_colors {
            let r = dither_perceptual_accuracy(color, &palette, RenderingIntent::Photo);
            let chromatic_pct = r.chromatic_fraction * 100.0;
            if r.delta_e > max_delta {
                failures.push(format!(
                    "  {name}: DeltaE={:.4} (max {max_delta:.2}) chromatic={chromatic_pct:.1}% \
                     input L={:.3} a={:.3} b={:.3}, avg L={:.3} a={:.3} b={:.3}",
                    r.delta_e,
                    r.input_lab.l,
                    r.input_lab.a,
                    r.input_lab.b,
                    r.avg_lab.l,
                    r.avg_lab.a,
                    r.avg_lab.b,
                ));
            }
            if chromatic_pct < min_chromatic_pct {
                failures.push(format!(
                    "  {name}: chromatic={chromatic_pct:.1}% (min {min_chromatic_pct:.0}%) — \
                     colored input dithered to mostly B&W! \
                     input chroma={:.4}, output chroma={:.4}",
                    (r.input_lab.a * r.input_lab.a + r.input_lab.b * r.input_lab.b).sqrt(),
                    r.output_chroma,
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "Perceptual accuracy failures (Photo intent):\n{}",
            failures.join("\n")
        );
    }

    /// Low-saturation photo colors must dither with good perceptual accuracy.
    /// These muted colors (shadows, overcast sky, concrete, foliage) are
    /// typical of real photographs. Error diffusion with unbiased Euclidean
    /// OKLab matching should reproduce them faithfully — the dithered average
    /// should be close to the input in perceptual terms.
    #[test]
    fn test_photo_muted_color_accuracy() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Real-world photo colors with low saturation. Error diffusion with
        // Euclidean OKLab matching produces correct perceptual averages for
        // these muted colors — chromatic palette entries are used in the
        // right proportions to match the input hue and chroma.
        let test_colors: &[(&str, Srgb)] = &[
            ("warm shadow", Srgb::from_u8(80, 70, 60)),
            ("cool shadow", Srgb::from_u8(60, 65, 75)),
            ("overcast sky", Srgb::from_u8(180, 185, 200)),
            ("concrete", Srgb::from_u8(150, 145, 135)),
            ("faded blue", Srgb::from_u8(130, 140, 160)),
            ("dark leaf", Srgb::from_u8(50, 65, 40)),
            ("sunset glow", Srgb::from_u8(220, 200, 170)),
        ];

        let mut failures = Vec::new();
        for &(name, color) in test_colors {
            let r = dither_perceptual_accuracy(color, &palette, RenderingIntent::Photo);
            // Muted colors should reproduce with DeltaE < 0.10.
            // Error diffusion naturally converges to the correct average
            // when palette matching is unbiased (Euclidean OKLab).
            if r.delta_e >= 0.10 {
                failures.push(format!(
                    "  {name}: DeltaE={:.4} (should be <0.10)",
                    r.delta_e,
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "Muted color accuracy failures (Photo intent):\n{}",
            failures.join("\n")
        );
    }

    /// Parameter sweep: dither uniform 255x255 blocks with varying chroma_clamp,
    /// measuring lightness error, chroma error, and per-palette-entry pixel counts.
    /// Run with `cargo test -p eink-dither sweep_dither_params -- --nocapture --ignored`
    #[test]
    #[ignore] // expensive diagnostic — run manually
    fn sweep_dither_params() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // Comprehensive test colors covering the full range found in real photos.
        let test_colors: &[(&str, Srgb)] = &[
            // === Pure greys (must be 100% B&W) ===
            ("grey 25%", Srgb::from_u8(64, 64, 64)),
            ("grey 50%", Srgb::from_u8(128, 128, 128)),
            ("grey 75%", Srgb::from_u8(192, 192, 192)),
            // === Near-grey: very subtle tints (should be ~95%+ B&W) ===
            ("warm shad dk", Srgb::from_u8(50, 45, 40)),
            ("warm shadow", Srgb::from_u8(80, 70, 60)),
            ("cool shad dk", Srgb::from_u8(40, 43, 50)),
            ("cool shadow", Srgb::from_u8(60, 65, 75)),
            ("warm mid", Srgb::from_u8(140, 135, 125)),
            ("cool mid", Srgb::from_u8(125, 130, 140)),
            ("warm light", Srgb::from_u8(200, 195, 185)),
            ("cool light", Srgb::from_u8(185, 190, 200)),
            // === Low chroma: noticeable tint but still muted ===
            ("dusk sky", Srgb::from_u8(80, 80, 120)),
            ("overcast", Srgb::from_u8(170, 175, 190)),
            ("concrete", Srgb::from_u8(150, 145, 135)),
            ("sand", Srgb::from_u8(180, 170, 145)),
            ("dark foliage", Srgb::from_u8(50, 65, 40)),
            ("faded denim", Srgb::from_u8(100, 110, 135)),
            ("clay", Srgb::from_u8(160, 130, 100)),
            ("slate", Srgb::from_u8(100, 110, 120)),
            // === Medium chroma: clearly colored ===
            ("skin tone", Srgb::from_u8(210, 161, 109)),
            ("dusty rose", Srgb::from_u8(160, 120, 130)),
            ("olive", Srgb::from_u8(120, 120, 60)),
            ("teal", Srgb::from_u8(60, 130, 120)),
            ("muted red", Srgb::from_u8(180, 80, 70)),
            ("sage green", Srgb::from_u8(130, 160, 120)),
            ("terracotta", Srgb::from_u8(180, 120, 80)),
            ("steel blue", Srgb::from_u8(70, 100, 150)),
            // === High chroma: saturated colors ===
            ("dark green", Srgb::from_u8(0, 100, 0)),
            ("pure red", Srgb::from_u8(255, 0, 0)),
            ("orange", Srgb::from_u8(255, 165, 0)),
            ("sky blue", Srgb::from_u8(50, 130, 230)),
        ];

        // Sweep damping thresholds (OKLab chroma units).
        // chroma_clamp controls how aggressively muted colors are pushed
        // toward B&W in error diffusion.
        let configs: &[(&str, f32, f32)] = &[
            // (label, kchroma, chroma_clamp)
            ("kc5 none", 5.0, f32::INFINITY),
            ("kc5 cc=0.08", 5.0, 0.08),
            ("kc5 cc=0.12", 5.0, 0.12),
            ("kc5 cc=0.18", 5.0, 0.18),
        ];

        eprintln!();
        eprintln!(
            "{:>14} | {:>9} |  dL    dC    dE  | Blk%  Wht%  Red%  Grn%  Blu%  Yel%",
            "", "config",
        );
        eprintln!("{}", "-".repeat(94));

        for &(label, kchroma, cc) in configs {
            let photo_palette =
                palette
                    .clone()
                    .with_distance_metric(crate::palette::DistanceMetric::HyAB {
                        kl: 2.0,
                        kc: 1.0,
                        kchroma,
                    });
            let options = DitherOptions::new().chroma_clamp(cc);
            let cc_label = label;

            for &(name, color) in test_colors {
                let image = vec![LinearRgb::from(color); 255 * 255];
                let indices = Atkinson.dither(&image, 255, 255, &photo_palette, &options);

                // Average in linear RGB + per-entry counts
                let n = indices.len() as f32;
                let mut sr = 0.0f32;
                let mut sg = 0.0f32;
                let mut sb = 0.0f32;
                let mut counts = [0u32; 6]; // B, W, R, G, Bl, Y
                for &idx in &indices {
                    let lin = palette.actual_linear(idx as usize);
                    sr += lin.r;
                    sg += lin.g;
                    sb += lin.b;
                    counts[idx as usize] += 1;
                }
                let avg = Oklab::from(LinearRgb::new(sr / n, sg / n, sb / n));
                let inp = Oklab::from(LinearRgb::from(color));

                let dl = (inp.l - avg.l).abs();
                let in_c = (inp.a * inp.a + inp.b * inp.b).sqrt();
                let out_c = (avg.a * avg.a + avg.b * avg.b).sqrt();
                let dc = (in_c - out_c).abs();
                let de =
                    ((inp.l - avg.l).powi(2) + (inp.a - avg.a).powi(2) + (inp.b - avg.b).powi(2))
                        .sqrt();
                let p: Vec<f32> = counts.iter().map(|&c| c as f32 / n * 100.0).collect();

                eprintln!(
                    "{name:>14} | {cc_label:>9} | {dl:.3} {dc:.3} {de:.3} | \
                     {:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                    p[0], p[1], p[2], p[3], p[4], p[5],
                );
            }
            eprintln!("{}", "-".repeat(94));
        }
    }

    /// Blue noise grey safety: dithering a grey gradient with the Graphics
    /// intent (blue noise) on a chromatic palette must produce ONLY black and
    /// white output. The blue noise ditherer uses find_second_nearest which
    /// was historically vulnerable to grey→yellow contamination.
    #[test]
    fn test_blue_noise_grey_gradient_chromatic_palette() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // 256-pixel wide grey gradient
        let image: Vec<Srgb> = (0..=255u8).map(|g| Srgb::from_u8(g, g, g)).collect();
        let ditherer = EinkDitherer::new(palette, RenderingIntent::Graphics);
        let result = ditherer.dither(&image, 256, 1);

        for (i, &idx) in result.indices().iter().enumerate() {
            assert!(
                idx == 0 || idx == 1,
                "Grey gradient pixel {} (grey={}) mapped to chromatic index {} via blue noise. \
                 find_second_nearest is leaking chromatic entries for grey pixels.",
                i,
                i,
                idx
            );
        }
    }

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

    // ========================================================================
    // Comprehensive color accuracy sweep (4096-point Oklch grid)
    // ========================================================================

    /// Generate a grid of test colors in Oklch space, filtering out-of-gamut.
    ///
    /// Produces up to 4096 candidate colors (16 L × 16 C × 16 H). Colors
    /// whose Oklch→Oklab→LinearRgb conversion falls outside sRGB [0,1] are
    /// skipped. Returns (label, Srgb) pairs for the ~2000–3000 in-gamut colors.
    fn generate_oklch_grid() -> Vec<(String, Srgb)> {
        let l_steps = 16;
        let c_steps = 16;
        let h_steps = 16;

        // L: 0.05 to 0.95 (avoid exact black/white — they're palette entries)
        // C: 0.0 to 0.37 (palette primaries ≈ 0.25–0.35)
        // H: 0 to 2π (full hue circle)
        let mut colors = Vec::with_capacity(l_steps * c_steps * h_steps);

        for li in 0..l_steps {
            let l = 0.05 + (li as f32 / (l_steps - 1) as f32) * 0.90;
            for ci in 0..c_steps {
                let c = ci as f32 / (c_steps - 1) as f32 * 0.37;
                for hi in 0..h_steps {
                    let h = hi as f32 / h_steps as f32 * std::f32::consts::TAU;
                    let oklch = Oklch { l, c, h };
                    let oklab = Oklab::from(oklch);
                    let linear = LinearRgb::from(oklab);

                    // Skip out-of-gamut
                    if linear.r < 0.0
                        || linear.r > 1.0
                        || linear.g < 0.0
                        || linear.g > 1.0
                        || linear.b < 0.0
                        || linear.b > 1.0
                    {
                        continue;
                    }

                    let srgb = Srgb::from(linear);
                    let h_deg = h.to_degrees();
                    let label = format!("L{l:.2}_C{c:.3}_H{h_deg:.0}");
                    colors.push((label, srgb));
                }
            }
        }

        colors
    }

    /// Comprehensive color accuracy sweep: dither 256×256 uniform blocks for
    /// ~2500 Oklch grid colors and report perceptual accuracy.
    ///
    /// Run: `cargo test -p eink-dither color_accuracy_sweep_photo -- --nocapture --ignored`
    #[test]
    #[ignore] // expensive diagnostic — run manually
    fn test_color_accuracy_sweep_photo() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();
        let grid = generate_oklch_grid();

        eprintln!();
        eprintln!(
            "Photo Intent (Error Diffusion) — {} in-gamut colors",
            grid.len()
        );
        eprintln!(
            "{:>22} | In_L  In_C  In_H° | Avg_L Avg_C  |  dE   | Chr% | Blk%  Wht%  Red%  Grn%  Blu%  Yel%",
            "Label"
        );
        eprintln!("{}", "-".repeat(110));

        let mut total_de = 0.0f64;
        let mut max_de = 0.0f32;
        let mut max_de_label = String::new();

        for (label, color) in &grid {
            let r = dither_perceptual_accuracy(*color, &palette, RenderingIntent::Photo);
            let in_lch = Oklch::from(r.input_lab);
            let avg_lch = Oklch::from(r.avg_lab);
            let chromatic_pct = r.chromatic_fraction * 100.0;
            let n = r.palette_counts.iter().sum::<u32>() as f32;
            let pcts: Vec<f32> = r
                .palette_counts
                .iter()
                .map(|&c| c as f32 / n * 100.0)
                .collect();

            total_de += r.delta_e as f64;
            if r.delta_e > max_de {
                max_de = r.delta_e;
                max_de_label = label.clone();
            }

            eprintln!(
                "{label:>22} | {:.2} {:.3} {:>5.0} | {:.2} {:.3}  | {:.3} | {:>4.1} | {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1}",
                in_lch.l, in_lch.c, in_lch.h.to_degrees(),
                avg_lch.l, avg_lch.c,
                r.delta_e,
                chromatic_pct,
                pcts.get(0).unwrap_or(&0.0),
                pcts.get(1).unwrap_or(&0.0),
                pcts.get(2).unwrap_or(&0.0),
                pcts.get(3).unwrap_or(&0.0),
                pcts.get(4).unwrap_or(&0.0),
                pcts.get(5).unwrap_or(&0.0),
            );
        }

        let avg_de = total_de / grid.len() as f64;
        eprintln!("{}", "-".repeat(110));
        eprintln!("Summary: avg DeltaE={avg_de:.4}, max DeltaE={max_de:.4} ({max_de_label})");
    }

    /// Comprehensive color accuracy sweep for Graphics (blue noise) intent.
    ///
    /// Run: `cargo test -p eink-dither color_accuracy_sweep_graphics -- --nocapture --ignored`
    #[test]
    #[ignore] // expensive diagnostic — run manually
    fn test_color_accuracy_sweep_graphics() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();
        let grid = generate_oklch_grid();

        eprintln!();
        eprintln!(
            "Graphics Intent (Blue Noise) — {} in-gamut colors",
            grid.len()
        );
        eprintln!(
            "{:>22} | In_L  In_C  In_H° | Avg_L Avg_C  |  dE   | Chr% | Blk%  Wht%  Red%  Grn%  Blu%  Yel%",
            "Label"
        );
        eprintln!("{}", "-".repeat(110));

        let mut total_de = 0.0f64;
        let mut max_de = 0.0f32;
        let mut max_de_label = String::new();

        for (label, color) in &grid {
            let r = dither_perceptual_accuracy(*color, &palette, RenderingIntent::Graphics);
            let in_lch = Oklch::from(r.input_lab);
            let avg_lch = Oklch::from(r.avg_lab);
            let chromatic_pct = r.chromatic_fraction * 100.0;
            let n = r.palette_counts.iter().sum::<u32>() as f32;
            let pcts: Vec<f32> = r
                .palette_counts
                .iter()
                .map(|&c| c as f32 / n * 100.0)
                .collect();

            total_de += r.delta_e as f64;
            if r.delta_e > max_de {
                max_de = r.delta_e;
                max_de_label = label.clone();
            }

            eprintln!(
                "{label:>22} | {:.2} {:.3} {:>5.0} | {:.2} {:.3}  | {:.3} | {:>4.1} | {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1}",
                in_lch.l, in_lch.c, in_lch.h.to_degrees(),
                avg_lch.l, avg_lch.c,
                r.delta_e,
                chromatic_pct,
                pcts.get(0).unwrap_or(&0.0),
                pcts.get(1).unwrap_or(&0.0),
                pcts.get(2).unwrap_or(&0.0),
                pcts.get(3).unwrap_or(&0.0),
                pcts.get(4).unwrap_or(&0.0),
                pcts.get(5).unwrap_or(&0.0),
            );
        }

        let avg_de = total_de / grid.len() as f64;
        eprintln!("{}", "-".repeat(110));
        eprintln!("Summary: avg DeltaE={avg_de:.4}, max DeltaE={max_de:.4} ({max_de_label})");
    }
}
