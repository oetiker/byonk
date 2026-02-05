# Pitfalls: E-ink Color Dithering

**Researched:** 2026-02-05
**Confidence:** HIGH (each pitfall verified against codebase)

## Critical Pitfalls

### Pitfall 1: Grey-to-Chromatic Mapping (THE PRIMARY BUG)

**What goes wrong:** With Euclidean distance in OKLab, mid-grey (L=0.600) is closer to red (L=0.628, a=0.225, b=0.126, dist=0.259) than to white (L=1.0, dist=0.400). On e-ink, scattered red dots on grey look terrible.

**Visual symptoms:** Grey areas rendered with colored noise. Neutral content gets chromatic contamination.

**Current status:** HyAB metric added but standard HyAB alone is insufficient -- when a chromatic color has similar lightness to grey, the lightness term is tiny and chroma dominates.

**Fix:** Add chroma coupling penalty: `distance += kchroma * |C_pixel - C_palette|`

**How to test:**
```rust
// Grey gradient on BWRGBY should produce ONLY black/white indices
let result = dither(grey_gradient, bwrgby_palette);
assert!(result.iter().all(|&idx| idx == 0 || idx == 1));
```

### Pitfall 2: Error Diffusion in sRGB Space

**What goes wrong:** Computing quantization error in gamma-encoded sRGB causes brightness drift. sRGB values are not linearly proportional to light; adding sRGB errors shifts mid-tones bright.

**Visual symptoms:** Output is noticeably brighter than input. Gradients skew toward lighter values.

**Current status:** CORRECT -- error computed in Linear RGB.

### Pitfall 3: Palette Set Mismatch (Official vs Actual)

**What goes wrong:** Matching against official device colors while computing error against actual displayed colors (or vice versa). The mismatch means error diffusion compensates for the wrong target.

**Current status:** CORRECT -- both matching and error use `actual` colors consistently.

### Pitfall 4: Double Gamma Correction

**What goes wrong:** Gamma decode applied twice (sRGB→linear→linear-again) or not at all (sRGB bytes treated as linear).

**Visual symptoms:** Double gamma = image extremely dark, crushed shadows. Missing gamma = too bright, washed out.

**Current status:** CORRECT -- Rust type system (`Srgb` vs `LinearRgb`) prevents accidental mixing.

### Pitfall 5: Match/Error Space Mismatch

**What goes wrong:** Nearest color found in Oklab, error computed in Linear RGB -- these are different spaces. The error correction vector is in a different coordinate system than the matching function.

**Current status:** CORRECT -- this is actually the standard dual-space approach. Match in perceptual (Oklab), diffuse in physical (Linear RGB). The rationale: matching should be perceptual, but error conservation should be physical (light energy is additive in linear space).

## Moderate Pitfalls

### Pitfall 6: LUT Resolution Too Low for Dark Values

**What goes wrong:** sRGB gamma has extreme nonlinearity in darks. A 256-entry LUT loses shadow detail.

**Current status:** CORRECT -- 4096-entry LUT with linear interpolation.

### Pitfall 7: Negative LMS in OKLab (cbrt of negatives)

**What goes wrong:** Error diffusion can push pixels out of gamut, producing negative linear RGB values. If `cbrt(-x)` returns NaN (as `powf(1.0/3.0)` does), the pipeline breaks.

**Current status:** CORRECT -- Rust's `f32::cbrt()` handles negatives properly.

### Pitfall 8: Error Clamping Too Aggressive or Loose

**What goes wrong:** Without clamping: error blows up to infinity, causing "blooming." Too tight: gradients band, average brightness drifts.

**Current status:** CORRECT -- configurable 0.5 default, tested on 200x200 images.

### Pitfall 9: Serpentine Not Flipping Kernel dx

**What goes wrong:** On reverse rows, kernel offsets must be horizontally flipped. Without flip, error diffuses into already-processed pixels.

**Current status:** CORRECT -- explicit `if reverse { -dx } else { dx }`.

### Pitfall 10: Contrast Around Wrong Midpoint

**What goes wrong:** Contrast at linear 0.5 midpoint = perceptual ~73%. Shadows affected more than highlights.

**Current status:** Intentional design choice. Acceptable for e-ink preprocessing.

## Minor Pitfalls

### Pitfall 11: f32 Precision in Matrix Chains
Round-trip Oklab precision ~1e-5. Handled by detecting exact matches BEFORE preprocessing.

### Pitfall 12: Exact Match Against Wrong Colors
Must compare against actual (calibrated) colors. Current: CORRECT.

### Pitfall 13: Blue Noise Blend Factor Distance Mismatch
Must normalize Euclidean (squared) and HyAB (linear) distances. Current: CORRECT.

## Summary

| Pitfall | Severity | Status |
|---------|----------|--------|
| 1. Grey-to-chromatic | CRITICAL | **NEEDS FIX** (chroma coupling) |
| 2. Error in sRGB | CRITICAL | Correct |
| 3. Palette set mismatch | CRITICAL | Correct |
| 4. Double gamma | CRITICAL | Correct (type system) |
| 5. Match/error spaces | CRITICAL | Correct (dual-space) |
| 6. LUT resolution | MODERATE | Correct (4096+interp) |
| 7. Negative cbrt | MODERATE | Correct (Rust cbrt) |
| 8. Error clamping | MODERATE | Correct (0.5 default) |
| 9. Serpentine dx | MODERATE | Correct |
| 10. Contrast midpoint | MODERATE | Intentional |
| 11-13. Minor issues | MINOR | All correct |

**Bottom line:** The codebase is well-implemented. Only Pitfall 1 (distance metric) needs fixing. Everything else is correct.

## Sources

- Bjorn Ottosson -- OKLab specification
- Surma -- "Ditherpunk" (linearization for dithering)
- John Novak -- "What every coder should know about gamma"
- Abasi et al. 2020 -- HyAB distance metric
- Codebase: `crates/eink-dither/src/` -- all modules verified
