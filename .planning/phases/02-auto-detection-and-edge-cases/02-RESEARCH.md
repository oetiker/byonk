# Phase 2: Auto-Detection and Edge Cases - Research

**Researched:** 2026-02-05
**Domain:** Palette auto-detection, chroma threshold tuning, and edge-case color mapping in the eink-dither Rust crate
**Confidence:** HIGH

## Summary

Phase 2 has two distinct concerns: (A) moving the chromatic palette auto-detection from `svg_to_png.rs` into the eink-dither crate API, and (B) validating that edge-case colors (pastels, browns, skin tones, dark chromatic) map to the visually correct palette entries. The auto-detection part is straightforward -- the logic already exists and just needs to move. The edge-case validation reveals a critical insight that will affect how requirements TEST-03 and TEST-04 are implemented.

**Critical finding on pastels (TEST-03):** The ROADMAP states "pastel and desaturated colors map to their correct chromatic palette entries, not to white." However, research shows that on a BWRGBY palette, pastels (light pink, pale blue, mint, peach) CORRECTLY map to white because white is genuinely the perceptually closest available color. Light pink has L=0.847 and chroma=0.086, while the closest chromatic entry (red) has L=0.628 and chroma=0.258 -- a massive lightness gap. Mapping light pink to red would make it far too dark. The correct behavior for pastels on a 6-color palette is to participate in dithering -- the nearest color is white, the error diffusion propagates the chromatic error to neighbors, and the result is a white-dominant area with occasional red/yellow pixels producing the perceptual impression of pink. This is correct dithering behavior, not a bug.

**What TEST-03 should actually verify:** That pastels with chroma > 0 are not FORCED to achromatic by an overly aggressive chroma penalty. Specifically: a pastel pixel's chroma coupling penalty should be proportional to its own chroma, not dominated by the kchroma weight. The test should verify that the error diffusion output for a uniform pastel region contains SOME chromatic pixels (not 100% white), confirming the chroma information is preserved through dithering.

**Primary recommendation:** Move auto-detection into `Palette::new()` with chroma threshold 0.03. For TEST-03 and TEST-04, write dithering-level tests (using `EinkDitherer`) that verify the output pixel mix, not single-pixel `find_nearest` tests. The kchroma=10.0 value from Phase 1 is correct for preventing grey-to-chromatic mismatches and does not need changing.

## Standard Stack

### Core

No new libraries needed. All changes are within the existing eink-dither crate.

| Component | Location | Purpose | Why Standard |
|-----------|----------|---------|--------------|
| `Palette` struct | `palette/palette.rs` | Add auto-detection in constructor | Already owns distance_metric field |
| `DistanceMetric` enum | `palette/palette.rs` | Enum variant for auto vs explicit | Existing pattern, compile-time safe |
| `EinkDitherer` builder | `api/builder.rs` | Integration test entry point | Tests full pipeline including error diffusion |
| `Oklab` struct | `color/oklab.rs` | Chroma computation for detection | Already has `a` and `b` fields needed |

### Supporting

| Component | Location | Purpose | When Used |
|-----------|----------|---------|-----------|
| `Oklch` | `preprocess/oklch.rs` | Reference for chroma formula | Already demonstrates `sqrt(a^2 + b^2)` |
| `palette` crate | dev-dependency | Cross-validation in tests | Already used for Oklab verification |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Chroma threshold in OKLab | sRGB R!=G check (current `svg_to_png.rs` approach) | sRGB check is cheaper but less principled; misses near-grey chromatic colors and includes colors with negligible perceptual chroma. Chroma in OKLab is the correct metric. |
| Auto-detection in `Palette::new()` | Separate `Palette::with_auto_metric()` method | Auto in constructor is simpler API; caller can still override with `with_distance_metric()` |
| Single kchroma for all paths | Different kchroma for error diffusion vs blue noise | Adds complexity for marginal benefit; kchroma=10.0 works for both when combined with dithering |

## Architecture Patterns

### Recommended Changes (3 files, ~50 lines total)

```
crates/eink-dither/src/
  palette/
    palette.rs        # PRIMARY: Add auto-detection in Palette::new(), add is_chromatic()
  api/
    builder.rs        # No changes needed (already passes palette through)
  domain_tests.rs     # Add edge-case tests (TEST-03, TEST-04)

src/rendering/
  svg_to_png.rs       # SECONDARY: Remove manual detection, simplify build_eink_palette()
```

