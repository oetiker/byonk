# Phase 1: Core Distance Metric Fix - Research

**Researched:** 2026-02-05
**Domain:** Perceptual color distance metrics for e-ink palette matching (Rust, no external dependencies)
**Confidence:** HIGH

## Summary

Phase 1 adds a chroma coupling penalty to the existing HyAB distance metric in `crates/eink-dither/src/palette/palette.rs`. The change is surgical: one enum variant gains a field (`kchroma: f32`), one struct gains a precomputed vector (`actual_chroma: Vec<f32>`), and one function (`Palette::distance()`) gets an additional term in the HyAB branch. All dithering algorithms inherit the fix automatically because they call `Palette::find_nearest()`, which delegates to `Palette::distance()`.

The current HyAB implementation (kl=2.0, kc=1.0) already fails the grey-to-achromatic test -- the `test_grey_gradient_no_chromatic_noise` test produces only 41.9% achromatic pixels on a grey gradient, well below the 50% threshold. This confirms that standard HyAB alone is insufficient. The chroma coupling penalty `kchroma * |C_pixel - C_palette|` adds an asymmetric cost: when a grey pixel (C=0) is compared to a chromatic palette entry (C>0), the penalty equals `kchroma * C_palette`, pushing the match toward achromatic entries. When two chromatic colors are compared, the penalty is the difference in their chroma magnitudes, which is typically small and does not distort chromatic-to-chromatic matching.

**Primary recommendation:** Add `kchroma: f32` to `DistanceMetric::HyAB`, precompute `actual_chroma` in `Palette::new()`, and update `Palette::distance()` to include `kchroma * |C_pixel - C_palette|`. Default parameters: kl=2.0, kc=1.0, kchroma=2.0.

## Standard Stack

### Core

No new libraries are needed. The fix is pure arithmetic on existing data structures.

| Component | Location | Purpose | Why Standard |
|-----------|----------|---------|--------------|
| `Oklab` struct | `color/oklab.rs` | Perceptual color space | Verified to 1e-6 vs `palette` crate and Ottosson reference |
| `Palette` struct | `palette/palette.rs` | Color matching with distance metrics | Contains the HyAB implementation to modify |
| `DistanceMetric` enum | `palette/palette.rs` | Configures distance formula | Already has HyAB variant, add `kchroma` field |
| `Oklch` struct | `preprocess/oklch.rs` | Chroma computation reference | Shows `chroma = sqrt(a^2 + b^2)` pattern already in codebase |

### Supporting

| Library | Version | Purpose | When Used |
|---------|---------|---------|-----------|
| `palette` crate | 0.7 | Cross-validation only | Dev-dependency for Oklab verification tests |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Chroma coupling penalty on HyAB | High kc value (e.g., kc=10.0) | Works for extreme greys but distorts chromatic-to-chromatic matching -- orange would map to wrong chromatic entry |
| Chroma coupling penalty on HyAB | Separate achromatic/chromatic matching passes | More complex, breaks the single-distance-function design, harder to test |
| Precomputed `actual_chroma` vector | Inline `sqrt(a^2+b^2)` per call | Wasteful; palette chroma never changes, pixel chroma still computed inline |
| `|C_pixel - C_palette|` (abs diff) | `(C_pixel - C_palette)^2` (squared) | Squared penalizes large differences more, but abs is consistent with HyAB's Manhattan-style L term and simpler to reason about |

## Architecture Patterns

### Recommended Changes (3 files, ~30 lines total)

```
crates/eink-dither/src/
├── palette/
│   └── palette.rs        # PRIMARY: Add kchroma, precompute chroma, update distance()
├── dither/
│   └── blue_noise.rs     # SECONDARY: Update test assertions (existing test fails)
└── (no other files need changing)
```

### Pattern 1: Extend the DistanceMetric Enum

**What:** Add `kchroma: f32` field to the `HyAB` variant.

**Current:**
```rust
HyAB {
    kl: f32,
    kc: f32,
}
```

**After:**
```rust
HyAB {
    kl: f32,
    kc: f32,
    kchroma: f32,
}
```

**Why this pattern:** The enum variant already groups HyAB parameters. Adding a field is backward-compatible for code that constructs it. All match arms already destructure the variant and will get a compile error if they miss the new field, making it impossible to forget to use it.

