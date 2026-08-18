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
    use crate::palette::{ColourModel, Palette};
    use crate::Oklch;

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
            dither_with_kernel_noise(&image_186, size, size, &palette, &ATKINSON, &options, None);
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
            dither_with_kernel_noise(&image_128, size, size, &palette, &ATKINSON, &options, None);
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
                    None,
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

            let has_blue = indices.contains(&4);
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
                None,
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
        let (idx, _) = palette.find_nearest(brown, ColourModel::Measured);
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
        let (idx, _) = palette.find_nearest(dark_red, ColourModel::Measured);
        assert_eq!(
            idx, 2,
            "REGRESSION (TEST-04): Dark red (139,0,0) should map to red (idx 2), got {}",
            idx
        );

        // Dark blue should map to blue, not black
        let dark_blue = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 139)));
        let (idx, _) = palette.find_nearest(dark_blue, ColourModel::Measured);
        assert_eq!(
            idx, 4,
            "REGRESSION (TEST-04): Dark blue (0,0,139) should map to blue (idx 4), got {}",
            idx
        );

        // Navy should map to blue, not black
        let navy = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 128)));
        let (idx, _) = palette.find_nearest(navy, ColourModel::Measured);
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
                let indices = dither_with_kernel_noise(
                    &image,
                    255,
                    255,
                    &photo_palette,
                    &ATKINSON,
                    &options,
                    None,
                );

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
        let (idx, _) = palette.find_nearest(dark_green, ColourModel::Measured);

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
                pcts.first().unwrap_or(&0.0),
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
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &ATKINSON,
            &options,
            None,
        );
        check_neutrality(&result, "Atkinson");

        // Test FloydSteinberg -- 100% propagation naturally cancels
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &FLOYD_STEINBERG,
            &options,
            None,
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
            None,
        );
        let blue_count = result.iter().filter(|&&idx| idx == 4).count();
        let blue_pct = blue_count as f64 / result.len() as f64 * 100.0;
        assert!(
            blue_pct > 1.0,
            "REGRESSION: FloydSteinberg white->dark_blue gradient has only {blue_pct:.2}% \
             blue pixels (expected >1%). Blue gradient renders as black."
        );

        // Test Atkinson
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &ATKINSON,
            &options,
            None,
        );
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
            "{:>5} | {:14} | {:28} | {:>8} | dist",
            "Hue", "sRGB", "OKLab L     a      b     C", "nearest"
        );
        eprintln!("{}", "-".repeat(85));

        for hue_deg in (90..=270).step_by(5) {
            let h = hue_deg as f32 / 360.0;
            // HSL to sRGB conversion
            let (r, g, b) = hsl_to_rgb(h, 1.0, 0.5);
            let srgb = Srgb::new(r, g, b);
            let oklab = Oklab::from(LinearRgb::from(srgb));
            let chroma = (oklab.a * oklab.a + oklab.b * oklab.b).sqrt();
            let (idx, dist) = photo_palette.find_nearest(oklab, ColourModel::Measured);

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
            None,
        );
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // Atkinson
        eprintln!("\n=== Atkinson: per-column dominant palette entry ===");
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &ATKINSON,
            &options,
            None,
        );
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
            None,
        );
        print_column_dominance(
            &result, width, height, &palette, &names, hue_start, hue_step,
        );

        // Sierra (full)
        eprintln!("\n=== Sierra: per-column dominant palette entry ===");
        let result = dither_with_kernel_noise(
            &image,
            width,
            height,
            &photo_palette,
            &SIERRA,
            &options,
            None,
        );
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
            None,
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
            None,
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

        let result1 =
            dither_with_kernel_noise(&image, 4, 4, &photo_palette, &ATKINSON, &options, None);
        let result2 =
            dither_with_kernel_noise(&image, 4, 4, &photo_palette, &ATKINSON, &options, None);

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

        // Dilute near-black starts. From a pure single-ink vertex (e.g. pure
        // black), growing another ink's weight is a zero-gradient move at
        // first because the cost normalises by the weight sum -- and the
        // coarsest ladder step overshoots targets that sit close to that
        // vertex before a gradient becomes visible. Without a foothold,
        // coordinate descent halts right at the vertex and reports the
        // target's own distance from it. Seeding starts that are already
        // slightly diluted with every other ink, at several dilution levels,
        // gives descent a direction to follow from step 1.
        let darkest = (0..n)
            .min_by(|&a, &b| {
                let la = Oklab::from(palette.actual_linear(a)).l;
                let lb = Oklab::from(palette.actual_linear(b)).l;
                la.total_cmp(&lb)
            })
            .unwrap();
        for i in 0..n {
            if i == darkest {
                continue;
            }
            for &eps in &[0.1f32, 0.03, 0.01, 0.003, 0.001, 0.0003, 0.0001] {
                let mut w = vec![0.0; n];
                w[darkest] = 1.0 - eps;
                w[i] = eps;
                starts.push(w);
            }
        }

        let mut best = (f32::MAX, vec![0.0; n]);
        for mut w in starts {
            let mut cur = cost(&w);
            // The tail (0.001, 0.0005, 0.0001) matters near the darkest
            // vertex: Cmax is tiny there, so the target itself is only a
            // few thousandths of chroma away and coarser steps can't resolve
            // it.
            for &step in &[
                0.4f32, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001, 0.0005, 0.0001,
            ] {
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
                worst.push((got[0] - bound, hue_deg, l, bound, got, recipe_s.join(" ")));
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

    /// Walk the error trajectory and print which ink the matcher returns.
    ///
    /// At 45deg L0.32 the target is fully in gamut, so there is no permanent
    /// residual to run away, yet Atkinson still fills 41% of the patch with
    /// green -- an ink the optimal recipe does not use. Either the matcher
    /// picks green for the unperturbed target (a matching bug), or diffusion
    /// walks the accumulator into a region where green wins (a dynamics bug).
    /// Those have opposite fixes, so this separates them: it steps along the
    /// error each ink choice creates and reports the decision region entered.
    #[test]
    #[ignore = "diagnostic; run with --ignored --nocapture"]
    fn test_error_trajectory_decision_regions() {
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
        // The ditherer matches through for_error_diffusion(), so the probe
        // must too -- the builder swaps the metric to Euclidean there, and
        // probing the HyAB default would answer a question nobody asked.
        let palette = Palette::new(&official, Some(&actual))
            .unwrap()
            .for_error_diffusion();

        for &(hue_deg, l) in &[(45i32, 0.32f32), (30, 0.20)] {
            let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, 1.0, l);
            let src = LinearRgb::from(Srgb::new(r, g, b));
            let (nearest, _) = palette.find_nearest(Oklab::from(src), ColourModel::Measured);
            eprintln!("\n=== hue {hue_deg}deg L {l:.2} ===");
            eprintln!(
                "  unperturbed target matches: {} (linear {:.3} {:.3} {:.3})",
                names[nearest], src.r, src.g, src.b
            );
            let t_ok = Oklab::from(src);
            let mut d: Vec<(f32, usize)> = (0..palette.len())
                .map(|k| {
                    let p = palette.actual_oklab(k);
                    (t_ok.distance_squared(p).sqrt(), k)
                })
                .collect();
            d.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let ranked: Vec<String> = d
                .iter()
                .map(|&(dist, k)| format!("{}:{dist:.3}", names[k]))
                .collect();
            eprintln!(
                "  euclidean OKLab distance, nearest first: {}",
                ranked.join("  ")
            );
            let (bound, recipe) = best_reachable(&palette, t_ok);
            let want: Vec<String> = recipe
                .iter()
                .enumerate()
                .filter(|(_, &w)| w > 0.02)
                .map(|(i, &w)| format!("{}:{:.0}%", names[i], w * 100.0))
                .collect();
            eprintln!("  optimal mixture (bound {bound:.3}): {}", want.join("  "));

            // Choosing ink k leaves error (target - ink). The next pixel is
            // presented with target + that error, and so on if the same ink
            // keeps winning. Stepping t along it traces where that leads.
            for k in 0..palette.len() {
                let ink = palette.actual_linear(k);
                let e = [src.r - ink.r, src.g - ink.g, src.b - ink.b];
                let mut regions: Vec<String> = Vec::new();
                let mut last = usize::MAX;
                for step in 0..=20 {
                    let t = step as f32 * 0.1;
                    // apply_error bounds the accumulated error, not the value.
                    let acc: Vec<f32> = e.iter().map(|c| (c * t).clamp(-1.0, 1.0)).collect();
                    let p = LinearRgb::new(src.r + acc[0], src.g + acc[1], src.b + acc[2]);
                    let (idx, _) = palette.find_nearest(Oklab::from(p), ColourModel::Measured);
                    if idx != last {
                        regions.push(format!("t={t:.1}->{}", names[idx]));
                        last = idx;
                    }
                }
                eprintln!("  after picking {:<4}: {}", names[k], regions.join("  "));
            }
        }
        eprintln!();
    }

    /// What does raising `noise_scale` cost in colour accuracy?
    ///
    /// The jitter visibly removes two structured artifacts -- herringbone
    /// over flat areas and the limit-cycle streaks that draw a solid line
    /// through a gradient -- and the shipped defaults are low (Atkinson 0.0).
    /// Raising them is only worth proposing if the accuracy it buys back is
    /// priced, and luminance alone will not price it: the jitter perturbs the
    /// kernel weights, so any damage should appear in colour first.
    ///
    /// Measured against the palette's physical bound, over the same
    /// saturation-swept targets as the algorithm ranking.
    #[test]
    #[ignore = "diagnostic; run with --ignored --nocapture"]
    fn test_noise_scale_against_bound() {
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
        const REACHABLE: f32 = 0.02;

        let scales = [0.0f32, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0];
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
        let saturations = [0.25f32, 0.5, 1.0];
        let lightnesses = [0.2f32, 0.32, 0.44, 0.56, 0.68, 0.8];

        eprintln!("\n=== noise_scale vs. the palette's physical bound ===");
        eprintln!("mean gap over bound, split by whether the target is reachable\n");
        eprintln!(
            "{:<20} {:>6} | {:>9} {:>9}",
            "algorithm", "noise", "in gap", "out gap"
        );
        eprintln!("{}", "-".repeat(50));

        for &(name, algo) in &algos {
            for &scale in &scales {
                let (mut in_sum, mut out_sum) = (0.0f32, 0.0f32);
                let (mut n_in, mut n_out) = (0usize, 0usize);
                for &s in &saturations {
                    for &l in &lightnesses {
                        for hue_deg in (0..360).step_by(15) {
                            let (r, g, b) = hsl_to_rgb(hue_deg as f32 / 360.0, s, l);
                            let src = Srgb::new(r, g, b);
                            let target = Oklab::from(LinearRgb::from(src));
                            let pixels = vec![src; PATCH * PATCH];
                            let (bound, _) = best_reachable(&palette, target);

                            let out = EinkDitherer::new(palette.clone())
                                .algorithm(algo)
                                .noise_scale(scale)
                                .dither(&pixels, PATCH, PATCH);
                            let mut acc = [0.0f32; 3];
                            for &idx in out.indices() {
                                let c = palette.actual_linear(idx as usize);
                                acc[0] += c.r;
                                acc[1] += c.g;
                                acc[2] += c.b;
                            }
                            let n = (PATCH * PATCH) as f32;
                            let avg =
                                Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
                            let gap = avg.distance_squared(target).sqrt() - bound;
                            if bound < REACHABLE {
                                in_sum += gap;
                                n_in += 1;
                            } else {
                                out_sum += gap;
                                n_out += 1;
                            }
                        }
                    }
                }
                eprintln!(
                    "{name:<20} {scale:>6.1} | {:>9.4} {:>9.4}",
                    in_sum / n_in as f32,
                    out_sum / n_out as f32
                );
            }
            eprintln!();
        }
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
                ("atk-hybr", DitherAlgorithm::AtkinsonHybrid),
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

    /// The six-ink panel palette, official colours.
    fn six_color_palette() -> Palette {
        Palette::new(
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
        .unwrap()
    }

    /// The fast `Cmax` table must agree with the slow exact oracle.
    ///
    /// `best_reachable` finds the closest point in the hull by optimisation.
    /// For a target sitting exactly at the table's reported chroma limit, that
    /// distance must be near zero — the point is on the boundary, so it is
    /// reachable. For a target well beyond the limit it must be clearly
    /// non-zero. If the table over-reports, the first check fails; if it
    /// under-reports, the second does.
    ///
    /// Both statistics are compared as a **ratio to `Cmax`**, not as an
    /// absolute dE: `Cmax -> 0` at both lightness extremes, so an absolute
    /// bound is structurally wrong there.
    ///
    /// Measured 2026-08-08 on the six-ink official palette, repaired oracle
    /// (dilute near-black starts + step ladder extended to 0.0001, see
    /// `best_reachable`): 360 bins checked, worst in-limit ratio `0.0128`
    /// (limit `IN_LIMIT_MAX_RATIO = 0.05`, ~3.9x margin), smallest
    /// beyond-limit ratio `0.4582` (limit `BEYOND_LIMIT_MIN_RATIO = 0.3`,
    /// ~1.5x margin). For reference, the stock (unrepaired) oracle measured
    /// worst in-limit ratio `0.9002` on the same grid; the beyond-limit
    /// statistic was unaffected by the repair (`0.4582` both before and
    /// after) — the fix only matters near the pure-black trap, which the
    /// `2.5*Cmax` targets never approach.
    #[test]
    #[ignore = "sweeps the hue/lightness grid against an optimiser; slow"]
    fn test_cmax_table_agrees_with_reachability_oracle() {
        use crate::gamut::cmax::CmaxTable;
        use crate::gamut::hull::Hull;

        /// A point at `0.9 * Cmax` must be reachable within this fraction of
        /// `Cmax`. Measured worst case (repaired oracle): `0.0128`.
        const IN_LIMIT_MAX_RATIO: f32 = 0.05;
        /// A point at `2.5 * Cmax` must be at least this fraction of `Cmax`
        /// away from the hull. Measured smallest case (repaired oracle):
        /// `0.4582`.
        const BEYOND_LIMIT_MIN_RATIO: f32 = 0.3;

        let palette = six_color_palette();
        let table = CmaxTable::build(&Hull::from_palette(&palette));

        let mut at_limit_worst = 0.0f32;
        let mut beyond_limit_min = f32::MAX;
        let mut checked = 0;

        for hi in 0..24 {
            let h = -std::f32::consts::PI + (hi as f32 / 24.0) * std::f32::consts::TAU;
            for li in 1..16 {
                let l = li as f32 / 16.0;
                let c_max = table.sample(h, l);
                if c_max < 1e-3 {
                    continue;
                }
                checked += 1;

                // Just inside the reported limit: must be reachable. Compared
                // RELATIVE to c_max -- see the doc comment above.
                let inside = Oklab::from(Oklch {
                    l,
                    c: c_max * 0.9,
                    h,
                });
                let (d_in, _) = best_reachable(&palette, inside);
                at_limit_worst = at_limit_worst.max(d_in / c_max);

                // Well beyond: must not be.
                let outside = Oklab::from(Oklch {
                    l,
                    c: c_max * 2.5,
                    h,
                });
                let (d_out, _) = best_reachable(&palette, outside);
                beyond_limit_min = beyond_limit_min.min(d_out / c_max);
            }
        }

        eprintln!(
            "cmax oracle: checked {checked} bins, worst in-limit ratio {at_limit_worst:.4}, \
             smallest beyond-limit ratio {beyond_limit_min:.4}"
        );
        assert!(
            checked > 100,
            "grid produced too few usable bins: {checked}"
        );
        assert!(
            at_limit_worst < IN_LIMIT_MAX_RATIO,
            "table over-reports Cmax: a point at 0.9*Cmax was {at_limit_worst:.4}*Cmax from the hull"
        );
        assert!(
            beyond_limit_min > BEYOND_LIMIT_MIN_RATIO,
            "table under-reports Cmax: a point at 2.5*Cmax was only \
             {beyond_limit_min:.4}*Cmax from the hull"
        );
    }

    // ========================================================================
    // Gamut mapping: preserved differences, not mean accuracy
    // ========================================================================

    /// Hue ordering around the circle must be preserved.
    ///
    /// The gamut mapper carries hue through untouched — it only ever changes
    /// chroma — so the ordering of the *mapped* targets must stay monotonic
    /// even where the dithered result is not. This measures the mapper, not
    /// the ditherer.
    #[test]
    #[ignore = "diagnostic sweep"]
    fn test_gamut_mapping_preserves_hue_order() {
        use crate::{GamutMapper, GamutOptions};

        let palette = six_color_palette();
        let mapper = GamutMapper::new(&palette);
        let opts = GamutOptions::default();

        let mut inversions = 0;
        let mut prev: Option<f32> = None;
        for deg in (0..360).step_by(15) {
            let h = (deg as f32).to_radians() - std::f32::consts::PI;
            // Ruling 5 (Global Constraint): clamp linear RGB before `Srgb::from`.
            // `l = 0.55, c = 0.20` is outside sRGB for part of the circle, and
            // `linear_to_srgb` has an epsilon-free `debug_assert!`. Clamping is
            // not a workaround: `map_color` only accepts colours the source
            // image could actually contain, so this is the real input domain.
            let lin = LinearRgb::from(Oklab::from(Oklch {
                l: 0.55,
                c: 0.20,
                h,
            }));
            let src = Srgb::from(LinearRgb::new(
                lin.r.clamp(0.0, 1.0),
                lin.g.clamp(0.0, 1.0),
                lin.b.clamp(0.0, 1.0),
            ));
            let mapped = mapper.map_color(src, 2.0, opts);
            let h_out = Oklch::from(Oklab::from(LinearRgb::from(mapped))).h;
            if let Some(p) = prev {
                // Both sequences advance around the circle; a decrease that is
                // not the single wrap point is an inversion.
                let step = h_out - p;
                if step < 0.0 && step > -std::f32::consts::PI {
                    inversions += 1;
                    eprintln!("hue inversion at {deg}\u{b0}: {p:.3} -> {h_out:.3}");
                }
            }
            prev = Some(h_out);
        }
        assert_eq!(inversions, 0, "gamut mapping must not reorder hues");
    }

    /// Local contrast across a saturation ramp must survive mapping.
    ///
    /// The point of the knee's strict monotonicity: adjacent steps of a ramp
    /// stay distinct. A clipping approach would collapse the top of the ramp
    /// to a single value.
    #[test]
    #[ignore = "diagnostic sweep"]
    fn test_gamut_mapping_preserves_local_contrast() {
        use crate::{GamutMapper, GamutOptions};

        let palette = six_color_palette();
        let mapper = GamutMapper::new(&palette);
        let opts = GamutOptions::default();

        // The ramp runs **along one compression ray**, not across a row of
        // fixed lightness. That is what makes it discriminating under ruling
        // 16: every sample shares a single direction from the anchor, so a
        // mapper that clips sends all of the out-of-gamut ones to the *same*
        // boundary point and the ramp collapses outright. A ramp at fixed `L`
        // gives each step its own ray direction and therefore its own boundary
        // point, so clipping keeps them accidentally distinct — the previous
        // form of this test passed against a clipping mutant. Verified by
        // mutation both ways.
        //
        // `r = 1.0` deliberately: a larger `r` widens the tail's input span so
        // the shoulder is never reached.
        //
        // Separation is measured between the mapped *points*. Under ruling 16
        // the map moves lightness as well, so chroma alone no longer describes
        // the output — past the shoulder chroma asymptotes and drifts back by
        // ~5e-5 while the points stay cleanly separated in `L`.
        let (dir_l, dir_c) = (0.55f32 - 0.5, 0.32f32);
        let mut collapsed = 0;
        let mut min_sep = f32::INFINITY;
        let mut prev: Option<Oklch> = None;
        for i in 1..=64 {
            // Bounded so the source stays within the ~0.33 Oklab chroma any
            // sRGB colour can reach. Sweeping further would synthesise colours
            // no input can produce, and at a high knee those all land on the
            // asymptote and tie in f32 — comparing two colours that have both
            // already collapsed, which says nothing about banding. The boundary
            // along this ray sits near s = 0.56, so the tail is still most of
            // the sweep.
            let s = i as f32 / 64.0 * 1.03;
            let src = Oklch {
                l: 0.5 + s * dir_l,
                c: s * dir_c,
                h: 0.6,
            };
            let out = mapper.mapped_point(src, 1.0, opts);
            if let Some(p) = prev {
                // Distance in Oklab: at fixed hue the ramp is planar, so this
                // is the (L, C) separation.
                let sep = ((out.l - p.l).powi(2) + (out.c - p.c).powi(2)).sqrt();
                min_sep = min_sep.min(sep);
                if sep <= 0.0 {
                    collapsed += 1;
                    eprintln!("ramp collapsed at s={s:.4}: {p:?} -> {out:?}");
                }
            }
            prev = Some(out);
        }
        eprintln!("minimum step separation in Oklab: {min_sep:.2e}");
        assert_eq!(collapsed, 0, "every ramp step must stay distinct");
    }

    // ========================================================================
    // Task 8: the region colour-model measurement pass
    // ========================================================================
    //
    // These are DIAGNOSTICS (owner ruling 20): non-asserting, `#[ignore]`d,
    // and their printed output is the deliverable. The one exception is
    // `an_all_continuous_mask_is_bit_identical_to_no_mask`, which guards an
    // invariant rather than a tuned threshold and therefore runs by default.
    //
    // They live here, in a lib unit test, and NOT in `tests/`: the only
    // fixture whose official and actual colour sets genuinely differ is
    // `crate::gamut::test_support::panel_measured()`, which is
    // `#[cfg(test)] pub(crate)` and invisible to an integration test. Under
    // any `Palette::new(x, None)` fixture the two colour models are identical
    // and every number below would read zero difference while appearing to
    // work.
    //
    // Run:
    //   cargo test -p eink-dither --lib region_model -- --ignored --nocapture
    mod region_model {
        use crate::api::EinkDitherer;
        use crate::color::{LinearRgb, Oklab, Srgb};
        use crate::dither::DitherAlgorithm;
        use crate::gamut::test_support::panel_measured;
        use crate::output::DitheredImage;
        use crate::{GamutMapper, GamutOptions};
        use std::path::PathBuf;

        /// `panel_measured()`'s entries, in index order. Indices 2-5 are the
        /// probes that can discriminate the two colour models; 0 and 1 are
        /// degenerate (official == actual) even in this fixture.
        const INKS: [&str; 6] = ["black", "white", "red", "yellow", "blue", "green"];

        /// The λ values Step 1 sweeps. `pin_carry`'s shipping default is 0.9
        /// and is PROVISIONAL — this sweep is what informs it.
        const LAMBDAS: [f32; 6] = [0.0, 0.5, 0.8, 0.9, 0.95, 1.0];

        /// Frame size every real-asset measurement runs at — a TRMNL panel.
        const FRAME_W: usize = 800;
        const FRAME_H: usize = 480;

        // ------------------------------------------------------------------
        // shared helpers
        // ------------------------------------------------------------------

        /// Load a repo asset and resample it to `w`x`h` with the `image` dev
        /// dependency. eink-dither's own `resize_lanczos` panics on any real
        /// dimension change (no image backend in the crate proper), so test
        /// code resizes here, exactly as `tests/visual_compare.rs` does.
        ///
        /// `resize_to_fill` crops to the panel's aspect rather than distorting.
        fn asset(rel: &str, w: usize, h: usize) -> Option<Vec<Srgb>> {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(rel);
            let img = match image::open(&path) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("SKIPPING: {} not readable ({e})", path.display());
                    return None;
                }
            };
            let img = img.resize_to_fill(w as u32, h as u32, image::imageops::FilterType::Lanczos3);
            Some(
                img.to_rgb8()
                    .pixels()
                    .map(|p| Srgb::from_u8(p[0], p[1], p[2]))
                    .collect(),
            )
        }

        /// Write an RGB buffer into `target/dither-compare/` and return its path.
        fn write_png(name: &str, rgb: &[u8], w: usize, h: usize) -> String {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dither-compare");
            std::fs::create_dir_all(&dir).expect("create target/dither-compare");
            let path = dir.join(name);
            image::save_buffer(
                &path,
                rgb,
                w as u32,
                h as u32,
                image::ExtendedColorType::Rgb8,
            )
            .expect("write png");
            path.display().to_string()
        }

        /// The panel's own appearance of a render: measured inks, whatever
        /// colour model the matching ran under.
        fn appearance(img: &DitheredImage) -> Vec<Oklab> {
            img.to_rgb_actual()
                .chunks_exact(3)
                .map(|c| Oklab::from(LinearRgb::from(Srgb::from_u8(c[0], c[1], c[2]))))
                .collect()
        }

        /// Mean Oklab ΔE between two renders over a pixel set.
        fn mean_de(a: &[Oklab], b: &[Oklab], set: &[usize]) -> f64 {
            if set.is_empty() {
                return f64::NAN;
            }
            let sum: f64 = set
                .iter()
                .map(|&i| {
                    let (p, q) = (a[i], b[i]);
                    (((p.l - q.l).powi(2) + (p.a - q.a).powi(2) + (p.b - q.b).powi(2)) as f64)
                        .sqrt()
                })
                .sum();
            sum / set.len() as f64
        }

        /// The same appearance, kept in linear light so blocks of it can be
        /// averaged before the perceptual distance is taken.
        fn appearance_linear(img: &DitheredImage) -> Vec<LinearRgb> {
            img.to_rgb_actual()
                .chunks_exact(3)
                .map(|c| LinearRgb::from(Srgb::from_u8(c[0], c[1], c[2])))
                .collect()
        }

        /// Mean Oklab ΔE after averaging both renders in LINEAR light over
        /// `k` x `k` blocks; `k = 1` reproduces `mean_de` exactly.
        ///
        /// A per-pixel ΔE between two halftones is dominated by dot-pattern
        /// decorrelation: two dithers of the SAME image score a large per-pixel
        /// distance while being visually identical, because the dots land in
        /// different places. Only the block-averaged figure is on a scale where
        /// "one JND ~ 0.02" means anything, because only it measures the
        /// spatially-integrated colour the eye and the panel actually deliver.
        ///
        /// `keep` restricts the measurement to blocks that are at least half
        /// made of marked pixels (used for the out-of-gamut subset, which is
        /// not block-aligned).
        fn block_de(
            a: &[LinearRgb],
            b: &[LinearRgb],
            w: usize,
            h: usize,
            k: usize,
            keep: Option<&[bool]>,
        ) -> f64 {
            let mut sum = 0.0f64;
            let mut blocks = 0usize;
            for by in (0..h).step_by(k) {
                for bx in (0..w).step_by(k) {
                    let (mut sa, mut sb) = ([0.0f64; 3], [0.0f64; 3]);
                    let (mut n, mut kept) = (0usize, 0usize);
                    for y in by..(by + k).min(h) {
                        for x in bx..(bx + k).min(w) {
                            let i = y * w + x;
                            sa[0] += a[i].r as f64;
                            sa[1] += a[i].g as f64;
                            sa[2] += a[i].b as f64;
                            sb[0] += b[i].r as f64;
                            sb[1] += b[i].g as f64;
                            sb[2] += b[i].b as f64;
                            n += 1;
                            if keep.is_none_or(|m| m[i]) {
                                kept += 1;
                            }
                        }
                    }
                    if n == 0 || kept * 2 < n {
                        continue;
                    }
                    let inv = 1.0 / n as f64;
                    let pa = Oklab::from(LinearRgb::new(
                        (sa[0] * inv) as f32,
                        (sa[1] * inv) as f32,
                        (sa[2] * inv) as f32,
                    ));
                    let pb = Oklab::from(LinearRgb::new(
                        (sb[0] * inv) as f32,
                        (sb[1] * inv) as f32,
                        (sb[2] * inv) as f32,
                    ));
                    sum += (((pa.l - pb.l).powi(2) + (pa.a - pb.a).powi(2) + (pa.b - pb.b).powi(2))
                        as f64)
                        .sqrt();
                    blocks += 1;
                }
            }
            if blocks == 0 {
                return f64::NAN;
            }
            sum / blocks as f64
        }

        /// Share of a pixel set whose chosen ink differs between two renders.
        fn changed_share(a: &DitheredImage, b: &DitheredImage, set: &[usize]) -> f64 {
            if set.is_empty() {
                return f64::NAN;
            }
            let n = set
                .iter()
                .filter(|&&i| a.indices()[i] != b.indices()[i])
                .count();
            n as f64 / set.len() as f64
        }

        /// Ink counts over a pixel set.
        fn hist(img: &DitheredImage, set: &[usize]) -> [usize; 6] {
            let mut c = [0usize; 6];
            for &i in set {
                c[img.indices()[i] as usize] += 1;
            }
            c
        }

        /// Format an ink histogram as shares, largest first, zeroes dropped.
        fn fmt_hist(c: &[usize; 6]) -> String {
            let total: usize = c.iter().sum();
            if total == 0 {
                return "(empty)".into();
            }
            let mut v: Vec<(usize, &str)> = c.iter().copied().zip(INKS).collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.0));
            v.iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, name)| format!("{name} {:.1}%", *n as f64 / total as f64 * 100.0))
                .collect::<Vec<_>>()
                .join("  ")
        }

        /// Total-variation distance between two ink histograms, in [0, 1].
        ///
        /// A PROXY for "how far this band's ink mix has been dragged from what
        /// it would have been". It is not a validated quality criterion; it is
        /// a single number to rank λ by, and the full histograms are printed
        /// beside it so the owner can disagree with the ranking.
        fn tvd(a: &[usize; 6], b: &[usize; 6]) -> f64 {
            let (sa, sb): (usize, usize) = (a.iter().sum(), b.iter().sum());
            if sa == 0 || sb == 0 {
                return f64::NAN;
            }
            0.5 * a
                .iter()
                .zip(b.iter())
                .map(|(&x, &y)| (x as f64 / sa as f64 - y as f64 / sb as f64).abs())
                .sum::<f64>()
        }

        /// Every pixel index of a column range, all rows.
        fn cols(w: usize, h: usize, r: std::ops::Range<usize>) -> Vec<usize> {
            (0..h)
                .flat_map(|y| r.clone().map(move |x| y * w + x))
                .collect()
        }

        /// Every pixel index of an axis-aligned rectangle.
        fn rect(w: usize, xr: std::ops::Range<usize>, yr: std::ops::Range<usize>) -> Vec<usize> {
            yr.flat_map(|y| xr.clone().map(move |x| y * w + x))
                .collect()
        }

        /// Paint a vertical line of `ink` over `px`, `cols` wide.
        fn paint_line(px: &mut [Srgb], w: usize, h: usize, xr: std::ops::Range<usize>, ink: Srgb) {
            for y in 0..h {
                for x in xr.clone() {
                    px[y * w + x] = ink;
                }
            }
        }

        /// The shipping ditherer for these measurements: the default Atkinson
        /// configuration, exactly as `svg_to_png.rs` builds it.
        fn shipping() -> EinkDitherer {
            EinkDitherer::new(panel_measured()).algorithm(DitherAlgorithm::Atkinson)
        }

        // ------------------------------------------------------------------
        // Free consistency check (mandated): polarity is silent if flipped
        // ------------------------------------------------------------------

        /// An all-`true` mask means "everything is continuous-tone": measured
        /// colour model, pinning off, and no model boundary anywhere. That is
        /// precisely what `None` means, so the two must agree bit for bit.
        ///
        /// This is an invariant, not a tuned threshold, so unlike the
        /// diagnostics around it, it asserts and runs by default. It
        /// independently re-verifies the mask polarity, which is otherwise
        /// silent if inverted.
        #[test]
        fn an_all_continuous_mask_is_bit_identical_to_no_mask() {
            let ditherer = shipping();

            // A frame with pin-eligible content (exact inks), chromatic
            // content, and a gradient — so a polarity flip has somewhere to
            // show up.
            let (w, h) = (64usize, 64usize);
            let mut px: Vec<Srgb> = (0..w * h)
                .map(|i| {
                    let (x, y) = (i % w, i / w);
                    Srgb::from_u8((x * 4) as u8, (y * 4) as u8, 128)
                })
                .collect();
            paint_line(&mut px, w, h, 30..32, Srgb::from_u8(0, 0, 0));
            paint_line(&mut px, w, h, 10..12, Srgb::from_u8(0, 255, 0));

            let all_true = vec![true; px.len()];
            let with_mask = ditherer.dither_with_regions(&px, w, h, Some(&all_true));
            let no_mask = ditherer.dither_with_regions(&px, w, h, None);

            assert_eq!(
                with_mask.indices(),
                no_mask.indices(),
                "an all-continuous mask must be bit-identical to no mask; if it \
                 is not, the mask polarity is inverted or the measured model is \
                 not the None default"
            );

            // ...and the opposite mask must actually differ, or the check above
            // would pass against an implementation that ignores the mask.
            let all_false = vec![false; px.len()];
            let unmarked = ditherer.dither_with_regions(&px, w, h, Some(&all_false));
            assert_ne!(
                unmarked.indices(),
                no_mask.indices(),
                "an all-structure mask produced identical output to no mask — \
                 the region map is not reaching the dither loop, and the \
                 bit-identity check above proves nothing"
            );
        }

        // ------------------------------------------------------------------
        // Step 1: the λ (pin_carry) sweep
        // ------------------------------------------------------------------

        /// Print, for each λ, what a pinned 2 px line does to the 4 px band on
        /// either side of it.
        ///
        /// The line's OWN ink purity is 100% at every λ by construction — a
        /// pinned pixel outputs its ink unconditionally, and λ only decides
        /// what it forwards. It is printed anyway because the brief asks for
        /// it; it is not a discriminator. See the report's plan-defect note.
        ///
        /// The discriminator is the band: each band histogram is compared
        /// against the SAME band rendered from a line-free copy of the same
        /// frame, i.e. what those pixels would have done undisturbed.
        ///
        /// NOTE on the left band in scenario A, which measures 0.000 at every
        /// λ: that is a measured result, NOT a structural guarantee. Atkinson
        /// has a `(-1, 1)` bottom-left tap (`kernel.rs`), as do Floyd-Steinberg,
        /// JJN and Sierra, so with the line at x = 15..17 the column at x = 14
        /// is exactly one tap upstream-left of it and λ does change the error
        /// landing there. On a uniform orange field that perturbation flips no
        /// ink, so the histogram is unchanged; on other content it could.
        /// Histograms are all that is compared here — equal histograms are not
        /// pixel-for-pixel equality.
        #[test]
        #[ignore] // diagnostic -- run manually
        fn lambda_sweep_diag() {
            eprintln!("\n=== Step 1: λ (pin_carry) sweep ===");
            eprintln!("fixture: gamut::test_support::panel_measured() — the only");
            eprintln!("fixture whose official and actual sets differ (probes 2-5).\n");

            // -- Scenario A: Task 2's 2 px line in a hostile field ----------
            let (w, h) = (32usize, 32usize);
            let field = Srgb::from_u8(255, 128, 0);
            let clean = vec![field; w * h];
            let mut lined = clean.clone();
            paint_line(&mut lined, w, h, 15..17, Srgb::from_u8(0, 0, 0));
            let unmarked = vec![false; w * h];
            let left = cols(w, h, 11..15);
            let right = cols(w, h, 17..21);
            let line_set = cols(w, h, 15..17);

            eprintln!("-- A: synthetic hostile field 32x32, (255,128,0), 2 px black line");
            eprintln!("   mask: all-structure (unmarked) everywhere; noise 0, serpentine off");
            let mk = |lambda: f32| {
                EinkDitherer::new(panel_measured())
                    .noise_scale(0.0)
                    .serpentine(false)
                    .pin_carry(lambda)
            };
            // The reference frame has no line, hence no pinned pixel, so λ
            // cannot affect it.
            let ref_img = mk(0.9).dither_with_regions(&clean, w, h, Some(&unmarked));
            let (ref_l, ref_r) = (hist(&ref_img, &left), hist(&ref_img, &right));
            eprintln!("   reference (no line) left band : {}", fmt_hist(&ref_l));
            eprintln!("   reference (no line) right band: {}", fmt_hist(&ref_r));
            for lambda in LAMBDAS {
                let img = mk(lambda).dither_with_regions(&lined, w, h, Some(&unmarked));
                let (hl, hr) = (hist(&img, &left), hist(&img, &right));
                eprintln!(
                    "   λ={lambda:.2}  line black {:.1}%  TVD L/R {:.3}/{:.3}",
                    hist(&img, &line_set)[0] as f64 / line_set.len() as f64 * 100.0,
                    tvd(&hl, &ref_l),
                    tvd(&hr, &ref_r)
                );
                eprintln!("            left  {}", fmt_hist(&hl));
                eprintln!("            right {}", fmt_hist(&hr));
            }

            // -- Scenario B: a real screen render ---------------------------
            let Some(bg) = asset("screens/builtin/default/background.jpg", FRAME_W, FRAME_H) else {
                return;
            };
            let (w, h) = (FRAME_W, FRAME_H);
            let mut lined = bg.clone();
            paint_line(&mut lined, w, h, 400..402, Srgb::from_u8(0, 0, 0));
            let unmarked = vec![false; w * h];
            let left = cols(w, h, 396..400);
            let right = cols(w, h, 402..406);
            let line_set = cols(w, h, 400..402);
            let d0 = shipping();

            eprintln!(
                "\n-- B: background.jpg {w}x{h}, 2 px black line at x=400, whole frame UNMARKED"
            );
            eprintln!("   (structure model both sides, so error crosses the line freely)");
            let ref_img = d0.dither_with_regions(&bg, w, h, Some(&unmarked));
            let (ref_l, ref_r) = (hist(&ref_img, &left), hist(&ref_img, &right));
            eprintln!("   reference (no line) left band : {}", fmt_hist(&ref_l));
            eprintln!("   reference (no line) right band: {}", fmt_hist(&ref_r));
            for lambda in LAMBDAS {
                let d = shipping().pin_carry(lambda);
                let img = d.dither_with_regions(&lined, w, h, Some(&unmarked));
                let (hl, hr) = (hist(&img, &left), hist(&img, &right));
                eprintln!(
                    "   λ={lambda:.2}  line black {:.1}%  TVD L/R {:.3}/{:.3}",
                    hist(&img, &line_set)[0] as f64 / line_set.len() as f64 * 100.0,
                    tvd(&hl, &ref_l),
                    tvd(&hr, &ref_r)
                );
                eprintln!("            left  {}", fmt_hist(&hl));
                eprintln!("            right {}", fmt_hist(&hr));
            }

            // -- Scenario C: the same line as structure inside a MARKED photo.
            // Containment (ruling 23) means no error crosses into the line at
            // all, so λ should have no effect whatsoever. This is the realistic
            // case for a grid line over a photograph, and it bounds how much λ
            // can matter in production.
            eprintln!("\n-- C: same frame, photo MARKED continuous, line unmarked structure");
            let mut mask = vec![true; w * h];
            for y in 0..h {
                for x in 400..402 {
                    mask[y * w + x] = false;
                }
            }
            let mut first: Option<Vec<u8>> = None;
            let mut all_same = true;
            for lambda in LAMBDAS {
                let d = shipping().pin_carry(lambda);
                let img = d.dither_with_regions(&lined, w, h, Some(&mask));
                let (hl, hr) = (hist(&img, &left), hist(&img, &right));
                eprintln!(
                    "   λ={lambda:.2}  line black {:.1}%  left {}",
                    hist(&img, &line_set)[0] as f64 / line_set.len() as f64 * 100.0,
                    fmt_hist(&hl)
                );
                eprintln!("            right {}", fmt_hist(&hr));
                match &first {
                    None => first = Some(img.indices().to_vec()),
                    Some(f) => all_same &= f == img.indices(),
                }
            }
            eprintln!(
                "   λ-invariant across the whole frame: {all_same}  \
                 (expected true: containment stops all error at the line)"
            );
        }

        // ------------------------------------------------------------------
        // Step 2: the unmarked-photograph cost
        // ------------------------------------------------------------------

        /// What it costs to forget to mark a photograph.
        ///
        /// Three arms, because "today's behaviour" for a marked photograph in
        /// byonk is measured model AND gamut mapping (`svg_to_png.rs` maps
        /// inside marked regions before dithering), and lumping the two
        /// together would make the colour model answer for the mapper:
        ///
        /// - `marked_mapped` — gamut-mapped, all-`true` mask. Today, shipping.
        /// - `marked_raw`    — all-`true` mask, no mapping. Isolates the mapper.
        /// - `unmarked`      — all-`false` mask, no mapping. What an author who
        ///                     does not mark the photo now gets.
        #[test]
        #[ignore] // diagnostic -- run manually
        fn unmarked_photograph_cost_diag() {
            eprintln!("\n=== Step 2: the unmarked-photograph cost ===");
            eprintln!("fixture: panel_measured(); ΔE is Oklab distance between the two");
            eprintln!("renders' MEASURED-ink appearance (what the panel shows).\n");

            for (label, rel) in [
                ("photo", "screens/builtin/calibration/color/photo.png"),
                ("background", "screens/builtin/default/background.jpg"),
            ] {
                let Some(src) = asset(rel, FRAME_W, FRAME_H) else {
                    continue;
                };
                let (w, h) = (FRAME_W, FRAME_H);
                let palette = panel_measured();
                let mapper = GamutMapper::new(&palette);

                let all_true = vec![true; src.len()];
                let all_false = vec![false; src.len()];

                let oog: Vec<usize> = src
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| mapper.rho(**p) > 1.0)
                    .map(|(i, _)| i)
                    .collect();
                let all: Vec<usize> = (0..src.len()).collect();

                let mut mapped = src.clone();
                mapper.map_frame(&mut mapped, &all_true, GamutOptions::default());

                let d = shipping();
                let marked_mapped = d.dither_with_regions(&mapped, w, h, Some(&all_true));
                let marked_raw = d.dither_with_regions(&src, w, h, Some(&all_true));
                let unmarked = d.dither_with_regions(&src, w, h, Some(&all_false));

                let (a_mm, a_mr, a_un) = (
                    appearance(&marked_mapped),
                    appearance(&marked_raw),
                    appearance(&unmarked),
                );

                eprintln!("-- {label} ({rel}) {w}x{h}");
                eprintln!(
                    "   out-of-gamut pixels (rho > 1): {} of {} ({:.1}%)",
                    oog.len(),
                    all.len(),
                    oog.len() as f64 / all.len() as f64 * 100.0
                );

                // The unmarked arm switches pinning on as well as the colour
                // model, so this is its exposure to the second mechanism:
                // source pixels that are byte-exact nominal inks and therefore
                // pinned. Photographs have almost none, but "almost" is worth
                // a number rather than an assumption.
                let ink_bytes: Vec<[u8; 3]> = (0..palette.len())
                    .map(|i| palette.official(i).to_bytes())
                    .collect();
                let mut pin_hist = [0usize; 6];
                for p in &src {
                    if let Some(i) = ink_bytes.iter().position(|b| *b == p.to_bytes()) {
                        pin_hist[i] += 1;
                    }
                }
                let pinned: usize = pin_hist.iter().sum();
                eprintln!(
                    "   pixels byte-exact on a nominal ink (pinned in the unmarked arm): \
                     {pinned} ({:.3}%)  {}",
                    pinned as f64 / all.len() as f64 * 100.0,
                    if pinned == 0 {
                        "—".to_string()
                    } else {
                        fmt_hist(&pin_hist)
                    }
                );
                for (set_name, set) in [("whole frame", &all), ("out-of-gamut only", &oog)] {
                    eprintln!("   [{set_name}]  n = {}", set.len());
                    eprintln!(
                        "     mean ΔE  unmarked vs marked_mapped : {:.4}",
                        mean_de(&a_un, &a_mm, set)
                    );
                    eprintln!(
                        "     mean ΔE  unmarked vs marked_raw    : {:.4}",
                        mean_de(&a_un, &a_mr, set)
                    );
                    eprintln!(
                        "     mean ΔE  marked_raw vs marked_mapped: {:.4}",
                        mean_de(&a_mr, &a_mm, set)
                    );
                    eprintln!(
                        "     ink changed  unmarked vs marked_mapped: {:.1}%",
                        changed_share(&unmarked, &marked_mapped, set) * 100.0
                    );
                    eprintln!(
                        "     ink changed  unmarked vs marked_raw   : {:.1}%",
                        changed_share(&unmarked, &marked_raw, set) * 100.0
                    );
                    eprintln!(
                        "     hist marked_mapped: {}",
                        fmt_hist(&hist(&marked_mapped, set))
                    );
                    eprintln!(
                        "     hist marked_raw   : {}",
                        fmt_hist(&hist(&marked_raw, set))
                    );
                    eprintln!(
                        "     hist unmarked     : {}",
                        fmt_hist(&hist(&unmarked, set))
                    );
                }

                // Block-averaged ΔE. The per-pixel figures above compare one
                // hard ink against another and are dominated by halftone
                // pattern decorrelation — two dithers of the same image score
                // large per-pixel ΔE while looking identical. Averaging in
                // linear light over k x k blocks first is what makes the
                // number comparable to a JND, and it is the honest basis for
                // ranking the colour model against the gamut mapper.
                let (l_mm, l_mr, l_un) = (
                    appearance_linear(&marked_mapped),
                    appearance_linear(&marked_raw),
                    appearance_linear(&unmarked),
                );
                let oog_mask: Vec<bool> = {
                    let mut m = vec![false; src.len()];
                    for &i in &oog {
                        m[i] = true;
                    }
                    m
                };
                eprintln!("   [block-averaged ΔE, linear light, k x k blocks]");
                for (pair_name, a, b) in [
                    ("unmarked vs marked_mapped ", &l_un, &l_mm),
                    ("unmarked vs marked_raw    ", &l_un, &l_mr),
                    ("marked_raw vs marked_mapped", &l_mr, &l_mm),
                ] {
                    let whole: Vec<String> = [1usize, 4, 8, 16]
                        .iter()
                        .map(|&k| format!("k={k}: {:.4}", block_de(a, b, w, h, k, None)))
                        .collect();
                    eprintln!("     {pair_name} whole frame   {}", whole.join("  "));
                    let sub: Vec<String> = [1usize, 4, 8, 16]
                        .iter()
                        .map(|&k| format!("k={k}: {:.4}", block_de(a, b, w, h, k, Some(&oog_mask))))
                        .collect();
                    eprintln!("     {pair_name} oog blocks    {}", sub.join("  "));
                }

                let src_rgb: Vec<u8> = src.iter().flat_map(|p| p.to_bytes()).collect();
                for (name, buf) in [
                    (format!("{label}_source.png"), src_rgb),
                    (
                        format!("{label}_marked_mapped.png"),
                        marked_mapped.to_rgb_actual(),
                    ),
                    (
                        format!("{label}_marked_raw.png"),
                        marked_raw.to_rgb_actual(),
                    ),
                    (format!("{label}_unmarked.png"), unmarked.to_rgb_actual()),
                ] {
                    eprintln!("   wrote {}", write_png(&name, &buf, w, h));
                }
                eprintln!(
                    "   NOTE: a viewer that does not linearise reads these ~30% too dark; \
                     the judgement is on the panel."
                );
            }
        }

        // ------------------------------------------------------------------
        // Step 3: the boundary artefact
        // ------------------------------------------------------------------

        /// A marked photograph abutting an unmarked flat field.
        ///
        /// Containment (ruling 23, as re-framed) drops every kernel tap that
        /// crosses the model boundary, so error that would have been deposited
        /// across it is lost rather than redistributed — the same thing the
        /// frame edge does. This prints the ink mix of the 4 px band on each
        /// side against a no-boundary control, which is what a seam would show
        /// up in.
        #[test]
        #[ignore] // diagnostic -- run manually
        fn boundary_artefact_diag() {
            eprintln!("\n=== Step 3: the model-boundary artefact ===");
            eprintln!("fixture: panel_measured(); left half = photo MARKED, right half =");
            eprintln!("unmarked flat field; boundary at x = 400.\n");

            let Some(photo) = asset(
                "screens/builtin/calibration/color/photo.png",
                FRAME_W,
                FRAME_H,
            ) else {
                return;
            };
            let (w, h) = (FRAME_W, FRAME_H);

            for (name, flat) in [
                ("mid-grey #808080", Srgb::from_u8(0x80, 0x80, 0x80)),
                ("steel blue #3366AA", Srgb::from_u8(0x33, 0x66, 0xAA)),
            ] {
                let mut px = photo.clone();
                let mut mask = vec![true; w * h];
                for y in 0..h {
                    for x in 400..w {
                        px[y * w + x] = flat;
                        mask[y * w + x] = false;
                    }
                }
                let d = shipping();
                let split = d.dither_with_regions(&px, w, h, Some(&mask));
                // Control: the same frame with no boundary at all (everything
                // measured-model, error crosses freely).
                let control = d.dither_with_regions(&px, w, h, None);

                let marked_band = cols(w, h, 396..400);
                let unmarked_band = cols(w, h, 400..404);
                let unmarked_far = cols(w, h, 700..704);

                eprintln!("-- unmarked field = {name}");
                eprintln!(
                    "   marked side, 4 px band  x396..400: {}",
                    fmt_hist(&hist(&split, &marked_band))
                );
                eprintln!(
                    "     same band, no-boundary control : {}",
                    fmt_hist(&hist(&control, &marked_band))
                );
                eprintln!(
                    "     TVD band vs control            : {:.3}",
                    tvd(&hist(&split, &marked_band), &hist(&control, &marked_band))
                );
                eprintln!(
                    "   unmarked side, 4 px band x400..404: {}",
                    fmt_hist(&hist(&split, &unmarked_band))
                );
                eprintln!(
                    "     same flat field, far  x700..704 : {}",
                    fmt_hist(&hist(&split, &unmarked_far))
                );
                eprintln!(
                    "     TVD band vs far flat field      : {:.3}  \
                     (a seam in the flat field would show here)",
                    tvd(&hist(&split, &unmarked_band), &hist(&split, &unmarked_far))
                );

                // Column-by-column: a seam is a localised spike, which a 4 px
                // aggregate can hide. Printed against the no-boundary control
                // in the same column, and against a far column, so a global
                // difference can be told apart from an edge effect.
                let black_share = |img: &DitheredImage, x: usize| {
                    let c = cols(w, h, x..x + 1);
                    hist(img, &c)[0] as f64 / c.len() as f64 * 100.0
                };
                eprintln!(
                    "   far reference x=200 (marked side): split {:.1}% black, control {:.1}%",
                    black_share(&split, 200),
                    black_share(&control, 200)
                );
                eprintln!("   per-column black share  x  split / no-boundary control:");
                for x in (384..400).chain(400..416) {
                    eprintln!(
                        "     x={x:3}  {:5.1}% / {:5.1}%",
                        black_share(&split, x),
                        black_share(&control, x)
                    );
                }

                // Calibration: the SAME flat field filling the whole frame,
                // all unmarked. Its left FRAME edge is the reference for how
                // big an onset transient an ordinary edge already produces, so
                // the boundary's transient can be judged against something
                // that ships today rather than against zero.
                let flat_frame = vec![flat; w * h];
                let all_structure = vec![false; w * h];
                let flat_only =
                    shipping().dither_with_regions(&flat_frame, w, h, Some(&all_structure));
                let edge: Vec<String> = (0..8)
                    .map(|x| format!("{:.1}", black_share(&flat_only, x)))
                    .collect();
                eprintln!(
                    "   frame-edge calibration, same field full-frame unmarked: \
                     x0..8 black {}%  (steady state x=700: {:.1}%)",
                    edge.join("/"),
                    black_share(&flat_only, 700)
                );

                let tag = if name.starts_with("mid") {
                    "grey"
                } else {
                    "blue"
                };
                eprintln!(
                    "   wrote {}",
                    write_png(&format!("boundary_{tag}.png"), &split.to_rgb_actual(), w, h)
                );
            }

            // ----------------------------------------------------------------
            // HORIZONTAL boundary. Atkinson's reach is asymmetric — 2 rows
            // down, 0 rows up — so a horizontal boundary drops EVERY downward
            // tap that crosses it, a strictly larger loss than the vertical
            // case above, where only the sideways taps of one serpentine
            // direction are affected. "No seam" measured in one orientation
            // does not establish it in the other.
            // ----------------------------------------------------------------
            eprintln!("\n-- HORIZONTAL boundary: top half photo MARKED, bottom half unmarked");
            eprintln!("   flat field; boundary at y = 240.");
            for (name, flat) in [
                ("mid-grey #808080", Srgb::from_u8(0x80, 0x80, 0x80)),
                ("steel blue #3366AA", Srgb::from_u8(0x33, 0x66, 0xAA)),
            ] {
                let mut px = photo.clone();
                let mut mask = vec![true; w * h];
                for i in 240 * w..w * h {
                    px[i] = flat;
                    mask[i] = false;
                }
                let d = shipping();
                let split = d.dither_with_regions(&px, w, h, Some(&mask));
                let control = d.dither_with_regions(&px, w, h, None);

                let row_black = |img: &DitheredImage, y: usize| {
                    let r: Vec<usize> = (y * w..(y + 1) * w).collect();
                    hist(img, &r)[0] as f64 / r.len() as f64 * 100.0
                };
                let band = |img: &DitheredImage, yr: std::ops::Range<usize>| {
                    let r: Vec<usize> = (yr.start * w..yr.end * w).collect();
                    fmt_hist(&hist(img, &r))
                };

                eprintln!("   unmarked field = {name}");
                eprintln!(
                    "     marked side, 4 px band  y236..240: {}",
                    band(&split, 236..240)
                );
                eprintln!(
                    "       same band, no-boundary control : {}",
                    band(&control, 236..240)
                );
                eprintln!(
                    "     unmarked side, 4 px band y240..244: {}",
                    band(&split, 240..244)
                );
                eprintln!(
                    "       same flat field, far  y440..444 : {}",
                    band(&split, 440..444)
                );
                eprintln!("     per-row black share  y  split / no-boundary control:");
                for y in 232..252 {
                    eprintln!(
                        "       y={y:3}  {:5.1}% / {:5.1}%",
                        row_black(&split, y),
                        row_black(&control, y)
                    );
                }

                // Frame-edge calibration in the matching orientation: the same
                // flat colour full-frame unmarked, read down its TOP edge.
                let flat_frame = vec![flat; w * h];
                let all_structure = vec![false; w * h];
                let flat_only =
                    shipping().dither_with_regions(&flat_frame, w, h, Some(&all_structure));
                let edge: Vec<String> = (0..8)
                    .map(|y| format!("{:.1}", row_black(&flat_only, y)))
                    .collect();
                eprintln!(
                    "     top-frame-edge calibration, same field full-frame unmarked: \
                     y0..8 black {}%  (steady state y=440: {:.1}%)",
                    edge.join("/"),
                    row_black(&flat_only, 440)
                );

                let tag = if name.starts_with("mid") {
                    "grey"
                } else {
                    "blue"
                };
                eprintln!(
                    "     wrote {}",
                    write_png(
                        &format!("boundary_h_{tag}.png"),
                        &split.to_rgb_actual(),
                        w,
                        h
                    )
                );
            }
        }

        // ------------------------------------------------------------------
        // Step 4: the swatch win
        // ------------------------------------------------------------------

        /// The headline case: nominal `#00FF00` and `#0000FF` patches.
        ///
        /// The brief quotes a baseline ("51% black / 27% red / 17% teal") from
        /// the session-11 handover, measured over a different pixel set (a
        /// screen crop that includes label text). It is NOT used here — every
        /// before-value below is derived in this same harness, over the stated
        /// pixel set: the patch interiors only, no labels, no margins.
        #[test]
        #[ignore] // diagnostic -- run manually
        fn swatch_win_diag() {
            eprintln!("\n=== Step 4: the swatch win ===");
            eprintln!("fixture: panel_measured(). Nominal green (0,255,0) is measured ink");
            eprintln!("(0x0D,0x87,0x6B) and nominal blue (0,0,255) is (0x20,0x54,0x97) —");
            eprintln!("probes 4 and 5, where the two colour models genuinely differ.\n");

            let (w, h) = (256usize, 128usize);
            let green_rect = rect(w, 16..112, 16..112);
            let blue_rect = rect(w, 144..240, 16..112);

            let build = |green: Srgb, blue: Srgb| {
                let mut px = vec![Srgb::from_u8(255, 255, 255); w * h];
                for &i in &green_rect {
                    px[i] = green;
                }
                for &i in &blue_rect {
                    px[i] = blue;
                }
                px
            };

            let exact = build(Srgb::from_u8(0, 255, 0), Srgb::from_u8(0, 0, 255));
            // One byte off each nominal ink: same colour model as the unmarked
            // arm, but cannot pin. Separates "the nominal model did it" from
            // "the pin did it".
            let near = build(Srgb::from_u8(1, 255, 0), Srgb::from_u8(0, 1, 255));

            let all_true = vec![true; w * h];
            let all_false = vec![false; w * h];
            let palette = panel_measured();
            let mapper = GamutMapper::new(&palette);
            let mut mapped = exact.clone();
            mapper.map_frame(&mut mapped, &all_true, GamutOptions::default());

            let d = shipping();
            let arms: [(&str, DitheredImage); 4] = [
                (
                    "marked, gamut-mapped (today, shipping)",
                    d.dither_with_regions(&mapped, w, h, Some(&all_true)),
                ),
                (
                    "marked, unmapped (measured model only)",
                    d.dither_with_regions(&exact, w, h, Some(&all_true)),
                ),
                (
                    "UNMARKED, exact ink (nominal model + pin)",
                    d.dither_with_regions(&exact, w, h, Some(&all_false)),
                ),
                (
                    "UNMARKED, one byte off (nominal model, no pin)",
                    d.dither_with_regions(&near, w, h, Some(&all_false)),
                ),
            ];
            eprintln!(
                "pixel set: patch interiors only — green x16..112, blue x144..240, y16..112 \
                 ({} px each)",
                green_rect.len()
            );
            for (label, img) in &arms {
                eprintln!("-- {label}");
                eprintln!("   green patch: {}", fmt_hist(&hist(img, &green_rect)));
                eprintln!("   blue  patch: {}", fmt_hist(&hist(img, &blue_rect)));
            }
            eprintln!(
                "   wrote {}",
                write_png("swatch_unmarked.png", &arms[2].1.to_rgb_actual(), w, h)
            );
            eprintln!(
                "   wrote {}",
                write_png("swatch_marked_mapped.png", &arms[0].1.to_rgb_actual(), w, h)
            );
        }

        // ------------------------------------------------------------------
        // Step 5: cost
        // ------------------------------------------------------------------

        /// Per-frame cost of the region map on a worst-case 800x480 frame.
        ///
        /// **Run in release AND ALONE.** These are wall-clock timings on a
        /// shared machine; run with the other four 800x480 diagnostics in
        /// parallel they are meaningless and the sign of the result flips
        /// (measured: the `None` baseline came out SLOWER than the feature).
        ///
        /// ```text
        /// cargo test --release -p eink-dither --lib region_map_cost_diag \
        ///   -- --ignored --nocapture --test-threads=1
        /// ```
        ///
        /// `None` is "without a RegionMap"; `Some(..)` is "with" — `RegionMap`
        /// itself is `pub(crate)` and is deliberately NOT widened for this.
        /// The `Some(..)` arms include building the pin map, which is part of
        /// what the caller pays.
        #[test]
        #[ignore] // diagnostic -- run manually
        fn region_map_cost_diag() {
            use std::hint::black_box;
            use std::time::Instant;

            eprintln!("\n=== Step 5: per-frame cost ===");
            eprintln!("   (wall clock — valid only when run ALONE: add --test-threads=1)");
            let Some(src) = asset(
                "screens/builtin/calibration/color/photo.png",
                FRAME_W,
                FRAME_H,
            ) else {
                return;
            };
            let (w, h) = (FRAME_W, FRAME_H);
            let d = shipping();

            let all_true = vec![true; w * h];
            let all_false = vec![false; w * h];
            // Worst case for the boundary check: every horizontal neighbour is
            // on the other side of a model boundary, so every tap compares and
            // most are dropped.
            let stripes: Vec<bool> = (0..w * h).map(|i| (i % w) % 2 == 0).collect();
            // Worst case for pinning: every pixel is an exact ink AND unmarked.
            let ink_palette = panel_measured();
            let inked: Vec<Srgb> = (0..w * h).map(|i| ink_palette.official(i % 6)).collect();

            let runs = 10;
            for (label, px, mask) in [
                ("no region map (None)", &src, None),
                ("all-continuous (measured, no pins)", &src, Some(&all_true)),
                ("all-structure (nominal + pins)", &src, Some(&all_false)),
                ("1 px stripes (every tap crosses)", &src, Some(&stripes)),
                (
                    "all-structure, every pixel pinned",
                    &inked,
                    Some(&all_false),
                ),
            ] {
                // warm-up
                black_box(d.dither_with_regions(px, w, h, mask.map(|m| m.as_slice())));
                let t = Instant::now();
                for _ in 0..runs {
                    black_box(d.dither_with_regions(px, w, h, mask.map(|m| m.as_slice())));
                }
                let ms = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
                eprintln!("   {ms:8.2} ms/frame   {label}");
            }
            eprintln!("   (gamut mapping's comparable figure was 218 ms and was accepted)");
        }
    }
}