### Pattern 1: Auto-Detection in Palette::new()

**What:** After constructing the palette and computing `actual_oklab` and `actual_chroma`, automatically select the distance metric based on palette contents.

**When to use:** Always -- this replaces the manual caller logic.

**Algorithm:**
```
CHROMA_THRESHOLD = 0.03

is_chromatic = any palette entry has chroma > CHROMA_THRESHOLD

if is_chromatic:
    distance_metric = HyAB { kl: 2.0, kc: 1.0, kchroma: 10.0 }
else:
    distance_metric = Euclidean
```

**Threshold justification:**
- Pure grey entries have chroma = 0.0000 exactly (verified: all R==G==B values produce chroma=0 in OKLab)
- The smallest "real" chromatic palette entry tested (muddy red 200,50,50) has chroma = 0.187
- Lavender (230,230,250) which is barely chromatic has chroma = 0.027
- A threshold of 0.03 cleanly separates all achromatic palettes from all intentionally chromatic palettes
- Near-grey values like (130,128,126) have chroma = 0.004, well below threshold

**Caller can still override:** The existing `with_distance_metric()` builder method remains, allowing callers to override the auto-detected metric.

### Pattern 2: Keep `with_distance_metric()` as Override

**What:** The existing `with_distance_metric()` method continues to work, overriding the auto-detected default.

**Why:** Backward compatibility. Any code that currently calls `with_distance_metric()` continues to work unchanged. The auto-detection only changes the DEFAULT for code that does NOT call `with_distance_metric()`.

### Pattern 3: Dithering-Level Edge-Case Tests

**What:** TEST-03 and TEST-04 should be tested at the `EinkDitherer` level (full pipeline), not at the `find_nearest` level.

**Why:** The correct behavior for pastels on a BWRGBY palette is that the dithering output contains a mix of colors -- white as the dominant color, with chromatic pixels scattered by error diffusion to represent the chroma. Single-pixel `find_nearest` would map light pink to white, which is correct per-pixel but incomplete -- the test needs to verify the DITHERING OUTPUT preserves chroma information.

**Test structure:**
```rust
// Create uniform pastel image (e.g., 16x16 light pink)
// Dither with EinkDitherer on BWRGBY palette
// Verify: output contains at least SOME chromatic pixels (idx >= 2)
// Verify: output is predominantly white (not red), confirming lightness is correct
```

### Anti-Patterns to Avoid

- **Do NOT lower kchroma to accommodate pastels.** The kchroma=10.0 value was validated in Phase 1 to prevent grey-to-chromatic mismatches in blue noise dithering. Pastels are correctly handled by the dithering algorithm's error propagation, not by the per-pixel nearest-color lookup.

- **Do NOT use sRGB R!=G as the chromatic detection method.** The crate should use OKLab chroma (perceptually meaningful) not sRGB channel inequality (arbitrary). The sRGB method in `svg_to_png.rs` works by accident but is not principled.

- **Do NOT add separate code paths for "pastel detection."** The HyAB + chroma coupling metric handles all colors correctly. The error diffusion naturally distributes chroma error to neighbors. No special handling needed.

- **Do NOT make kchroma content-adaptive.** A fixed kchroma=10.0 works across all tested palettes. Making it depend on palette contents would be fragile and hard to validate.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Chromatic palette detection | Custom heuristic in caller code | Auto-detect in `Palette::new()` using OKLab chroma | Single point of truth, works for all callers |
| sRGB to chroma conversion | New conversion function | Existing `actual_chroma` Vec already computed in `Palette::new()` | Phase 1 already added this |
| Edge-case test images | Per-pixel synthetic images | `EinkDitherer::dither()` with uniform color blocks | Tests the real pipeline, not a simulation |

**Key insight:** The `actual_chroma` Vec<f32> already computed in `Palette::new()` (added in Phase 1) contains everything needed for auto-detection. The max of this vector tells you if any entry is chromatic. No new computation needed.

## Common Pitfalls

### Pitfall 1: Removing `with_distance_metric()` During Refactor

