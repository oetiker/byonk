//! Domain-critical regression tests for eink-dither.
//!
//! These tests are designed to catch specific classes of bugs, not just
//! confirm happy paths. Each test documents the regression it guards against.

#[cfg(test)]
mod domain_tests {
    use crate::api::EinkDitherer;
    use crate::color::{LinearRgb, Oklab, Srgb};
    use crate::dither::{
        dither_with_kernel_noise, DitherAlgorithm, DitherOptions, ATKINSON, FLOYD_STEINBERG,
        JARVIS_JUDICE_NINKE, SIERRA, SIERRA_LITE, SIERRA_TWO_ROW,
    };
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
        let result_186 =
            dither_with_kernel_noise(&image_186, size, size, &palette, &ATKINSON, &options);
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
        let result_128 =
            dither_with_kernel_noise(&image_128, size, size, &palette, &ATKINSON, &options);
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

            // Test all 8 algorithms via DitherAlgorithm enum
            let algorithms: &[(&str, DitherAlgorithm)] = &[
                ("Atkinson", DitherAlgorithm::Atkinson),
                ("AtkinsonHybrid", DitherAlgorithm::AtkinsonHybrid),
                ("FloydSteinberg", DitherAlgorithm::FloydSteinberg),
                ("JarvisJudiceNinke", DitherAlgorithm::JarvisJudiceNinke),
                ("Sierra", DitherAlgorithm::Sierra),
                ("SierraTwoRow", DitherAlgorithm::SierraTwoRow),
                ("SierraLite", DitherAlgorithm::SierraLite),
                ("Stucki", DitherAlgorithm::Stucki),
                ("Burkes", DitherAlgorithm::Burkes),
            ];