**Impact:** Every site that constructs `DistanceMetric::HyAB { kl, kc }` must add `kchroma`. This is:
1. `palette.rs` tests (5 locations)
2. `blue_noise.rs` test (1 location)
3. `svg_to_png.rs` in byonk main crate (1 location)

The Rust compiler will enforce all sites are updated.

### Pattern 2: Precompute Palette Chroma at Construction

**What:** Add `actual_chroma: Vec<f32>` to `Palette` struct, computed once in `Palette::new()`.

**Where:** `palette.rs`, `Palette::new()` method.

**Formula:** For each Oklab entry: `chroma = sqrt(a * a + b * b)` (same formula used in `Oklch::from(Oklab)` at `preprocess/oklch.rs:77`).

**Why precompute:** Palette colors never change after construction. Computing chroma per-comparison would waste cycles in the hot loop of `find_nearest()` (called once per pixel per dither pass). Pixel chroma must be computed inline since it varies per pixel.

**Example:**
```rust
// In Palette::new(), after computing actual_oklab:
let actual_chroma: Vec<f32> = actual_oklab
    .iter()
    .map(|c| (c.a * c.a + c.b * c.b).sqrt())
    .collect();
```

### Pattern 3: Update distance() with Chroma Coupling Term

**What:** Extend the HyAB branch of `Palette::distance()` to include the chroma coupling penalty.

**Current formula:** `kl * |dL| + kc * sqrt(da^2 + db^2)`

**New formula:** `kl * |dL| + kc * sqrt(da^2 + db^2) + kchroma * |C_pixel - C_palette|`

**Key insight:** The chroma coupling term is NOT part of the standard HyAB formulation. It is a domain-specific extension for discrete palette matching. Standard HyAB (Abasi et al., 2020) separates lightness from chrominance but does not penalize chroma magnitude differences. The extension adds an asymmetric penalty that prevents achromatic-to-chromatic matching.

**Signature change:** `distance()` currently takes two `Oklab` values. To include the chroma coupling penalty, it needs the palette entry index (to look up precomputed chroma) and the pixel's chroma. Two approaches:

1. **Change `distance()` signature** to accept palette index and pixel chroma -- breaks the current symmetric `(Oklab, Oklab) -> f32` API.
2. **Compute pixel chroma inline** in `find_nearest()` and pass both chromas to `distance()` -- cleaner, keeps `distance()` focused.

**Recommended: approach 2.** Add a helper that takes `(Oklab, Oklab, f32, f32)` where the last two are pixel_chroma and palette_chroma, or compute pixel chroma once in `find_nearest()` before the loop and use the precomputed palette chroma inside the loop.

**Implementation detail:** The `distance()` method is also called directly from `blue_noise.rs::find_second_nearest()`. This function calls `palette.distance(color, palette.actual_oklab(i))`. It must also incorporate chroma coupling. The cleanest approach: make the `distance()` method accept pixel chroma as a parameter, or provide a variant like `distance_with_chroma()`.

### Anti-Patterns to Avoid

- **Do NOT add a separate matching path for achromatic pixels.** The chroma coupling penalty handles this organically. Branching on "is pixel grey?" creates edge cases (how grey is "grey"?) and makes the algorithm non-continuous.

- **Do NOT change the error diffusion space.** Error diffusion MUST remain in Linear RGB. The fix is only in the distance metric used for palette matching, not in how quantization error is computed or propagated.

- **Do NOT modify `Oklab::distance_squared()` or `Oklab::hyab_distance()`.** These are general-purpose methods on the color type. The chroma coupling is palette-specific behavior that belongs in `Palette::distance()`.

- **Do NOT make kchroma depend on the specific palette contents.** The parameter should be a fixed value that works across all chromatic palettes. Content-adaptive parameters are fragile.

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Chroma computation | New chroma() method on Oklab | Inline `sqrt(a*a + b*b)` | It's one line. Adding a method would need pub visibility decisions. The same formula already exists in `Oklch::from(Oklab)`. |
| Test palette construction | New test helpers | Reuse existing `make_6_color_palette()` in palette.rs tests | Already exists, already tested |
| Distance metric auto-selection | Detection logic in `distance()` | Keep explicit metric setting via `with_distance_metric()` | Auto-detection is Phase 2 scope. Phase 1 uses explicit configuration. |