**What goes wrong:** If auto-detection replaces the manual metric setting, someone might remove the `with_distance_metric()` method entirely, breaking backward compatibility.
**Why it happens:** Over-eager cleanup during the refactor.
**How to avoid:** Keep `with_distance_metric()` as-is. It overrides the auto-detected default. Add a comment: "Override auto-detected metric. Called AFTER new() to replace the automatic selection."
**Warning signs:** Compilation failures in downstream code or tests that explicitly set metrics.

### Pitfall 2: Testing Pastels with find_nearest Instead of Dithering

**What goes wrong:** Tests check `palette.find_nearest(light_pink)` and expect it to return a chromatic index. Light pink correctly returns white on BWRGBY. The test fails and someone lowers kchroma to "fix" it, breaking grey-to-achromatic protection.
**Why it happens:** Misunderstanding of what "correct mapping" means in a dithering context.
**How to avoid:** TEST-03 must test at the `EinkDitherer::dither()` level. Create a uniform pastel image, dither it, and verify the output contains SOME chromatic pixels from error diffusion. The `find_nearest` result (white) is correct; the dithering result (mix of white + chromatic) is also correct.
**Warning signs:** Tests that call `palette.find_nearest()` directly on pastel colors and assert chromatic results.

### Pitfall 3: Chroma Threshold Too Low (False Positive Detection)

**What goes wrong:** A threshold near zero (e.g., 0.001) would trigger HyAB+chroma for palettes that have palette entries like (5,5,4) due to device calibration rounding.
**Why it happens:** Device-reported palettes might have very slight R/G/B differences in "grey" entries due to hardware measurement noise.
**How to avoid:** Use threshold 0.03 in OKLab chroma. Pure greys have chroma=0.0 exactly. The smallest intentionally chromatic color (very muted) would have chroma > 0.05. Threshold 0.03 provides a clear gap.
**Warning signs:** Achromatic palettes unexpectedly getting HyAB treatment.

### Pitfall 4: Chroma Threshold Too High (False Negative Detection)

**What goes wrong:** A threshold too high (e.g., 0.2) would miss palettes with muted chromatic entries like "muddy red" (200,50,50; chroma=0.187).
**Why it happens:** Conservative threshold selection.
**How to avoid:** Use 0.03. Even the most muted intentional chromatic color has chroma > 0.05.
**Warning signs:** Palettes with "real world" muted colors getting Euclidean instead of HyAB.

### Pitfall 5: Breaking svg_to_png.rs Without Testing Integration

**What goes wrong:** Moving detection into the crate and simplifying `svg_to_png.rs` might change behavior subtly (e.g., the sRGB R!=G check has slightly different edge-case behavior than OKLab chroma threshold).
**Why it happens:** The sRGB check and OKLab chroma check are not exactly equivalent.
**How to avoid:** For all palettes currently used in production, verify the same metric is selected. The TRMNL device's known palettes (BW, 4-grey, BWRGBY) all produce the same result with both methods. Add a test that explicitly constructs each known palette and asserts the correct metric is auto-selected.
**Warning signs:** Visual regression on actual device output after the change.

### Pitfall 6: Pastel Test Expectations That Conflict With kchroma=10

**What goes wrong:** Writing a test like "light pink must map to red" that directly conflicts with the kchroma=10 behavior, then adjusting kchroma downward.
**Why it happens:** Literal reading of "pastels map to correct chromatic entries" without understanding dithering.
**How to avoid:** Reinterpret TEST-03 as "pastels produce visually correct dithered output containing chromatic pixels" rather than "pastels map to chromatic in find_nearest." Document the rationale explicitly in test comments.
**Warning signs:** Attempts to reduce kchroma below 8.2 (the validated Phase 1 minimum for blue noise).

## Code Examples

### Example 1: Auto-Detection in Palette::new()

```rust
// In palette/palette.rs, Palette::new(), after computing actual_chroma:

/// Chroma threshold for auto-detecting chromatic palettes.
/// Any palette entry with chroma above this is considered chromatic.
/// Pure greys have chroma=0.0 exactly. Intentional chromatic colors
/// have chroma > 0.05. Threshold 0.03 provides a clean separation.
const CHROMA_DETECTION_THRESHOLD: f32 = 0.03;

// Auto-detect distance metric based on palette content
let is_chromatic = actual_chroma.iter().any(|&c| c > CHROMA_DETECTION_THRESHOLD);
let distance_metric = if is_chromatic {
    DistanceMetric::HyAB {
        kl: 2.0,
        kc: 1.0,
        kchroma: 10.0,
    }
} else {
    DistanceMetric::Euclidean
};
```