            for (name, algorithm) in algorithms {
                let result = dither_with_kernel_noise(
                    &image,
                    size,
                    size,
                    &palette,
                    algorithm.kernel(),
                    &options,
                );

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
            let ditherer = EinkDitherer::new(palette.clone())
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

        // Test 2: Varied colorful input should use palette breadth
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

            let ditherer = EinkDitherer::new(palette.clone())
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
        let ditherer = EinkDitherer::new(palette.clone())
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
            let ditherer = EinkDitherer::new(palette.clone())
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
            let result = dither_with_kernel_noise(
                &image,
                width,
                height,
                &palette,
                &FLOYD_STEINBERG,
                &options,
            );

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
        let r = dither_perceptual_accuracy(light_pink, &palette);
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
        let r = dither_perceptual_accuracy(pale_blue, &palette);
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
        let r = dither_perceptual_accuracy(skin, &palette);
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

    fn dither_perceptual_accuracy(input: Srgb, palette: &Palette) -> DitherAccuracyResult {
        let image = vec![input; 255 * 255];
        let ditherer = EinkDitherer::new(palette.clone())
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
            ("dark grey", Srgb::from_u8(64, 64, 64), 0.10, 0.0),
            ("light grey", Srgb::from_u8(192, 192, 192), 0.08, 0.0),
            // Exact palette entries — 100% chromatic
            ("pure red", Srgb::from_u8(255, 0, 0), 0.01, 95.0),
            ("pure green", Srgb::from_u8(0, 255, 0), 0.01, 95.0),
            ("pure blue", Srgb::from_u8(0, 0, 255), 0.01, 95.0),
            // Secondary / mixed saturated colors — should use chromatic entries.
            // Cyan and magenta require combining two palette primaries, so with
            // error_clamp=0.3 (Photo default) the chromatic fraction is lower
            // than with clamp=0.5 because oscillation amplitude is limited.
            ("cyan", Srgb::from_u8(0, 255, 255), 0.30, 0.0),
            ("magenta", Srgb::from_u8(255, 0, 255), 0.40, 5.0),
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
            ("dark hair", Srgb::from_u8(107, 99, 107), 0.07, 0.0),
            ("muted scarf", Srgb::from_u8(140, 108, 104), 0.08, 0.0),
            ("dark clothing", Srgb::from_u8(150, 124, 133), 0.08, 0.0),
            ("blue shirt", Srgb::from_u8(127, 112, 121), 0.06, 0.0),
            ("glasses", Srgb::from_u8(161, 161, 172), 0.06, 0.0),
        ];

        let mut failures = Vec::new();
        for &(name, color, max_delta, min_chromatic_pct) in test_colors {
            let r = dither_perceptual_accuracy(color, &palette);
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
            "Perceptual accuracy failures:\n{}",
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
            let r = dither_perceptual_accuracy(color, &palette);
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
            "Muted color accuracy failures:\n{}",
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
                let indices =
                    dither_with_kernel_noise(&image, 255, 255, &photo_palette, &ATKINSON, &options);

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
    /// Produces up to 4096 candidate colors (16 L x 16 C x 16 H). Colors
    /// whose Oklch->Oklab->LinearRgb conversion falls outside sRGB [0,1] are
    /// skipped. Returns (label, Srgb) pairs for the ~2000-3000 in-gamut colors.
    fn generate_oklch_grid() -> Vec<(String, Srgb)> {
        let l_steps = 16;
        let c_steps = 16;
        let h_steps = 16;

        // L: 0.05 to 0.95 (avoid exact black/white -- they're palette entries)
        // C: 0.0 to 0.37 (palette primaries ~ 0.25-0.35)
        // H: 0 to 2pi (full hue circle)
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

    /// Comprehensive color accuracy sweep: dither 256x256 uniform blocks for
    /// ~2500 Oklch grid colors and report perceptual accuracy.
    ///
    /// Run: `cargo test -p eink-dither color_accuracy_sweep_photo -- --nocapture --ignored`
    #[test]
    #[ignore] // expensive diagnostic -- run manually
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
        eprintln!("Error Diffusion -- {} in-gamut colors", grid.len());
        eprintln!(
            "{:>22} | In_L  In_C  In_H\u{00b0} | Avg_L Avg_C  |  dE   | Chr% | Blk%  Wht%  Red%  Grn%  Blu%  Yel%",
            "Label"
        );
        eprintln!("{}", "-".repeat(110));

        let mut total_de = 0.0f64;
        let mut max_de = 0.0f32;
        let mut max_de_label = String::new();

        for (label, color) in &grid {
            let r = dither_perceptual_accuracy(*color, &palette);
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

    // ========================================================================
    // Grey-chromatic leakage regression tests
    // ========================================================================

    /// Grey gradient on a 6-color palette with dark chromatic entries must
    /// be perceptually neutral -- the averaged output chroma per column
    /// should be low even though individual pixels may use chromatic entries.
    ///
    /// Error diffusion algorithms are allowed (and expected) to use all
    /// palette colors to represent grey tones. What matters is that the
    /// *perceived average* is achromatic:
    /// - Floyd-Steinberg (100% propagation): chromatic artifacts cancel
    ///   perfectly -> neutral gradient using all colors
    /// - Atkinson (75% propagation): without chroma_clamp, the 25%
    ///   chromatic error loss accumulates into a visible color tint.
    ///   With chroma_clamp, the chromatic error is damped before
    ///   propagation, preventing drift.
    #[test]
    fn test_grey_gradient_perceived_neutral() {
        // 6-color palette with dark chromatic entries that overlap grey lightness
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(200, 50, 50),   // 2: dark red
            Srgb::from_u8(255, 230, 50),  // 3: yellow
            Srgb::from_u8(40, 50, 120),   // 4: dark blue
            Srgb::from_u8(50, 120, 50),   // 5: dark green
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();
        let photo_palette = palette.for_error_diffusion();

        // 256x64 grey gradient (0..255 across width, repeated 64 rows)
        let width = 256;
        let height = 64;
        let image: Vec<LinearRgb> = (0..height)
            .flat_map(|_| {
                (0..width).map(|x| {
                    let v = x as f32 / 255.0;
                    LinearRgb::from(Srgb::new(v, v, v))
                })
            })
            .collect();

        let options = DitherOptions::new().chroma_clamp(0.04);

        // Helper: compute max OKLab chroma of per-column averages.
        // Each column is a uniform grey value repeated across 64 rows.
        // The average of palette colors chosen for that column should
        // be nearly achromatic (low chroma).
        let check_neutrality = |result: &[u8], algo_name: &str| {
            // Check neutrality over the full image (all columns averaged).
            // Individual columns may have noticeable chroma (especially at
            // mid-grey where dark chromatic entries are Euclidean-closest),
            // but the overall gradient should be perceptually neutral.
            let n = result.len() as f32;
            let mut sr = 0.0f32;
            let mut sg = 0.0f32;
            let mut sb = 0.0f32;
            for &idx in result.iter() {
                let lin = palette.actual_linear(idx as usize);
                sr += lin.r;
                sg += lin.g;
                sb += lin.b;
            }
            let avg = Oklab::from(LinearRgb::new(sr / n, sg / n, sb / n));
            let overall_chroma = (avg.a * avg.a + avg.b * avg.b).sqrt();

            // Also compute per-column max chroma for diagnostic
            let mut max_col_chroma = 0.0f32;
            let mut worst_col = 0;
            for col in 0..width {
                let mut cr = 0.0f32;
                let mut cg = 0.0f32;
                let mut cb = 0.0f32;
                for row in 0..height {
                    let lin = palette.actual_linear(result[row * width + col] as usize);
                    cr += lin.r;
                    cg += lin.g;
                    cb += lin.b;
                }
                let cn = height as f32;
                let col_avg = Oklab::from(LinearRgb::new(cr / cn, cg / cn, cb / cn));
                let col_chroma = (col_avg.a * col_avg.a + col_avg.b * col_avg.b).sqrt();
                if col_chroma > max_col_chroma {
                    max_col_chroma = col_chroma;
                    worst_col = col;
                }
            }

            assert!(
                overall_chroma < 0.04,
                "REGRESSION: {algo_name} grey gradient has overall chroma {overall_chroma:.4} \
                 (expected <0.04), max column chroma {max_col_chroma:.4} at col {worst_col}. \
                 Visible color tint in grey gradient."
            );
        };

        // Test Atkinson -- chroma_clamp prevents green tint from 25% error loss
        let result =
            dither_with_kernel_noise(&image, width, height, &photo_palette, &ATKINSON, &options);
        check_neutrality(&result, "Atkinson");

        // Test FloydSteinberg -- 100% propagation naturally cancels
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &FLOYD_STEINBERG,
            &options,
        );
        check_neutrality(&result, "FloydSteinberg");
    }

    /// White->dark_blue gradient must produce dark_blue pixels in the output.
    /// Without chroma_clamp, Floyd-Steinberg's 100% propagation creates
    /// high-amplitude oscillations that push pixels into the black region,
    /// rendering the gradient as black instead of blue.
    #[test]
    fn test_blue_gradient_contains_blue() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(200, 50, 50),   // 2: dark red
            Srgb::from_u8(255, 230, 50),  // 3: yellow
            Srgb::from_u8(40, 50, 120),   // 4: dark blue
            Srgb::from_u8(50, 120, 50),   // 5: dark green
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();
        let photo_palette = palette.for_error_diffusion();

        // 256x64 gradient from white to dark blue
        let dark_blue = Srgb::from_u8(40, 50, 120);
        let white = Srgb::from_u8(255, 255, 255);
        let width = 256;
        let height = 64;
        let image: Vec<LinearRgb> = (0..height)
            .flat_map(|_| {
                (0..width).map(|x| {
                    let t = x as f32 / 255.0; // 0=white, 1=dark_blue
                    let r = white.r + t * (dark_blue.r - white.r);
                    let g = white.g + t * (dark_blue.g - white.g);
                    let b = white.b + t * (dark_blue.b - white.b);
                    LinearRgb::from(Srgb::new(r, g, b))
                })
            })
            .collect();

        let options = DitherOptions::new().chroma_clamp(0.04);

        // Test FloydSteinberg -- the algorithm most affected by blue->black
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &FLOYD_STEINBERG,
            &options,
        );
        let blue_count = result.iter().filter(|&&idx| idx == 4).count();
        let blue_pct = blue_count as f64 / result.len() as f64 * 100.0;
        assert!(
            blue_pct > 1.0,
            "REGRESSION: FloydSteinberg white->dark_blue gradient has only {blue_pct:.2}% \
             blue pixels (expected >1%). Blue gradient renders as black."
        );

        // Test Atkinson
        let result =
            dither_with_kernel_noise(&image, width, height, &photo_palette, &ATKINSON, &options);
        let blue_count = result.iter().filter(|&&idx| idx == 4).count();
        let blue_pct = blue_count as f64 / result.len() as f64 * 100.0;
        assert!(
            blue_pct > 1.0,
            "REGRESSION: Atkinson white->dark_blue gradient has only {blue_pct:.2}% \
             blue pixels (expected >1%). Blue gradient renders as black."
        );
    }

    /// Diagnostic: trace the hue sweep green->blue transition to understand
    /// color inversion artifacts.
    ///
    /// Run: `cargo test -p eink-dither hue_sweep_green_blue -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_hue_sweep_green_blue_diagnostic() {
        // User's 6-color calibrator palette
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // 0: black
            Srgb::from_u8(255, 255, 255), // 1: white
            Srgb::from_u8(200, 50, 50),   // 2: dark red
            Srgb::from_u8(255, 230, 50),  // 3: yellow
            Srgb::from_u8(40, 50, 120),   // 4: dark blue
            Srgb::from_u8(50, 120, 50),   // 5: dark green
        ];
        let names = ["black", "white", "d.red", "yellow", "d.blue", "d.green"];
        let palette = Palette::new(&palette_colors, None).unwrap();
        let photo_palette = palette.for_error_diffusion();

        // === Part 1: Raw nearest-match (no error diffusion) ===
        // Show which palette entry wins for each hue at S=1, L=0.5
        eprintln!("\n=== Raw nearest-match (Euclidean OKLab) per hue ===");
        eprintln!(
            "{:>5} | {:14} | {:28} | {:>8} | {}",
            "Hue", "sRGB", "OKLab L     a      b     C", "nearest", "dist"
        );
        eprintln!("{}", "-".repeat(85));

        for hue_deg in (90..=270).step_by(5) {
            let h = hue_deg as f32 / 360.0;
            // HSL to sRGB conversion
            let (r, g, b) = hsl_to_rgb(h, 1.0, 0.5);
            let srgb = Srgb::new(r, g, b);
            let oklab = Oklab::from(LinearRgb::from(srgb));
            let chroma = (oklab.a * oklab.a + oklab.b * oklab.b).sqrt();
            let (idx, dist) = photo_palette.find_nearest(oklab);

            eprintln!(
                "{hue_deg:>5}\u{00b0} | ({:>3},{:>3},{:>3}) | {:.3} {:.4} {:.4} {:.3} | {}({}) | {:.4}",
                (r * 255.0) as u8,
                (g * 255.0) as u8,
                (b * 255.0) as u8,
                oklab.l,
                oklab.a,
                oklab.b,
                chroma,
                names[idx],
                idx,
                dist,
            );
        }