**Key insight:** This phase has no "don't hand-roll" concerns. Everything is arithmetic on existing types. There are no external libraries to use and no complex algorithms to avoid reimplementing.

## Common Pitfalls

### Pitfall 1: Breaking the Euclidean Path

**What goes wrong:** If the `actual_chroma` field is required by `Palette` but not populated for Euclidean mode, it wastes memory or causes panics.
**Why it happens:** Rushing to add the field without considering the non-HyAB case.
**How to avoid:** Always populate `actual_chroma` in `Palette::new()` regardless of metric. It is cheap (one Vec<f32> with N entries where N is palette size, typically 2-16) and simplifies the code.
**Warning signs:** Euclidean-mode tests failing, or runtime panics on `actual_chroma[i]` access.

### Pitfall 2: Computing Pixel Chroma Inside the Inner Loop Redundantly

**What goes wrong:** `find_nearest()` calls `distance()` for each palette entry. If pixel chroma is computed inside `distance()`, it is recomputed N times per pixel (once per palette entry) even though it does not change.
**Why it happens:** Keeping the `distance()` API symmetric (takes two Oklab values).
**How to avoid:** Compute pixel chroma ONCE before the palette loop in `find_nearest()`, then pass it as a parameter.
**Warning signs:** Unnecessary sqrt operations in profiling (N times per pixel instead of 1).

### Pitfall 3: Forgetting find_second_nearest() in Blue Noise

**What goes wrong:** `find_second_nearest()` in `blue_noise.rs` calls `palette.distance()`. If `distance()` signature changes to require chroma, this function must also pass chroma.
**Why it happens:** The function is in a different file and easy to overlook.
**How to avoid:** The Rust compiler will catch this IF the `distance()` signature changes. If using a new method name, grep for all calls to the old method.
**Warning signs:** Blue noise dithering produces different results from error diffusion on the same palette.

### Pitfall 4: Incorrect Sign in Chroma Penalty

**What goes wrong:** Using `C_pixel - C_palette` (signed) instead of `|C_pixel - C_palette|` (absolute). Signed values could make the penalty negative, reducing total distance.
**Why it happens:** Confusing the penalty direction.
**How to avoid:** Always use `.abs()` on the chroma difference. The penalty must be non-negative.
**Warning signs:** Some chromatic-to-chromatic matches become worse than expected.

### Pitfall 5: Existing Test Needs Updating, Not Removal

**What goes wrong:** The `test_grey_gradient_no_chromatic_noise` test in `blue_noise.rs` currently FAILS (41.9% achromatic). After the fix, the assertion threshold must be tightened (the test expects majority achromatic but should require 100% achromatic for this grey gradient on BWRGBY).
**Why it happens:** The test was written with a weak assertion because HyAB alone was known to be insufficient.
**How to avoid:** After implementing the fix, update the test to assert 100% achromatic (all indices are 0 or 1). Also update the `test_hyab_all_greys_map_to_valid_color` test whose comment says "mid-greys near a chromatic color's lightness may map to that chromatic color" -- with chroma coupling, this should no longer happen.
**Warning signs:** Tests pass but with the old weak thresholds, providing false confidence.

### Pitfall 6: Updating svg_to_png.rs Caller

**What goes wrong:** The `build_eink_palette()` function in `src/rendering/svg_to_png.rs` constructs `DistanceMetric::HyAB { kl: 2.0, kc: 1.0 }`. After adding `kchroma`, this call site must be updated to include `kchroma: 2.0`.
**Why it happens:** The caller is in the main byonk crate, not in eink-dither.
**How to avoid:** The Rust compiler will produce a "missing field" error. Cannot compile without fixing it.
**Warning signs:** Compilation failure in the main crate.

## Code Examples

### Example 1: DistanceMetric Enum Extension

```rust
// Source: palette/palette.rs, DistanceMetric enum
HyAB {
    /// Lightness weight (Manhattan distance on L axis)
    kl: f32,
    /// Chrominance weight (Euclidean distance on a,b plane)
    kc: f32,
    /// Chroma coupling weight. Penalizes matching pixels with different
    /// chroma magnitudes (e.g., grey pixel to chromatic palette entry).
    /// Higher values force achromatic pixels to achromatic palette entries.
    kchroma: f32,
},
```

### Example 2: Precompute Chroma in Palette::new()