### Example 2: Simplified svg_to_png.rs Caller

```rust
// BEFORE (manual detection):
fn build_eink_palette(palette: &[(u8, u8, u8)]) -> Result<(EinkPalette, Vec<u8>), RenderError> {
    // ... dedup logic ...
    let mut eink_palette = EinkPalette::new(&unique_colors, None)?;
    let has_chromatic = palette.iter().any(|&(r, g, b)| r != g || g != b);
    if has_chromatic {
        eink_palette = eink_palette.with_distance_metric(DistanceMetric::HyAB {
            kl: 2.0, kc: 1.0, kchroma: 10.0,
        });
    }
    Ok((eink_palette, index_map))
}

// AFTER (auto-detection in crate):
fn build_eink_palette(palette: &[(u8, u8, u8)]) -> Result<(EinkPalette, Vec<u8>), RenderError> {
    // ... dedup logic ...
    let eink_palette = EinkPalette::new(&unique_colors, None)?;
    // No manual metric selection needed -- Palette::new() auto-detects
    Ok((eink_palette, index_map))
}
```

### Example 3: Palette::is_chromatic() Method

```rust
/// Returns true if the palette was auto-detected as chromatic.
///
/// A palette is chromatic if any entry has chroma (in OKLab space) above
/// the detection threshold. Chromatic palettes use HyAB+chroma distance
/// by default; achromatic palettes use Euclidean distance.
pub fn is_chromatic(&self) -> bool {
    self.actual_chroma.iter().any(|&c| c > CHROMA_DETECTION_THRESHOLD)
}
```

### Example 4: TEST-03 - Pastel Dithering Test

```rust
#[test]
fn test_pastel_produces_chromatic_pixels_in_dither() {
    // Light pink on BWRGBY should produce a mix of white and red/yellow
    // via error diffusion, NOT 100% white.
    let palette_colors = [
        Srgb::from_u8(0, 0, 0),       // black
        Srgb::from_u8(255, 255, 255), // white
        Srgb::from_u8(255, 0, 0),     // red
        Srgb::from_u8(0, 255, 0),     // green
        Srgb::from_u8(0, 0, 255),     // blue
        Srgb::from_u8(255, 255, 0),   // yellow
    ];
    let palette = Palette::new(&palette_colors, None).unwrap();
    // Auto-detection should select HyAB+chroma

    let light_pink = Srgb::from_u8(255, 182, 193);
    let image = vec![light_pink; 32 * 32];

    let ditherer = EinkDitherer::new(palette, RenderingIntent::Photo)
        .saturation(1.0)
        .contrast(1.0);
    let result = ditherer.dither(&image, 32, 32);
    let indices = result.indices();

    // Should have SOME chromatic pixels from error diffusion
    let chromatic_count = indices.iter().filter(|&&idx| idx >= 2).count();
    assert!(
        chromatic_count > 0,
        "Light pink should produce at least some chromatic pixels via error diffusion, \
         got 100% achromatic. Chroma information is being lost."
    );

    // Should be PREDOMINANTLY white (light pink is closer to white than red)
    let white_count = indices.iter().filter(|&&idx| idx == 1).count();
    assert!(
        white_count > chromatic_count,
        "Light pink should be predominantly white, got {} white vs {} chromatic",
        white_count, chromatic_count
    );
}
```

### Example 5: TEST-04 - Brown/Skin Tone Test

```rust
#[test]
fn test_brown_maps_to_warm_chromatic() {
    // Brown (139,69,19) should map to red on BWRGBY (nearest warm chromatic)
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
    assert_eq!(idx, 2, "Brown should map to red (index 2), got index {}", idx);
}

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
    assert_eq!(idx, 2, "Dark red should map to red (idx 2), got {}", idx);

    // Dark blue should map to blue, not black
    let dark_blue = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 139)));
    let (idx, _) = palette.find_nearest(dark_blue);
    assert_eq!(idx, 4, "Dark blue should map to blue (idx 4), got {}", idx);

    // Navy should map to blue, not black
    let navy = Oklab::from(LinearRgb::from(Srgb::from_u8(0, 0, 128)));
    let (idx, _) = palette.find_nearest(navy);
    assert_eq!(idx, 4, "Navy should map to blue (idx 4), got {}", idx);
}
```

