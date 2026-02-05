# Technology Stack: Color Science for E-ink Palette Dithering

**Project:** Byonk eink-dither color rendering fix
**Researched:** 2026-02-05

## Executive Summary

The eink-dither crate's color pipeline is **architecturally sound** but has one critical design problem: the color distance metric used for nearest-palette matching does not adequately separate lightness from chrominance for 6-7 color e-ink palettes. The OKLab color space implementation and sRGB gamma conversions are correct (verified against the `palette` crate and IEC 61966-2-1). The fix requires changing the **distance metric**, not the color space conversions.

## The Correct Pipeline

```
Input (sRGB u8)
    |
[1] sRGB float (0.0-1.0)
    |
[2] Linear RGB (gamma decode via IEC 61966-2-1 piecewise)
    |
[3] OKLab (for nearest-color matching via M1 * cbrt * M2)
    |
[3a] find_nearest(): HyAB+chroma distance in OKLab space
    |
[4] Error = pixel_linear_rgb - palette_entry_linear_rgb
    (error computed and diffused in LINEAR RGB)
    |
[5] Output: palette index per pixel
```

**Key principle:** Match in perceptual space (OKLab), diffuse error in linear space (LinearRgb).

## What the Existing Code Gets Right

1. **sRGB gamma** via build-time LUT -- correct IEC 61966-2-1, verified
2. **OKLab conversion matrices** -- updated 2021-01-25 values, verified against `palette` crate
3. **Error diffusion in linear RGB** -- correct space for energy conservation
4. **Type-safe color spaces** (Srgb, LinearRgb, Oklab as separate types)
5. **Dual palette (official/actual)** -- correct for real-world e-ink calibration
6. **Exact match detection** -- bypasses dithering for palette-exact pixels
7. **HyAB distance metric** -- already implemented, needs chroma extension
8. **Precomputed palette representations** -- all three spaces stored at construction
9. **Serpentine scanning** -- eliminates directional artifacts
10. **Error clamping** -- prevents floating-point blowup

## The Problem: Distance Metric

Plain Euclidean distance in OKLab treats lightness and chrominance equally. For a 6-color palette where only 2 entries are achromatic (black, white) and 4 are chromatic (red, green, blue, yellow), mid-grey (L=0.600, a=0, b=0) is closer to red (L=0.628, a=0.225, b=0.126) than to white (L=1.0):

```
dist(grey, red)   = sqrt((0.600-0.628)^2 + (0-0.225)^2 + (0-0.126)^2) = 0.259
dist(grey, white) = sqrt((0.600-1.000)^2 + 0 + 0)                     = 0.400
```

Mathematically correct in OKLab but **perceptually wrong for discrete dithering** -- red dots on grey look terrible on e-ink.

## The Fix: HyAB + Chroma Coupling Penalty

Standard HyAB (kl*|dL| + kc*sqrt(da^2+db^2)) helps but is insufficient when a chromatic color has nearly identical lightness to a grey pixel. Adding a chroma coupling penalty fixes this:

```
distance = kl*|dL| + kc*sqrt(da^2+db^2) + kchroma*|C_pixel - C_palette|
```

Where C = sqrt(a^2 + b^2) is the chroma.

With kl=2.0, kc=1.0, kchroma=2.0:
```
dist(grey, red)   = 0.056+0.258 + 2.0*|0.000-0.258| = 0.830
dist(grey, white) = 0.800+0.000 + 2.0*|0.000-0.000| = 0.800  <-- now nearest!
```

Chromatic-to-chromatic matching is barely affected since similar chroma levels produce tiny penalties.

## Recommended Stack

| Technology | Purpose | Why |
|------------|---------|-----|
| Hand-rolled OKLab | Color space | Already correct, zero-dependency |
| Hand-rolled sRGB gamma LUT | Gamma encode/decode | Already correct, sub-LSB accuracy |
| Hand-rolled HyAB+chroma | Distance metric | Simple formula, partially implemented |
| `palette` crate (dev-dep only) | Cross-validation | Keep as test reference, not runtime |

## Files to Modify

| File | Change |
|------|--------|
| `palette/palette.rs` | Add kchroma, precompute actual_chroma, update distance() |

All dither algorithms delegate to `Palette::find_nearest()` and `Palette::distance()`, requiring **zero changes** to algorithm code.

## Confidence Assessment

| Component | Confidence | Source |
|-----------|------------|--------|
| sRGB gamma formula | HIGH | IEC 61966-2-1, cross-validated |
| OKLab matrices | HIGH | Ottosson reference, cross-validated |
| HyAB metric foundation | HIGH | Abasi et al. 2020 |
| Chroma coupling approach | MEDIUM | First principles + diagnostic data |
| Error diffusion in linear RGB | HIGH | Literature consensus |
| kchroma=2.0 default | LOW | Initial estimate, needs hardware testing |

## Sources

- [Bjorn Ottosson, "A perceptual color space for image processing"](https://bottosson.github.io/posts/oklab/)
- [Abasi et al., "Distance metrics for very large color differences" (2020)](https://onlinelibrary.wiley.com/doi/10.1002/col.22451)
- [IEC 61966-2-1 / sRGB specification](https://www.color.org/srgb.pdf)
- [palette crate 0.7.6 (Rust)](https://docs.rs/palette/latest/palette/)
- [Surma, "Ditherpunk"](https://surma.dev/things/ditherpunk/)
- [HyAB k-means for color quantization](https://30fps.net/pages/hyab-kmeans/)
