# Features: E-ink Color Dithering Pipeline

**Project:** Byonk eink-dither color rendering fix
**Researched:** 2026-02-05

## Table Stakes (Must-Have for Correct Output)

### TS-1: Perceptual Distance Metric (HyAB with Chroma Coupling)

**Status:** Partially implemented -- HyAB exists but needs chroma coupling penalty and validation.
**Why critical:** Without proper distance metric, grey pixels map to chromatic palette colors. This IS the primary bug.
**Acceptance:** Grey gradient on BWRGBY palette uses only B/W. Chromatic colors still match correctly.

### TS-2: Correct Gamma Handling (sRGB Linearization)

**Status:** CORRECT -- 4096-entry LUT, IEC 61966-2-1 standard, verified round-trip to 1 LSB.
**No changes needed.**

### TS-3: Error Diffusion in Linear RGB

**Status:** CORRECT -- error computed as `pixel.r - nearest_linear.r` in linear space.
**Rationale:** Error diffusion is about light energy conservation. Linear RGB is additive; Oklab is not.
**No changes needed.**

### TS-4: Error Clamping

**Status:** CORRECT -- configurable 0.5 default, domain-tested on 200x200 images.
**No changes needed.**

### TS-5: Serpentine Scanning

**Status:** CORRECT -- dx flipped on reverse rows.
**No changes needed.**

### TS-6: Valid Palette Indices for Any Palette Size

**Status:** CORRECT -- tested across palette sizes 1, 2, 3, 5, 7, 16.
**No changes needed.**

### TS-7: Dual-Palette Support (Official vs Actual Colors)

**Status:** CORRECT -- matching against actual, outputting official indices.
**No changes needed.**

## Differentiators (Quality Improvements)

### D-1: Adaptive Distance Metric Selection

Auto-select Euclidean for achromatic palettes, HyAB+chroma for chromatic. Currently in consumer code (`svg_to_png.rs`), should be in the crate itself.

### D-2: Perceptual Preprocessing (Saturation/Contrast Boost)

Implemented. Photo intent: saturation 1.5, contrast 1.1 in Oklch/linear RGB. May need re-tuning after metric fix.

### D-3: Exact-Match Preservation for Graphics

Implemented. Palette-exact pixels bypass dithering and error propagation.

### D-4: Blue Noise Ordered Dithering

Implemented. 64x64 blue noise matrix, blend between two nearest colors.

### D-5: Rendering Intent Selection (Photo vs Graphics)

Implemented. Photo = Atkinson + preprocessing. Graphics = blue noise, no enhancement.

### D-6: Multiple Error Diffusion Kernels

Implemented. Atkinson (75%), Floyd-Steinberg, JJN, Sierra -- all share `dither_with_kernel()`.

### D-7: Configurable Error Clamp

Implemented. `DitherOptions::error_clamp(f32)`.

## Anti-Features (Do NOT Build)

| ID | Anti-Feature | Reason |
|----|-------------|--------|
| AF-1 | Error diffusion in Oklab | Hue drift from non-linear a/b addition; violates energy conservation |
| AF-2 | AI/ML-based dithering | Overkill, non-debuggable, adds dependencies |
| AF-3 | Hardcoded palette-specific logic | Brittle; correct distance metric solves it generically |
| AF-4 | Performance optimization before correctness | 384K pixels * 7 colors = sub-millisecond already |
| AF-5 | Spatial/edge-aware dithering | Complex, marginal benefit; exact-match handles the key case |

## MVP for This Milestone

1. **Fix TS-1**: Add chroma coupling to HyAB, validate with comprehensive test suite
2. **Verify TS-2 through TS-7** still pass (existing domain tests)
3. **Move D-1 auto-detection** from `svg_to_png.rs` into the crate API
4. **Document** color science rationale throughout

## Sources

- Codebase: `crates/eink-dither/src/` -- all modules analyzed
- Abasi et al. 2020 -- HyAB metric
- Surma, "Ditherpunk" -- error diffusion best practices
- [Beyond 6 Colors: Spectra 6-color E-Ink](https://myembeddedstuff.com/e-ink-spectra-6-color)