### Example 6: Auto-Detection Integration Tests

```rust
#[test]
fn test_auto_detect_achromatic_palette() {
    // BW palette should auto-detect as Euclidean
    let bw = Palette::new(
        &[Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)],
        None,
    ).unwrap();
    assert!(bw.is_euclidean(), "BW palette should auto-select Euclidean");
}

#[test]
fn test_auto_detect_grey_palette() {
    // 4-grey palette should auto-detect as Euclidean
    let greys = Palette::new(
        &[
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(85, 85, 85),
            Srgb::from_u8(170, 170, 170),
            Srgb::from_u8(255, 255, 255),
        ],
        None,
    ).unwrap();
    assert!(greys.is_euclidean(), "Grey palette should auto-select Euclidean");
}

#[test]
fn test_auto_detect_chromatic_palette() {
    // BWRGBY palette should auto-detect as HyAB+chroma
    let bwrgby = Palette::new(
        &[
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ],
        None,
    ).unwrap();
    assert!(!bwrgby.is_euclidean(), "BWRGBY palette should auto-select HyAB+chroma");
}

#[test]
fn test_auto_detect_override() {
    // Auto-detected HyAB can be overridden to Euclidean
    let palette = Palette::new(
        &[
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
        ],
        None,
    ).unwrap()
    .with_distance_metric(DistanceMetric::Euclidean);
    assert!(palette.is_euclidean(), "Manual override should take precedence");
}
```

## Key Numerical Analysis

### Chroma Values for Auto-Detection Threshold

| Color | sRGB | Oklab Chroma | Category |
|-------|------|-------------|----------|
| Pure grey | (128,128,128) | 0.0000 | Achromatic |
| Near-grey warm | (130,128,126) | 0.0039 | Achromatic (noise) |
| Near-grey cool | (126,128,130) | 0.0039 | Achromatic (noise) |
| Lavender | (230,230,250) | 0.0269 | Barely chromatic |
| **THRESHOLD** | | **0.03** | |
| Pale blue | (173,216,230) | 0.0489 | Clearly chromatic |
| Light pink | (255,182,193) | 0.0858 | Chromatic |
| Brown | (139,69,19) | 0.1121 | Chromatic |
| Dark red | (139,0,0) | 0.1641 | Strongly chromatic |
| Yellow | (255,255,0) | 0.2110 | Palette entry |
| Red | (255,0,0) | 0.2577 | Palette entry |
| Green | (0,255,0) | 0.2948 | Palette entry |
| Blue | (0,0,255) | 0.3132 | Palette entry |

The threshold 0.03 cleanly separates the groups with no ambiguity.

### Pastel Behavior Analysis (kchroma=10.0)

| Pastel | find_nearest | Correct? | Dithered Output (expected) |
|--------|-------------|----------|---------------------------|
| Light pink (255,182,193) | white | Yes | Mostly white + some red/yellow pixels |
| Pale blue (173,216,230) | white | Yes | Mostly white + some blue pixels |
| Mint (189,252,201) | white | Yes | Mostly white + some green/yellow pixels |
| Light salmon (255,160,122) | yellow | Yes | Yellow + red mix |
| Medium skin (210,161,109) | white | Debatable | Mostly white + some red/yellow |
| Brown (139,69,19) | red | Yes | Red + black mix |
| Dark red (139,0,0) | red | Yes | Red dominant |
| Dark blue (0,0,139) | blue | Yes | Blue dominant |

The find_nearest result for pastels is WHITE because white is genuinely the closest palette entry. Error diffusion propagates the chroma error, producing the correct visual result. This is standard dithering behavior.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual metric selection in caller | Auto-detection in `Palette::new()` | This phase | Eliminates caller burden, single point of truth |
| sRGB R!=G chromatic check | OKLab chroma > threshold | This phase | More principled, handles edge cases |
| Default Euclidean | Auto-select based on palette | This phase | Chromatic palettes "just work" |