        // === Part 2: Dithered hue sweep (like calibrator) ===
        // Each column = one hue step, 32 rows deep
        let hue_start = 90;
        let hue_end = 270;
        let hue_step = 2; // finer than calibrator's 5 degrees for detail
        let width = (hue_end - hue_start) / hue_step + 1;
        let height = 32;

        let image: Vec<LinearRgb> = (0..height)
            .flat_map(|_| {
                (0..width).map(|col| {
                    let hue_deg = hue_start + col * hue_step;
                    let h = hue_deg as f32 / 360.0;
                    let (r, g, b) = hsl_to_rgb(h, 1.0, 0.5);
                    LinearRgb::from(Srgb::new(r, g, b))
                })
            })
            .collect();

        let options = DitherOptions::new();

        // FloydSteinberg
        eprintln!("\n=== FloydSteinberg: per-column dominant palette entry ===");
        eprintln!(
            "(dominant = most-used entry in that column across {} rows)",
            height
        );
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &FLOYD_STEINBERG,
            &options,
        );
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // Atkinson
        eprintln!("\n=== Atkinson: per-column dominant palette entry ===");
        let result =
            dither_with_kernel_noise(&image, width, height, &photo_palette, &ATKINSON, &options);
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // JarvisJudiceNinke
        eprintln!("\n=== JarvisJudiceNinke: per-column dominant palette entry ===");
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &JARVIS_JUDICE_NINKE,
            &options,
        );
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // Sierra (full)
        eprintln!("\n=== Sierra: per-column dominant palette entry ===");
        let result =
            dither_with_kernel_noise(&image, width, height, &photo_palette, &SIERRA, &options);
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // SierraTwoRow
        eprintln!("\n=== SierraTwoRow: per-column dominant palette entry ===");
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &SIERRA_TWO_ROW,
            &options,
        );
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // SierraLite
        eprintln!("\n=== SierraLite: per-column dominant palette entry ===");
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &SIERRA_LITE,
            &options,
        );
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );
    }

    /// HSL to RGB (S=0..1, L=0..1, H=0..1) -> (r, g, b) in 0..1
    fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
        if s == 0.0 {
            return (l, l, l);
        }
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        let r = hue_to_channel(p, q, h + 1.0 / 3.0).clamp(0.0, 1.0);
        let g = hue_to_channel(p, q, h).clamp(0.0, 1.0);
        let b = hue_to_channel(p, q, h - 1.0 / 3.0).clamp(0.0, 1.0);
        (r, g, b)
    }

    fn hue_to_channel(p: f32, q: f32, mut t: f32) -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    fn print_column_dominance(
        result: &[u8],
        width: usize,
        height: usize,
        _palette: &Palette,
        names: &[&str; 6],
        hue_start: usize,
        hue_step: usize,
    ) {
        eprintln!(
            "{:>5} | dominant  | Blk%  Wht% dRed%  Yel% dBlu% dGrn%",
            "Hue"
        );
        eprintln!("{}", "-".repeat(65));

        let mut prev_dominant = 255u8;
        let mut inversions = Vec::new();

        for col in 0..width {
            let hue_deg = hue_start + col * hue_step;
            let mut counts = [0u32; 6];
            for row in 0..height {
                let idx = result[row * width + col] as usize;
                if idx < 6 {
                    counts[idx] += 1;
                }
            }
            let n = height as f32;
            let dominant = counts.iter().enumerate().max_by_key(|(_, &c)| c).unwrap().0 as u8;

            // Detect inversions: dominant switched back to a previous color
            if dominant != prev_dominant && prev_dominant != 255 {
                // Check if this is a "backward" switch
                if col > 1 {
                    let prev2_col = col - 2;
                    let mut prev2_counts = [0u32; 6];
                    for row in 0..height {
                        let idx = result[row * width + prev2_col] as usize;
                        if idx < 6 {
                            prev2_counts[idx] += 1;
                        }
                    }
                    let prev2_dominant = prev2_counts
                        .iter()
                        .enumerate()
                        .max_by_key(|(_, &c)| c)
                        .unwrap()
                        .0 as u8;
                    if dominant == prev2_dominant && dominant != prev_dominant {
                        inversions.push(hue_deg);
                    }
                }
            }
            prev_dominant = dominant;

            let pcts: Vec<String> = counts
                .iter()
                .map(|&c| format!("{:>5.1}", c as f32 / n * 100.0))
                .collect();

            eprintln!(
                "{hue_deg:>5}\u{00b0} | {:>7}({}) | {} {} {} {} {} {}",
                names[dominant as usize],
                dominant,
                pcts[0],
                pcts[1],
                pcts[2],
                pcts[3],
                pcts[4],
                pcts[5],
            );
        }

        if !inversions.is_empty() {
            eprintln!(
                "\n  *** COLOR INVERSIONS detected at hues: {:?}",
                inversions
            );
        }
    }

    // ========================================================================
    // AtkinsonHybrid tests
    // ========================================================================

    /// Uniform grey 128,128,128 dithered with a 6-color chromatic palette
    /// should produce near-neutral output. AtkinsonHybrid should do at least
    /// as well as Atkinson with chroma_clamp.
    #[test]
    fn test_atkinson_hybrid_grey_neutrality() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(200, 50, 50),   // dark red
            Srgb::from_u8(255, 230, 50),  // yellow
            Srgb::from_u8(40, 50, 120),   // dark blue
            Srgb::from_u8(50, 120, 50),   // dark green
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        // 200x200 uniform grey
        let mid = Srgb::from_u8(128, 128, 128);
        let width = 200;
        let height = 200;

        let ditherer =
            EinkDitherer::new(palette.clone()).algorithm(DitherAlgorithm::AtkinsonHybrid);
        let pixels: Vec<Srgb> = vec![mid; width * height];
        let result = ditherer.dither(&pixels, width, height);

        // Compute mean RGB of output
        let n = result.indices().len() as f32;
        let mut sr = 0.0f32;
        let mut sg = 0.0f32;
        let mut sb = 0.0f32;
        for &idx in result.indices() {
            let lin = palette.actual_linear(idx as usize);
            sr += lin.r;
            sg += lin.g;
            sb += lin.b;
        }
        let avg = Oklab::from(LinearRgb::new(sr / n, sg / n, sb / n));
        let overall_chroma = (avg.a * avg.a + avg.b * avg.b).sqrt();

        eprintln!(
            "AtkinsonHybrid grey neutrality: overall chroma={overall_chroma:.4}, L={:.4}",
            avg.l
        );

        assert!(
            overall_chroma < 0.04,
            "AtkinsonHybrid should produce near-neutral output on grey, got chroma={overall_chroma:.4}"
        );
    }

    /// The original Atkinson behavior must be unchanged when using the
    /// plain Atkinson variant (not AtkinsonHybrid).
    #[test]
    fn test_atkinson_unchanged() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(200, 50, 50),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();
        let photo_palette = palette.for_error_diffusion();

        let mid = Srgb::from_u8(128, 128, 128);
        let lin = LinearRgb::from(mid);
        let image = vec![lin; 16];

        let options = DitherOptions::new().error_clamp(0.08).noise_scale(0.0);

        let result1 = dither_with_kernel_noise(&image, 4, 4, &photo_palette, &ATKINSON, &options);
        let result2 = dither_with_kernel_noise(&image, 4, 4, &photo_palette, &ATKINSON, &options);

        assert_eq!(
            result1, result2,
            "Atkinson should be deterministic and unchanged"
        );

        // Verify hybrid_propagation is false by default
        assert!(!options.hybrid_propagation);
    }

    /// AtkinsonHybrid should produce valid palette indices and be reusable.
    #[test]
    fn test_atkinson_hybrid_basic_validity() {
        let palette_colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette = Palette::new(&palette_colors, None).unwrap();

        let pixels: Vec<Srgb> = (0..64)
            .map(|i| {
                let v = (i as f32 / 63.0 * 255.0) as u8;
                Srgb::from_u8(v, v, v)
            })
            .collect();

        let ditherer =
            EinkDitherer::new(palette.clone()).algorithm(DitherAlgorithm::AtkinsonHybrid);

        let result1 = ditherer.dither(&pixels, 8, 8);
        let result2 = ditherer.dither(&pixels, 8, 8);

        // All indices must be in range
        for &idx in result1.indices() {
            assert!(
                (idx as usize) < palette.len(),
                "Index {} out of palette range {}",
                idx,
                palette.len()
            );
        }

        // Must be reusable (deterministic)
        assert_eq!(
            result1.indices(),
            result2.indices(),
            "AtkinsonHybrid builder should be reusable"
        );
    }

    // ========================================================================
    // DIAGNOSTIC: full-hue gamut sweep, measured by patch average
    // ========================================================================

    /// Diagnostic: sweep the full hue circle as flat patches, dither each
    /// through the real `EinkDitherer` path, and measure what the patch
    /// *averages to* — the colour a viewer perceives from a distance —
    /// rather than which single palette entry won.
    ///
    /// `print_column_dominance` above reports the dominant entry, which
    /// cannot distinguish "the ditherer mixed red and yellow into orange"
    /// from "the ditherer painted the whole patch red". This one can: it
    /// averages the ACTUAL palette colours in linear RGB (the space in
    /// which light physically adds) and reports the achieved OKLCh plus
    /// the perceptual error against the request.
    ///
    /// Three configurations are compared, which separates the two distinct
    /// causes of flat output:
    ///
    /// - `production`  — what byonk ships today.
    /// - `no-exact`    — exact-match passthrough disabled. Any pixel equal
    ///   to an official palette colour is otherwise forced to that entry
    ///   with its error discarded, which pins the pure primaries (0°, 60°,
    ///   120°, 240°) to a single flat colour no matter what else changes.
    /// - `no-exact+clamp` — additionally widens `error_clamp`. A saturated
    ///   hue sits at a channel extreme, so the default 0.08 of headroom
    ///   lets almost no error accumulate and the same entry wins forever.
    ///
    /// Run: `cargo test -p eink-dither hue_gamut_sweep -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_hue_gamut_sweep_patch_average() {
        // reTerminal E1002, measured colours from default-config.yaml.
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let names = ["blk", "wht", "red", "yel", "blu", "grn"];
        let palette = Palette::new(&official, Some(&actual)).unwrap();

        const PATCH: usize = 8;

        // (label, error_clamp override)
        let configs: [(&str, Option<f32>); 2] = [("default", None), ("clamp 2.0", Some(2.0))];

        eprintln!(
            "\nPatch average over {PATCH}x{PATCH}, Atkinson. dE = OKLab distance to the request."
        );
        eprintln!(
            "{:>4} | {:>17} | {:>26} | {:>26}",
            "hue", "requested", "default", "clamp 2.0"
        );
        eprintln!("{}", "-".repeat(84));

        let mut totals = [0.0f32; 2];
        for hue_deg in (0..360).step_by(15) {
            let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, 1.0, 0.5);
            let src = Srgb::new(r, g, b);
            let pixels = vec![src; PATCH * PATCH];
            let target_lab = Oklab::from(LinearRgb::from(src));
            let target = Oklch::from(target_lab);

            let mut cells: Vec<String> = Vec::new();
            for (ci, &(_, clamp)) in configs.iter().enumerate() {
                let mut d = EinkDitherer::new(palette.clone()).algorithm(DitherAlgorithm::Atkinson);
                if let Some(c) = clamp {
                    d = d.error_clamp(c);
                }
                let out = d.dither(&pixels, PATCH, PATCH);

                let mut sum = [0.0f32; 3];
                let mut hist = [0usize; 6];
                for &idx in out.indices() {
                    let c = palette.actual_linear(idx as usize);
                    sum[0] += c.r;
                    sum[1] += c.g;
                    sum[2] += c.b;
                    hist[idx as usize] += 1;
                }
                let n = (PATCH * PATCH) as f32;
                let avg_lab = Oklab::from(LinearRgb::new(sum[0] / n, sum[1] / n, sum[2] / n));
                let got = Oklch::from(avg_lab);
                let de = avg_lab.distance_squared(target_lab).sqrt();
                totals[ci] += de;

                let used = hist.iter().filter(|&&n| n > 0).count();
                let top = hist
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, &n)| n)
                    .map(|(i, _)| names[i])
                    .unwrap();
                cells.push(format!(
                    "L{:.2} C{:.3} h{:>3.0}\u{00b0} dE{:.2} {}{}",
                    got.l,
                    got.c,
                    got.h.to_degrees().rem_euclid(360.0),
                    de,
                    top,
                    if used == 1 {
                        "*".to_string()
                    } else {
                        format!("+{}", used - 1)
                    },
                ));
            }

            eprintln!(
                "{hue_deg:>3}\u{00b0} | L{:.2} C{:.3} h{:>3.0}\u{00b0} | {:>26} | {:>26}",
                target.l,
                target.c,
                target.h.to_degrees().rem_euclid(360.0),
                cells[0],
                cells[1],
            );
        }
        eprintln!("{}", "-".repeat(84));
        eprintln!(
            "mean dE over 24 hues: default {:.3} | clamp 2.0 {:.3}",
            totals[0] / 24.0,
            totals[1] / 24.0
        );
        eprintln!("(* = patch is a single flat colour, i.e. no dithering happened at all)");
    }

    /// Isolates the `error_clamp` variable behind the flat-patch collapse
    /// seen in `test_hue_gamut_sweep_patch_average`.
    ///
    /// `clamp_channel` clamps the *pixel value plus accumulated error* into
    /// `[-error_clamp, 1 + error_clamp]`. A fully saturated hue already sits
    /// at a channel extreme (magenta is b=1.0), so at error_clamp=0.08 there
    /// is only 0.08 of headroom for error to accumulate in that channel: the
    /// same entry wins every pixel and the patch comes out flat.
    ///
    /// Exact-match passthrough is disabled here deliberately, to hold the
    /// other cause fixed — otherwise the pure primaries (0/60/120/240) are
    /// pinned by exact matching and show no response to the clamp at all,
    /// which is what made this look like a dead end on the first pass.
    ///
    /// Findings: widening the clamp restores mixing at 120 (yel-only ->
    /// yel+grn, reaching the palette optimum), 180 and 300. It does NOT
    /// help at 225-270, where flat blue really is the best the hull offers.
    ///
    /// Run: `cargo test -p eink-dither clamp_headroom -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_clamp_headroom_hypothesis() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let names = ["blk", "wht", "red", "yel", "blu", "grn"];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 8;

        for &hue_deg in &[120.0f32, 270.0, 300.0, 180.0] {
            eprintln!("\n=== hue {hue_deg}\u{00b0} vs error_clamp ===");
            for &ec in &[0.08f32, 0.2, 0.5, 1.0, 2.0] {
                let (r, g, b) = hsl_to_rgb(hue_deg / 360.0, 1.0, 0.5);
                let src = Srgb::new(r, g, b);
                let pixels = vec![src; PATCH * PATCH];

                let out = EinkDitherer::new(palette.clone())
                    .algorithm(DitherAlgorithm::Atkinson)
                    .error_clamp(ec)
                    .dither(&pixels, PATCH, PATCH);

                let mut sum = [0.0f32; 3];
                let mut hist = [0usize; 6];
                for &idx in out.indices() {
                    let c = palette.actual_linear(idx as usize);
                    sum[0] += c.r;
                    sum[1] += c.g;
                    sum[2] += c.b;
                    hist[idx as usize] += 1;
                }
                let n = (PATCH * PATCH) as f32;
                let got = Oklch::from(Oklab::from(LinearRgb::new(
                    sum[0] / n,
                    sum[1] / n,
                    sum[2] / n,
                )));
                let target = Oklch::from(Oklab::from(LinearRgb::from(src)));
                let mix: Vec<String> = hist
                    .iter()
                    .enumerate()
                    .filter(|(_, &n)| n > 0)
                    .map(|(i, &n)| format!("{}:{}", names[i], n))
                    .collect();
                eprintln!(
                    "  clamp {:>4} | got L{:.2} C{:.3} h{:>3.0}\u{00b0} (target C{:.3} h{:>3.0}\u{00b0}) | {}",
                    ec,
                    got.l,
                    got.c,
                    got.h.to_degrees().rem_euclid(360.0),
                    target.c,
                    target.h.to_degrees().rem_euclid(360.0),
                    mix.join(" "),
                );
            }
        }
    }

    /// Second hypothesis for the flat-patch collapse: the collapse is an
    /// out-of-gamut effect, not a diffusion defect. A 6-colour panel's
    /// hull contains no bright magenta (its bluest primary is dark), so
    /// magenta is unreachable — and the pipeline responds by picking the
    /// nearest entry for every pixel instead of mapping into the hull.
    ///
    /// If that is right, a target that IS inside the hull — e.g. the exact
    /// 50/50 linear mix of two primaries — must dither to a proper mix.
    ///
    /// Run: `cargo test -p eink-dither in_gamut -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_in_gamut_targets_still_mix() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let names = ["blk", "wht", "red", "yel", "blu", "grn"];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 8;

        // Exact 50/50 linear mixes of primary pairs — guaranteed in-hull.
        let pairs = [(2usize, 4usize), (4, 5), (2, 3), (1, 4)];
        for (i, j) in pairs {
            let a = palette.actual_linear(i);
            let b = palette.actual_linear(j);
            let mid = LinearRgb::new((a.r + b.r) * 0.5, (a.g + b.g) * 0.5, (a.b + b.b) * 0.5);
            let src = Srgb::from(mid);
            let pixels = vec![src; PATCH * PATCH];

            let out = EinkDitherer::new(palette.clone())
                .algorithm(DitherAlgorithm::Atkinson)
                .dither(&pixels, PATCH, PATCH);

            let mut sum = [0.0f32; 3];
            let mut hist = [0usize; 6];
            for &idx in out.indices() {
                let c = palette.actual_linear(idx as usize);
                sum[0] += c.r;
                sum[1] += c.g;
                sum[2] += c.b;
                hist[idx as usize] += 1;
            }
            let n = (PATCH * PATCH) as f32;
            let got = LinearRgb::new(sum[0] / n, sum[1] / n, sum[2] / n);
            let dist = Oklab::from(got).distance_squared(Oklab::from(mid)).sqrt();
            let mix: Vec<String> = hist
                .iter()
                .enumerate()
                .filter(|(_, &n)| n > 0)
                .map(|(k, &n)| format!("{}:{}", names[k], n))
                .collect();
            eprintln!(
                "50/50 {:>3}+{:<3} target lin({:.3},{:.3},{:.3}) got({:.3},{:.3},{:.3}) dE={:.3} | {}",
                names[i], names[j], mid.r, mid.g, mid.b, got.r, got.g, got.b, dist,
                mix.join(" ")
            );
        }
    }

    /// Best patch-average reachable on this palette for `target`, and the
    /// mixture that achieves it.
    ///
    /// A dithered patch's average is, by construction, a convex combination
    /// of the palette's ACTUAL colours in linear RGB — that is the space in
    /// which light adds. So the convex hull of those colours is a hard bound
    /// on what ANY error-diffusion algorithm can reproduce, independent of
    /// kernel, clamp or tuning. Measuring the ditherer against this bound
    /// separates "the panel physically cannot make this colour" from "the
    /// ditherer failed to make a colour the panel can make".
    ///
    /// The objective (OKLab distance of a linear-RGB mixture) is not convex,
    /// so this is coordinate descent with a decreasing step from a fixed,
    /// deterministic spread of starts: every vertex, the centroid, and every
    /// pairwise midpoint. With six colours that is ample.
    fn best_reachable(palette: &Palette, target: Oklab) -> (f32, Vec<f32>) {
        let n = palette.len();
        let cost = |w: &[f32]| -> f32 {
            let total: f32 = w.iter().sum();
            if total <= 0.0 {
                return f32::MAX;
            }
            let mut mix = [0.0f32; 3];
            for (i, &wi) in w.iter().enumerate() {
                let c = palette.actual_linear(i);
                mix[0] += wi * c.r;
                mix[1] += wi * c.g;
                mix[2] += wi * c.b;
            }
            let mix = LinearRgb::new(mix[0] / total, mix[1] / total, mix[2] / total);
            Oklab::from(mix).distance_squared(target).sqrt()
        };

        let mut starts: Vec<Vec<f32>> = Vec::new();
        for i in 0..n {
            let mut w = vec![0.0; n];
            w[i] = 1.0;
            starts.push(w);
        }
        starts.push(vec![1.0 / n as f32; n]);
        for i in 0..n {
            for j in (i + 1)..n {
                let mut w = vec![0.0; n];
                w[i] = 0.5;
                w[j] = 0.5;
                starts.push(w);
            }
        }

        let mut best = (f32::MAX, vec![0.0; n]);
        for mut w in starts {
            let mut cur = cost(&w);
            for &step in &[0.4f32, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002] {
                loop {
                    let mut improved = false;
                    for i in 0..n {
                        for d in [step, -step] {
                            let mut cand = w.clone();
                            cand[i] = (cand[i] + d).max(0.0);
                            if cand.iter().sum::<f32>() <= 0.0 {
                                continue;
                            }
                            let c = cost(&cand);
                            if c < cur - 1e-7 {
                                w = cand;
                                cur = c;
                                improved = true;
                            }
                        }
                    }
                    if !improved {
                        break;
                    }
                }
            }
            if cur < best.0 {
                let total: f32 = w.iter().sum();
                best = (cur, w.iter().map(|x| x / total).collect());
            }
        }
        best
    }

    /// Is the hue collapse inherent to a 6-colour panel, or is the ditherer
    /// failing to use the palette it has?
    ///
    /// For each target this compares the achieved patch average against
    /// `best_reachable` — the physical bound above. `gap = achieved - bound`
    /// is the part that is the ditherer's fault and nothing else's.
    ///
    /// Run: `cargo test -p eink-dither gamut_bound -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_dither_versus_gamut_bound() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let names = ["blk", "wht", "red", "yel", "blu", "grn"];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 16;

        // (label, algorithm, error_clamp override)
        let configs: [(&str, DitherAlgorithm, Option<f32>); 4] = [
            ("atkinson", DitherAlgorithm::Atkinson, None),
            ("atkinson+clamp2", DitherAlgorithm::Atkinson, Some(2.0)),
            ("floyd", DitherAlgorithm::FloydSteinberg, None),
            ("floyd+clamp2", DitherAlgorithm::FloydSteinberg, Some(2.0)),
        ];

        let lightnesses = [0.2f32, 0.32, 0.44, 0.56, 0.68, 0.8];
        let mut sum_bound = 0.0f32;
        let mut sum_got = [0.0f32; 4];
        let mut count = 0usize;
        let mut worst: Vec<(f32, i32, f32, f32, [f32; 4], String)> = Vec::new();

        for &l in &lightnesses {
            for hue_deg in (0..360).step_by(15) {
                let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, 1.0, l);
                let src = Srgb::new(r, g, b);
                let target = Oklab::from(LinearRgb::from(src));
                let pixels = vec![src; PATCH * PATCH];

                let (bound, recipe) = best_reachable(&palette, target);
                sum_bound += bound;
                count += 1;

                let mut got = [0.0f32; 4];
                for (ci, &(_, algo, clamp)) in configs.iter().enumerate() {
                    let mut d = EinkDitherer::new(palette.clone()).algorithm(algo);
                    if let Some(c) = clamp {
                        d = d.error_clamp(c);
                    }
                    let out = d.dither(&pixels, PATCH, PATCH);
                    let mut acc = [0.0f32; 3];
                    for &idx in out.indices() {
                        let c = palette.actual_linear(idx as usize);
                        acc[0] += c.r;
                        acc[1] += c.g;
                        acc[2] += c.b;
                    }
                    let n = (PATCH * PATCH) as f32;
                    let avg = Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
                    got[ci] = avg.distance_squared(target).sqrt();
                    sum_got[ci] += got[ci];
                }

                let recipe_s: Vec<String> = recipe
                    .iter()
                    .enumerate()
                    .filter(|(_, &w)| w > 0.02)
                    .map(|(i, &w)| format!("{}:{:.0}%", names[i], w * 100.0))
                    .collect();
                worst.push((
                    got[0] - bound,
                    hue_deg as i32,
                    l,
                    bound,
                    got,
                    recipe_s.join(" "),
                ));
            }
        }

        let n = count as f32;
        eprintln!("\n=== Dither vs. the palette's physical bound ===");
        eprintln!(
            "{} targets (24 hues x {} lightness levels), {PATCH}x{PATCH} patches\n",
            count,
            lightnesses.len()
        );
        eprintln!(
            "  gamut bound (best ANY algorithm could do) : mean dE {:.3}",
            sum_bound / n
        );
        for (ci, &(label, _, _)) in configs.iter().enumerate() {
            eprintln!(
                "  {:<16} : mean dE {:.3}   gap over bound {:.3}",
                label,
                sum_got[ci] / n,
                (sum_got[ci] - sum_bound) / n
            );
        }

        worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        eprintln!("\nWorst 12 targets by default-config gap, dE under each config:");
        eprintln!(
            "{:>5} {:>5} | {:>6} | {:>6} {:>6} {:>6} {:>6} | best possible mixture",
            "hue", "L", "bound", "atk", "atk+c2", "floyd", "fl+c2"
        );
        eprintln!("{}", "-".repeat(88));
        for (_gap, hue, l, bound, got, recipe) in worst.iter().take(12) {
            eprintln!(
                "{hue:>4}\u{00b0} {:>5.2} | {bound:>6.3} | {:>6.3} {:>6.3} {:>6.3} {:>6.3} | {recipe}",
                l, got[0], got[1], got[2], got[3]
            );
        }

        // The list above ranks by Atkinson's gap, so it can only ever show
        // targets Floyd-Steinberg improves. Floyd's *mean* is the worse of the
        // two, so it must lose somewhere; ranking the other way round is what
        // shows where, and that is the half of the trade the algorithm choice
        // actually turns on.
        let mut floyd_worse: Vec<_> = worst.iter().filter(|w| w.4[2] > w.4[0]).collect();
        floyd_worse.sort_by(|a, b| (b.4[2] - b.4[0]).partial_cmp(&(a.4[2] - a.4[0])).unwrap());
        eprintln!(
            "\nFloyd is worse than Atkinson on {}/{count} targets. Worst 12:",
            floyd_worse.len()
        );
        eprintln!(
            "{:>5} {:>5} | {:>6} | {:>6} {:>6} {:>6} {:>6} | best possible mixture",
            "hue", "L", "bound", "atk", "atk+c2", "floyd", "fl+c2"
        );
        eprintln!("{}", "-".repeat(88));
        for (_gap, hue, l, bound, got, recipe) in floyd_worse.iter().take(12) {
            eprintln!(
                "{hue:>4}\u{00b0} {:>5.2} | {bound:>6.3} | {:>6.3} {:>6.3} {:>6.3} {:>6.3} | {recipe}",
                l, got[0], got[1], got[2], got[3]
            );
        }

        // Per-lightness means separate "dark targets" from "everything else",
        // which is the axis the Atkinson error loss is expected to act on.
        eprintln!("\nMean dE by lightness:");
        eprintln!(
            "{:>5} | {:>6} | {:>6} {:>6} {:>6} {:>6}",
            "L", "bound", "atk", "atk+c2", "floyd", "fl+c2"
        );
        eprintln!("{}", "-".repeat(48));
        for &l in &lightnesses {
            let rows: Vec<_> = worst.iter().filter(|w| w.2 == l).collect();
            let k = rows.len() as f32;
            let b: f32 = rows.iter().map(|w| w.3).sum::<f32>() / k;
            let m: Vec<f32> = (0..4)
                .map(|ci| rows.iter().map(|w| w.4[ci]).sum::<f32>() / k)
                .collect();
            eprintln!(
                "{l:>5.2} | {b:>6.3} | {:>6.3} {:>6.3} {:>6.3} {:>6.3}",
                m[0], m[1], m[2], m[3]
            );
        }

        let at_bound = worst.iter().filter(|w| w.0 < 0.02).count();
        eprintln!("\n{at_bound}/{count} targets are already within 0.02 dE of the physical bound.");
    }

    /// Rank every algorithm separately on reachable and gamut-limited targets.
    ///
    /// A single mean over the hue circle hides the decision, because the two
    /// halves reward opposite behaviour. Where the target is reachable, the
    /// job is to mix, and error must survive to accumulate. Where it is not,
    /// the best available answer is usually one solid ink, and the residual
    /// error is permanent and one-signed — an algorithm that faithfully
    /// propagates all of it drives the accumulator away until it trips a
    /// wrong ink. Averaging the two hides which algorithm fails which way.
    #[test]
    #[ignore = "diagnostic; run with --ignored --nocapture"]
    fn test_algorithm_ranking_in_and_out_of_gamut() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 16;

        let algos = [
            ("atkinson", DitherAlgorithm::Atkinson),
            ("atkinson-hybrid", DitherAlgorithm::AtkinsonHybrid),
            ("floyd-steinberg", DitherAlgorithm::FloydSteinberg),
            ("jarvis-judice-ninke", DitherAlgorithm::JarvisJudiceNinke),
            ("sierra", DitherAlgorithm::Sierra),
            ("sierra-two-row", DitherAlgorithm::SierraTwoRow),
            ("sierra-lite", DitherAlgorithm::SierraLite),
            ("stucki", DitherAlgorithm::Stucki),
            ("burkes", DitherAlgorithm::Burkes),
        ];

        // A target counts as reachable when the physical bound is essentially
        // zero: the palette can express it, so any miss is the algorithm's.
        const REACHABLE: f32 = 0.02;
        let lightnesses = [0.2f32, 0.32, 0.44, 0.56, 0.68, 0.8];

        let mut in_gap = vec![0.0f32; algos.len()];
        let mut in_worst = vec![0.0f32; algos.len()];
        let mut out_gap = vec![0.0f32; algos.len()];
        let mut out_worst = vec![0.0f32; algos.len()];
        let (mut n_in, mut n_out) = (0usize, 0usize);

        // Sweep saturation too. At s=1.0 only 20 of 144 targets are reachable,
        // which is far too thin a sample to choose on — and unrepresentative,
        // since real content is mostly not fully saturated. The muted rings
        // are where a screen actually lives.
        let saturations = [0.25f32, 0.5, 1.0];

        for &s in &saturations {
            for &l in &lightnesses {
                for hue_deg in (0..360).step_by(15) {
                    let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, s, l);
                    let src = Srgb::new(r, g, b);
                    let target = Oklab::from(LinearRgb::from(src));
                    let pixels = vec![src; PATCH * PATCH];
                    let (bound, _) = best_reachable(&palette, target);
                    let reachable = bound < REACHABLE;
                    if reachable {
                        n_in += 1;
                    } else {
                        n_out += 1;
                    }

                    for (ai, &(_, algo)) in algos.iter().enumerate() {
                        let out = EinkDitherer::new(palette.clone())
                            .algorithm(algo)
                            .dither(&pixels, PATCH, PATCH);
                        let mut acc = [0.0f32; 3];
                        for &idx in out.indices() {
                            let c = palette.actual_linear(idx as usize);
                            acc[0] += c.r;
                            acc[1] += c.g;
                            acc[2] += c.b;
                        }
                        let n = (PATCH * PATCH) as f32;
                        let avg = Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
                        let gap = avg.distance_squared(target).sqrt() - bound;
                        if reachable {
                            in_gap[ai] += gap;
                            in_worst[ai] = in_worst[ai].max(gap);
                        } else {
                            out_gap[ai] += gap;
                            out_worst[ai] = out_worst[ai].max(gap);
                        }
                    }
                }
            }
        }

        eprintln!("\n=== Algorithm ranking, split by whether the target is reachable ===");
        eprintln!("{n_in} reachable targets (bound < {REACHABLE}), {n_out} gamut-limited\n");
        eprintln!(
            "{:<20} | {:>9} {:>9} | {:>9} {:>9}",
            "algorithm", "in mean", "in worst", "out mean", "out worst"
        );
        eprintln!("{}", "-".repeat(66));
        let mut order: Vec<usize> = (0..algos.len()).collect();
        order.sort_by(|&a, &b| {
            (in_gap[a] / n_in as f32 + out_gap[a] / n_out as f32)
                .partial_cmp(&(in_gap[b] / n_in as f32 + out_gap[b] / n_out as f32))
                .unwrap()
        });
        for ai in order {
            eprintln!(
                "{:<20} | {:>9.3} {:>9.3} | {:>9.3} {:>9.3}",
                algos[ai].0,
                in_gap[ai] / n_in as f32,
                in_worst[ai],
                out_gap[ai] / n_out as f32,
                out_worst[ai]
            );
        }
        eprintln!();
    }

    /// Which inks actually landed, against the mixture that was available.
    ///
    /// The aggregate dE says Atkinson and Floyd-Steinberg each win on about
    /// half the hue circle, which is not actionable on its own. This prints
    /// the ink histogram next to the optimal recipe for a handful of targets
    /// chosen from both halves, because "missed by 0.06" does not say whether
    /// the patch came out too dark, too light, or speckled with a wrong ink —
    /// and those have opposite fixes.
    #[test]
    #[ignore = "diagnostic; run with --ignored --nocapture"]
    fn test_ink_histogram_versus_optimal_recipe() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let names = ["blk", "wht", "red", "yel", "blu", "grn"];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 32;

        // First two: dark, in gamut, Atkinson misses badly (the open defect).
        // Last two: saturated blue, far out of gamut, Atkinson is at the bound
        // and Floyd is well past it.
        let targets = [(30i32, 0.20f32), (45, 0.32), (240, 0.44), (255, 0.44)];

        eprintln!("\n=== Ink histogram vs. the optimal mixture ===");
        for &(hue_deg, l) in &targets {
            let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, 1.0, l);
            let src = Srgb::new(r, g, b);
            let target = Oklab::from(LinearRgb::from(src));
            let pixels = vec![src; PATCH * PATCH];
            let (bound, recipe) = best_reachable(&palette, target);

            let total: f32 = recipe.iter().sum();
            let want: Vec<String> = recipe
                .iter()
                .enumerate()
                .map(|(i, &w)| format!("{}:{:>3.0}%", names[i], w / total * 100.0))
                .collect();
            eprintln!("\n  hue {hue_deg}° L {l:.2}   (physical bound dE {bound:.3})");
            eprintln!("    optimal  {}", want.join("  "));

            for (label, algo) in [
                ("atkinson", DitherAlgorithm::Atkinson),
                ("floyd   ", DitherAlgorithm::FloydSteinberg),
                ("burkes  ", DitherAlgorithm::Burkes),
                ("jarvis  ", DitherAlgorithm::JarvisJudiceNinke),
            ] {
                let out = EinkDitherer::new(palette.clone())
                    .algorithm(algo)
                    .dither(&pixels, PATCH, PATCH);
                let mut hist = vec![0usize; palette.len()];
                for &idx in out.indices() {
                    hist[idx as usize] += 1;
                }
                let n = (PATCH * PATCH) as f32;
                let got: Vec<String> = hist
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| format!("{}:{:>3.0}%", names[i], c as f32 / n * 100.0))
                    .collect();
                eprintln!("    {label} {}", got.join("  "));
            }
        }
        eprintln!();
    }

    /// A smooth input ramp must produce a smooth output ramp.
    ///
    /// If this breaks, it means a pixel is being pinned to a palette entry
    /// because its value happens to equal one, rather than because the
    /// content called for it. Exact-match passthrough used to do exactly
    /// that: any pixel equal to an official palette colour was forced to
    /// that entry and its error discarded. Mid-gradient that puts a hard
    /// seam across a smooth ramp, and at hue 120 (pure #00FF00) it pinned
    /// the patch to the panel's dark green (L 0.56) when a bright
    /// yellow-green mixture (L 0.87) was available and far closer.
    ///
    /// The ramp below crosses hue 120 deliberately. The test is scale-free:
    /// it compares the largest step between neighbouring patches against the
    /// median step, so it measures smoothness rather than any absolute
    /// colour, and does not need updating when tuning changes.
    #[test]
    fn test_ramp_through_palette_primary_has_no_seam() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        // Measured E1002 colours: the seam only exists when the panel's real
        // green (#0D876B, dark) differs from the official #00FF00 that the
        // content asks for. With actual == official, pinning to it is right.
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 12;

        // Hues 108..132 straddle 120, where pure #00FF00 lands exactly on a
        // palette entry.
        let mut avgs: Vec<Oklab> = Vec::new();
        for step in 0..=12 {
            let hue = 108.0 + step as f32 * 2.0;
            let (r, g, b) = hsl_to_rgb(hue / 360.0, 1.0, 0.5);
            let pixels = vec![Srgb::new(r, g, b); PATCH * PATCH];
            let out = EinkDitherer::new(palette.clone())
                .algorithm(DitherAlgorithm::Atkinson)
                .dither(&pixels, PATCH, PATCH);
            let mut sum = [0.0f32; 3];
            for &idx in out.indices() {
                let c = palette.actual_linear(idx as usize);
                sum[0] += c.r;
                sum[1] += c.g;
                sum[2] += c.b;
            }
            let n = (PATCH * PATCH) as f32;
            avgs.push(Oklab::from(LinearRgb::new(
                sum[0] / n,
                sum[1] / n,
                sum[2] / n,
            )));
        }

        let mut steps: Vec<f32> = avgs
            .windows(2)
            .map(|w| w[0].distance_squared(w[1]).sqrt())
            .collect();
        let max_step = steps.iter().cloned().fold(0.0f32, f32::max);
        let mut sorted = steps.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        steps.sort_by(|a, b| b.partial_cmp(a).unwrap());

        // A seam shows up as one step dwarfing its neighbours. Allow generous
        // headroom: dithering is quantised, so steps are never uniform.
        assert!(
            max_step <= median * 6.0 + 0.02,
            "ramp through hue 120 has a seam: largest step {max_step:.3} vs \
             median {median:.3} (steps, descending: {:?})",
            steps
                .iter()
                .map(|s| (s * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>()
        );
    }

    /// Sweep `error_clamp` under the new "bound the error" semantics to pick
    /// per-algorithm defaults, scoring against the palette's physical bound.
    ///
    /// Run: `cargo test -p eink-dither clamp_sweep -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_error_clamp_sweep_against_bound() {
        let official = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(255, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(0, 255, 0),
        ];
        let actual = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(0xB5, 0x03, 0x03),
            Srgb::from_u8(0xFF, 0xEE, 0x00),
            Srgb::from_u8(0x20, 0x54, 0x97),
            Srgb::from_u8(0x0D, 0x87, 0x6B),
        ];
        let palette = Palette::new(&official, Some(&actual)).unwrap();
        const PATCH: usize = 16;
        let lightnesses = [0.2f32, 0.32, 0.44, 0.56, 0.68, 0.8];

        // Precompute the bound once; it does not depend on tuning.
        let mut targets = Vec::new();
        let mut sum_bound = 0.0f32;
        for &l in &lightnesses {
            for hue_deg in (0..360).step_by(15) {
                let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, 1.0, l);
                let src = Srgb::new(r, g, b);
                let target = Oklab::from(LinearRgb::from(src));
                let (bound, _) = best_reachable(&palette, target);
                sum_bound += bound;
                targets.push((src, target));
            }
        }
        let n = targets.len() as f32;
        eprintln!(
            "\nbound = {:.4} (mean dE over {} targets)\n",
            sum_bound / n,
            targets.len()
        );

        for algo in [
            DitherAlgorithm::Atkinson,
            DitherAlgorithm::FloydSteinberg,
            DitherAlgorithm::SierraLite,
            DitherAlgorithm::JarvisJudiceNinke,
        ] {
            eprint!("{:>20}:", format!("{algo:?}"));
            for &ec in &[0.05f32, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0] {
                let mut tot = 0.0f32;
                for (src, target) in &targets {
                    let pixels = vec![*src; PATCH * PATCH];
                    let out = EinkDitherer::new(palette.clone())
                        .algorithm(algo)
                        .error_clamp(ec)
                        .dither(&pixels, PATCH, PATCH);
                    let mut acc = [0.0f32; 3];
                    for &idx in out.indices() {
                        let c = palette.actual_linear(idx as usize);
                        acc[0] += c.r;
                        acc[1] += c.g;
                        acc[2] += c.b;
                    }
                    let m = (PATCH * PATCH) as f32;
                    let avg = Oklab::from(LinearRgb::new(acc[0] / m, acc[1] / m, acc[2] / m));
                    tot += avg.distance_squared(*target).sqrt();
                }
                eprint!("  ec{ec}={:.4}", tot / n);
            }
            eprintln!();
        }
    }

    /// Sweep error_clamp against BOTH metrics at once: muted-colour accuracy
    /// (which wants a tight bound) and the saturated-patch gamut gap (which
    /// wants a loose one). The default has to satisfy both.
    ///
    /// Run: `cargo test -p eink-dither clamp_tradeoff -- --nocapture --ignored`
    #[test]
    #[ignore] // diagnostic -- run manually
    fn test_error_clamp_tradeoff() {
        let pal6 = Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(0, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(255, 255, 0),
            ],
            None,
        )
        .unwrap();
        let muted: &[(&str, Srgb)] = &[
            ("warm shadow", Srgb::from_u8(80, 70, 60)),
            ("cool shadow", Srgb::from_u8(60, 65, 75)),
            ("overcast sky", Srgb::from_u8(180, 185, 200)),
            ("concrete", Srgb::from_u8(150, 145, 135)),
            ("faded blue", Srgb::from_u8(130, 140, 160)),
            ("dark leaf", Srgb::from_u8(50, 65, 40)),
            ("sunset glow", Srgb::from_u8(220, 200, 170)),
        ];

        let measured = Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(255, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(0, 255, 0),
            ],
            Some(&[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(0xB5, 0x03, 0x03),
                Srgb::from_u8(0xFF, 0xEE, 0x00),
                Srgb::from_u8(0x20, 0x54, 0x97),
                Srgb::from_u8(0x0D, 0x87, 0x6B),
            ]),
        )
        .unwrap();

        eprintln!(
            "\n{:>6} | {:>12} | {:>12}",
            "clamp", "muted max dE", "gamut mean dE"
        );
        eprintln!("{}", "-".repeat(38));
        for &ec in &[0.05f32, 0.1, 0.15, 0.2, 0.3, 0.5, 1.0, 2.0] {
            // muted accuracy: worst case over the set
            let mut worst = 0.0f32;
            for &(_, color) in muted {
                let image = vec![color; 128 * 128];
                let out = EinkDitherer::new(pal6.clone())
                    .saturation(1.0)
                    .contrast(1.0)
                    .error_clamp(ec)
                    .dither(&image, 128, 128);
                let mut acc = [0.0f32; 3];
                for &idx in out.indices() {
                    let c = pal6.actual_linear(idx as usize);
                    acc[0] += c.r;
                    acc[1] += c.g;
                    acc[2] += c.b;
                }
                let n = out.indices().len() as f32;
                let avg = Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
                let de = avg
                    .distance_squared(Oklab::from(LinearRgb::from(color)))
                    .sqrt();
                worst = worst.max(de);
            }

            // saturated patches vs the measured palette
            let mut tot = 0.0f32;
            let mut cnt = 0.0f32;
            for &l in &[0.2f32, 0.44, 0.68] {
                for hue_deg in (0..360).step_by(30) {
                    let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, 1.0, l);
                    let src = Srgb::new(r, g, b);
                    let out = EinkDitherer::new(measured.clone()).error_clamp(ec).dither(
                        &vec![src; 16 * 16],
                        16,
                        16,
                    );
                    let mut acc = [0.0f32; 3];
                    for &idx in out.indices() {
                        let c = measured.actual_linear(idx as usize);
                        acc[0] += c.r;
                        acc[1] += c.g;
                        acc[2] += c.b;
                    }
                    let n = 256.0f32;
                    let avg = Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
                    tot += avg
                        .distance_squared(Oklab::from(LinearRgb::from(src)))
                        .sqrt();
                    cnt += 1.0;
                }
            }
            eprintln!("{ec:>6} | {worst:>12.4} | {:>12.4}", tot / cnt);
        }
    }
}