```rust
// Source: palette/palette.rs, Palette::new()
// After computing actual_oklab:
let actual_chroma: Vec<f32> = actual_oklab
    .iter()
    .map(|c| (c.a * c.a + c.b * c.b).sqrt())
    .collect();
```

### Example 3: Updated distance() Method

```rust
// Source: palette/palette.rs, Palette::distance()
// Note: signature changes to accept pixel_chroma and palette index
pub fn distance(&self, pixel: Oklab, palette_color: Oklab,
                pixel_chroma: f32, palette_idx: usize) -> f32 {
    match self.distance_metric {
        DistanceMetric::Euclidean => pixel.distance_squared(palette_color),
        DistanceMetric::HyAB { kl, kc, kchroma } => {
            let dl = (pixel.l - palette_color.l).abs();
            let da = pixel.a - palette_color.a;
            let db = pixel.b - palette_color.b;
            let chroma_penalty = (pixel_chroma - self.actual_chroma[palette_idx]).abs();
            kl * dl + kc * (da * da + db * db).sqrt() + kchroma * chroma_penalty
        }
    }
}
```

### Example 4: Updated find_nearest()

```rust
// Source: palette/palette.rs, Palette::find_nearest()
pub fn find_nearest(&self, color: Oklab) -> (usize, f32) {
    // Compute pixel chroma once (achromatic pixels have chroma ~0)
    let pixel_chroma = (color.a * color.a + color.b * color.b).sqrt();

    let mut best_idx = 0;
    let mut best_dist = f32::MAX;

    for (i, &palette_color) in self.actual_oklab.iter().enumerate() {
        let dist = self.distance(color, palette_color, pixel_chroma, i);
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }

    (best_idx, best_dist)
}
```

### Example 5: Test - Grey Gradient Produces Only B/W

```rust
// Source: palette/palette.rs tests
#[test]
fn test_chroma_coupling_grey_gradient_bw_only() {
    let palette = make_6_color_palette(); // Uses kl=2.0, kc=1.0, kchroma=2.0

    // Every grey from 0 to 255 must map to black (0) or white (1)
    for grey_val in 0..=255u8 {
        let grey = Oklab::from(LinearRgb::from(Srgb::from_u8(grey_val, grey_val, grey_val)));
        let (idx, _) = palette.find_nearest(grey);
        assert!(
            idx == 0 || idx == 1,
            "Grey {} mapped to index {} ({:?}), expected black or white",
            grey_val, idx, palette.official(idx).to_bytes()
        );
    }
}
```

### Example 6: Test - Chromatic Colors Match Correctly

```rust
// Source: palette/palette.rs tests
#[test]
fn test_chroma_coupling_chromatic_exact_match() {
    let palette = make_6_color_palette();

    // Each chromatic color must match its own palette entry
    let test_cases = [
        (Srgb::from_u8(255, 0, 0), 2, "red"),
        (Srgb::from_u8(0, 255, 0), 3, "green"),
        (Srgb::from_u8(0, 0, 255), 4, "blue"),
        (Srgb::from_u8(255, 255, 0), 5, "yellow"),
    ];
    for (color, expected_idx, name) in test_cases {
        let oklab = Oklab::from(LinearRgb::from(color));
        let (idx, _) = palette.find_nearest(oklab);
        assert_eq!(idx, expected_idx,
            "Pure {} should map to index {}, got {}", name, expected_idx, idx);
    }
}
```

### Example 7: Test - Off-Palette Chromatic Maps to Nearest Chromatic