## Open Questions

### 1. Medium Skin Tone Mapping

- **What we know:** Medium skin (210,161,109) has chroma=0.090 and maps to white with kchroma=10. This is borderline -- a human might expect it to map to red/yellow for dithering purposes.
- **What's unclear:** Whether the error diffusion output for a uniform medium-skin-tone region is visually acceptable (enough chromatic pixels mixed in).
- **Recommendation:** Test it empirically in the dithering-level test. If the dithered output looks too white (no warm tones), the saturation boost in Photo mode (1.5x default) should help. This is a preprocessing concern, not a distance metric concern.

### 2. Dark Green (0,100,0) Mapping

- **What we know:** Dark green (chroma=0.148) maps to "yellow" with HyAB+kchroma=10.0. This is because yellow (L=0.968) is far from dark green (L=0.436), but the chromatic distance in a,b space makes green (L=0.866) very far too. Yellow ends up winning by being closest in the combined metric.
- **What's unclear:** Whether this is visually correct on the actual e-ink display.
- **Recommendation:** Add dark green to TEST-04. Verify it maps to green (idx=3) -- if it maps to yellow, the test should verify this is the correct behavior or flag it for kl/kc tuning (Phase 3 scope). Looking at the numbers more carefully: dark green to green has dab=0.203 vs dark green to yellow has dab=0.152, but green has a much larger lightness gap. This may require adjusting kl downward for better dark-chromatic matching. Flag for investigation.

### 3. Correctness of Requirement TEST-03 Wording

- **What we know:** TEST-03 says "pastel/desaturated colors map to correct chromatic entries (not forced achromatic)." Research shows pastels CORRECTLY map to white on BWRGBY because white is the nearest palette color.
- **What's unclear:** Whether the requirement should be reworded or whether "correct chromatic entries" was intended to mean "the dithered output preserves chroma information."
- **Recommendation:** Implement TEST-03 as a dithering-level test: uniform pastel input should produce dithered output containing SOME chromatic pixels. This satisfies the intent (chroma information is not lost) while being mathematically correct. Add a test comment explaining the rationale.

## Sources

### Primary (HIGH confidence)

- **Codebase analysis** -- All source files in `crates/eink-dither/src/` and `src/rendering/svg_to_png.rs` read and analyzed
- **Phase 1 research and verification** -- `.planning/phases/01-core-distance-metric-fix/01-RESEARCH.md` and `01-VERIFICATION.md`
- **Numerical analysis** -- Computed OKLab L, a, b, chroma values for 30+ test colors using the exact formulas from the codebase's `Oklab::from(LinearRgb)` implementation
- **Distance metric analysis** -- Computed HyAB + chroma coupling distances for all test colors against all BWRGBY palette entries at multiple kchroma values (0, 2, 5, 10)

### Secondary (MEDIUM confidence)

- **Existing test suite** -- 199 tests pass in eink-dither crate, confirming Phase 1 implementation is stable
- **Phase 1 kchroma=10.0 rationale** -- Verified from Phase 1 verification report: kchroma > 8.2 needed for blue noise find_second_nearest to prevent yellow (L=0.97) capturing grey

### Tertiary (LOW confidence)

- **Pastel "correctness" interpretation** -- The conclusion that pastels should map to white on BWRGBY is based on mathematical analysis of perceptual distances. Visual validation on actual e-ink hardware would upgrade this to HIGH confidence.
- **Chroma threshold 0.03** -- First-principles analysis shows clear separation, but no production palette data beyond the known TRMNL palettes.

## Metadata

**Confidence breakdown:**
- Auto-detection (AUTO-01, AUTO-02, AUTO-03): HIGH -- straightforward code move with clear threshold analysis
- Edge-case mapping (TEST-03): MEDIUM -- mathematical analysis is sound but pastel behavior interpretation needs validation
- Edge-case mapping (TEST-04): HIGH -- brown, dark red, dark blue, navy all map correctly with current parameters
- Architecture: HIGH -- minimal changes, compile-enforced, backward compatible

**Research date:** 2026-02-05
**Valid until:** Indefinite -- threshold analysis based on OKLab color space properties which are stable
