# Architecture: E-ink Color Dithering Pipeline

**Researched:** 2026-02-05
**Confidence:** HIGH (verified against codebase, color science references, `palette` crate)

## Executive Summary

The eink-dither crate has a well-structured architecture with correct color space implementations. OKLab matrices verified to 1e-6 tolerance. sRGB gamma LUTs use exact IEC 61966-2-1 formulas.

**The critical architectural issue** is in the distance metric for palette matching: plain Euclidean or even standard HyAB cannot prevent grey-to-chromatic mismatches for extremely limited palettes (6-7 colors). The fix is adding a chroma coupling penalty to the HyAB metric in `palette/palette.rs`.

## The Three Color Spaces

### sRGB (gamma-encoded)
- **Use for:** Input/output, exact byte-level comparison, storage
- **Never use for:** Arithmetic, distance calculations
- **In crate:** `Srgb` struct

### Linear RGB (gamma-decoded, physical light)
- **Use for:** Light arithmetic, contrast adjustment, error diffusion
- **Never use for:** Perceptual distance
- **In crate:** `LinearRgb` struct

### OKLab (perceptual, uniform distance)
- **Use for:** Perceptual distance, palette matching
- **Components:** L (lightness 0..1), a (green-red ~-0.5..0.5), b (blue-yellow ~-0.5..0.5)
- **In crate:** `Oklab` struct

## Correct Pipeline Stages

```
Stage 1: SVG Rasterization → sRGB RGBA (resvg, upstream)
Stage 2: Alpha Compositing → Vec<Srgb> (svg_to_png.rs)
Stage 3: Exact Match Detection → Vec<Option<u8>> (sRGB byte comparison) ✓
Stage 4: sRGB → Linear RGB → Vec<LinearRgb> (4096-entry LUT) ✓
Stage 5: Saturation Boost (Photo only) → Linear→Oklab→Oklch→Oklab→Linear ✓
Stage 6: Contrast Adjustment (Photo only) → Linear RGB scaling ✓
Stage 7: Error Diffusion Dithering ← THE FIX NEEDED HERE
Stage 8: Index Output → Vec<u8> ✓
Stage 9: Index Remapping → dedup correction ✓
Stage 10: PNG Encoding → optimized PNG bytes ✓
```

## Stage 7: What Needs to Change

The distance metric in `palette.rs::distance()` needs a chroma coupling penalty:

**Current:** HyAB = kl*|dL| + kc*sqrt(da^2+db^2)
**Correct:** HyAB+chroma = kl*|dL| + kc*sqrt(da^2+db^2) + kchroma*|C_pixel - C_palette|

Where C = sqrt(a^2 + b^2) is the chroma (colorfulness).

This prevents achromatic pixels from mapping to chromatic palette entries that happen to have similar lightness.

## Component Boundary Analysis

### Must Change

**`palette/palette.rs`**
- Add `kchroma` parameter to `DistanceMetric::HyAB`
- Precompute `actual_chroma: Vec<f32>` at palette construction
- Update `distance()` to include chroma coupling penalty
- Add auto-detection: chromatic palettes → HyAB+chroma default

### Must NOT Change

- `color/oklab.rs` -- verified correct to 1e-6
- `color/srgb.rs` + `color/lut.rs` + `build.rs` -- gamma correct (IEC 61966-2-1)
- `color/linear_rgb.rs` -- simple container
- `preprocess/` -- pipeline order and spaces correct
- `dither/blue_noise.rs` -- no error diffusion, operates in Oklab for matching
- `dither/atkinson.rs`, `floyd_steinberg.rs`, `jjn.rs`, `sierra.rs` -- all delegate to `dither_with_kernel()`, no changes needed
- `dither/kernel.rs` -- weights are algorithm-specific constants

## Error Diffusion: Current Approach is Correct

The dual-space approach is the consensus best practice:
1. **Match** in Oklab (perceptual) -- find the nearest palette color
2. **Diffuse error** in Linear RGB (physical) -- conserve light energy

Why not diffuse in Oklab? Error diffusion is additive -- accumulated error is added to neighboring pixels. Addition is only physically meaningful in linear space. In Oklab, adding a/b errors can shift hue unpredictably.

## Gamma Handling Audit (All Correct)

| Conversion | Location | Correct? |
|------------|----------|----------|
| sRGB u8 → Srgb f32 | `srgb.rs::from_u8()` | YES |
| Srgb → LinearRgb | `srgb.rs::From<Srgb>` via LUT | YES |
| LinearRgb → Srgb | `srgb.rs::From<LinearRgb>` via LUT | YES |
| LinearRgb → Oklab | `oklab.rs::From<LinearRgb>` | YES |
| Oklab → LinearRgb | `oklab.rs::From<Oklab>` | YES |
| LUT generation | `build.rs` IEC 61966-2-1 f64 | YES |

## Build Order (Phases)

### Phase 1: Fix Distance Metric (Critical)
Add chroma coupling penalty to HyAB in `palette.rs`. Create comprehensive test suite for grey gradients, chromatic matching, edge cases.

### Phase 2: Validation and Tuning
Test HyAB kl/kc/kchroma parameters across reference images. Ensure existing domain tests still pass. Tune defaults.

### Phase 3: API Improvements
Move auto-detection logic from `svg_to_png.rs` into the crate. Document color science rationale.

### Phase 4: Documentation
Inline comments at every conversion point. Crate-level docs with pipeline diagram.

## Sources

- Codebase: `crates/eink-dither/src/` -- all files read and analyzed
- Bjorn Ottosson, "A perceptual color space for image processing"
- Abasi et al. 2020 -- HyAB distance metric
- IEC 61966-2-1 -- sRGB standard
- `palette` crate v0.7 -- reference implementation used in test suite