```rust
// Source: palette/palette.rs tests
#[test]
fn test_chroma_coupling_orange_maps_to_chromatic() {
    let palette = make_6_color_palette();

    // Orange (off-palette) should map to a chromatic entry, not black/white
    let orange = Oklab::from(LinearRgb::from(Srgb::from_u8(255, 165, 0)));
    let (idx, _) = palette.find_nearest(orange);
    assert!(
        idx >= 2,
        "Orange should map to a chromatic entry (idx >= 2), got {}",
        idx
    );
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Euclidean in Oklab | HyAB (kl*\|dL\| + kc*sqrt(da^2+db^2)) | Already in codebase | Separates lightness from chrominance, but insufficient for discrete palettes |
| Standard HyAB | HyAB + chroma coupling (+ kchroma*\|dC\|) | This phase | Prevents achromatic-to-chromatic mismatches on limited palettes |

**Note on prior art:** The chroma coupling penalty is NOT part of the original HyAB metric (Abasi et al., 2020). It is a domain-specific extension motivated by the discrete palette matching problem on e-ink displays. Standard HyAB was designed for continuous color difference measurement where large chroma differences are already captured by the a,b Euclidean term. In discrete palette matching with very few entries, the standard HyAB term is insufficient because a chromatic palette entry with lightness close to the input grey will have a small total HyAB distance despite being perceptually inappropriate for halftone dithering.

## Open Questions

### 1. Optimal kchroma Value

- **What we know:** kchroma=2.0 is a first-principles estimate based on diagnostic Oklab values. The bug report's manual calculation shows it separates grey-from-red on the BWRGBY palette.
- **What's unclear:** Whether 2.0 works for all palette configurations (5-color, 7-color, with orange, etc.) and all content types. Higher values over-penalize and may affect desaturated chromatic colors (pastels).
- **Recommendation:** Use 2.0 as default, validate in Phase 2 with hardware. The Phase 1 test suite ensures correctness at the boundary cases (pure greys, pure chromatics), so any needed tuning in Phase 2 is parameter adjustment, not formula restructuring.

### 2. distance() API Signature Change

- **What we know:** The current `distance(Oklab, Oklab) -> f32` API is clean and symmetric. Adding chroma parameters breaks this symmetry.
- **What's unclear:** Whether to change the public API signature or add a private helper.
- **Recommendation:** Change the `distance()` method to accept `pixel_chroma` and `palette_idx` parameters. The method is `pub` but only called from within the crate (by `find_nearest()` and `blue_noise.rs::find_second_nearest()`). Alternatively, keep the public API and add a private `distance_with_chroma()` method. Either works; the planner should decide.

### 3. Interaction with Blue Noise Blend Factor

- **What we know:** `blue_noise.rs` computes a blend factor from dist1/dist2. It already handles HyAB vs Euclidean via `palette.is_euclidean()`. The chroma coupling term is added to the HyAB branch, so distances remain linear (not squared).
- **What's unclear:** Whether the blend factor behaves correctly with the added chroma penalty term. The ratio dist1/(dist1+dist2) should still produce a valid 0..1 blend factor since both distances include the same terms.
- **Recommendation:** No special handling needed. Existing `is_euclidean()` check suffices. Add a test to verify blend behavior.

## Sources

### Primary (HIGH confidence)
- **Codebase analysis** -- all source files in `crates/eink-dither/src/` read and verified
- **Test execution** -- `cargo test -p eink-dither` run, confirmed 1 failing test (`test_grey_gradient_no_chromatic_noise` at 41.9% achromatic)
- **Abasi et al., 2020** -- HyAB metric definition: [Color Research & Application](https://onlinelibrary.wiley.com/doi/abs/10.1002/col.22451)
- **Bjorn Ottosson** -- Oklab color space: [bottosson.github.io](https://bottosson.github.io/posts/oklab/)

### Secondary (MEDIUM confidence)
- **30fps.net** -- HyAB k-means for color quantization, confirms 2x lightness weight: [30fps.net/pages/hyab-kmeans](https://30fps.net/pages/hyab-kmeans/)
- **Bisqwit dithering article** -- Penalty-based approaches for palette matching in dithering: [bisqwit.iki.fi](https://bisqwit.iki.fi/story/howto/dither/jy/)
- **ColorAide documentation** -- HyAB metric reference: [facelessuser.github.io/coloraide/distance](https://facelessuser.github.io/coloraide/distance/)
- **Wikipedia** -- [Color difference](https://en.wikipedia.org/wiki/Color_difference), [Oklab color space](https://en.wikipedia.org/wiki/Oklab_color_space)

### Tertiary (LOW confidence)
- **kchroma=2.0 parameter** -- First-principles estimate from diagnostic data in `.planning/color-dither-bug.md`. Needs hardware validation (Phase 2).
- **Chroma coupling concept** -- Domain-specific extension, not from published literature. Justified by the specific problem (discrete palette matching), not by color science consensus.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, all verified against references
- Architecture: HIGH -- single formula change in one function, compile-enforced propagation
- Pitfalls: HIGH -- all 6 pitfalls identified from codebase analysis with concrete prevention strategies

**Research date:** 2026-02-05
**Valid until:** Indefinite -- color science fundamentals do not expire; parameter values may need tuning
