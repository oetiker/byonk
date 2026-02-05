# Project Research Summary

**Project:** Byonk eink-dither color rendering fix
**Domain:** E-ink color dithering / computational color science
**Researched:** 2026-02-05
**Confidence:** HIGH

## Executive Summary

The eink-dither crate has a well-architected color pipeline with correct implementations of OKLab color space, sRGB gamma handling, and error diffusion algorithms. The core bug is narrowly scoped: the distance metric used for palette matching treats lightness and chrominance equally, causing grey pixels to map to chromatic palette entries when a color like red has similar lightness. On limited 6-7 color e-ink palettes where only 2 entries are achromatic (black, white), this produces visually unacceptable results.

The fix is surgical: add a chroma coupling penalty to the existing HyAB distance metric in `palette/palette.rs`. This prevents achromatic-to-chromatic mismatches while preserving correct chromatic-to-chromatic matching. The distance formula changes from `kl*|dL| + kc*sqrt(da^2+db^2)` to `kl*|dL| + kc*sqrt(da^2+db^2) + kchroma*|C_pixel - C_palette|` where C is chroma magnitude. All existing color space conversions, error diffusion logic, and preprocessing remain untouched.

The key risk is incorrect parameter tuning (kl, kc, kchroma values). This is mitigated by comprehensive test suites covering grey gradients (must use only B/W), chromatic matching (must preserve color accuracy), and domain-validated reference images. Initial recommendation: kl=2.0, kc=1.0, kchroma=2.0, with hardware validation required before release.

## Key Findings

### Recommended Stack

The existing implementation already uses the correct stack. No new dependencies or external libraries are needed. All color science is hand-rolled with implementations verified against the `palette` crate (used as dev-dependency for cross-validation only).

**Core technologies:**
- **Hand-rolled OKLab**: Perceptual color space for palette matching — matrices verified to 1e-6 tolerance against Bjorn Ottosson's reference and `palette` crate
- **Hand-rolled sRGB gamma LUT**: 4096-entry lookup table with linear interpolation — implements IEC 61966-2-1 standard, verified round-trip to 1 LSB
- **Hand-rolled HyAB distance metric**: Separates lightness from chrominance — partially implemented, needs chroma coupling extension
- **Type-safe color spaces**: Separate `Srgb`, `LinearRgb`, `Oklab` types — prevents accidental mixing and double-gamma errors

**Critical version requirements:** None. All implementations are internal to the crate.

**Rationale for no external color libraries:** The pipeline needs precise control over gamma handling, error diffusion space selection, and distance metric customization. External libraries impose constraints (e.g., diffusing in wrong color space) that would require workarounds.

### Expected Features

The crate already has 11 of 13 features correctly implemented. Only two changes are needed:

**Must have (table stakes):**
- **TS-1: Perceptual distance metric with chroma coupling** — NEEDS FIX (primary bug)
- TS-2 through TS-7: Correct gamma, error diffusion in linear RGB, error clamping, serpentine scanning, valid indices, dual-palette support — ALL CORRECT

**Should have (differentiators):**
- **D-1: Adaptive distance metric selection** — Auto-detect achromatic vs chromatic palettes, move from consumer code into crate API
- D-2 through D-7: Perceptual preprocessing, exact-match preservation, blue noise dithering, rendering intent selection, multiple kernels, configurable clamp — ALL IMPLEMENTED

**Defer (v2+):**
- Performance optimization beyond correctness (already sub-millisecond for 384K pixels × 7 colors)
- Spatial/edge-aware dithering (complex, marginal benefit)
- AI/ML-based approaches (overkill, non-debuggable)

**Anti-features (do NOT build):**
- Error diffusion in Oklab (violates energy conservation, causes hue drift)
- Hardcoded palette-specific logic (brittle, defeats generic approach)

### Architecture Approach

The crate follows a clear pipeline: sRGB input → gamma decode to Linear RGB → perceptual preprocessing → error diffusion dithering → palette index output. Each conversion uses the correct color space for its purpose: sRGB for I/O, Linear RGB for light arithmetic and error diffusion, OKLab for perceptual distance. The dual-space approach (match in OKLab, diffuse error in Linear RGB) is industry best practice.

**Major components:**
1. **Color space conversions** (`color/oklab.rs`, `color/srgb.rs`, `color/lut.rs`) — Handle gamma and perceptual transforms. Status: CORRECT, no changes needed.
2. **Palette matching** (`palette/palette.rs`) — Finds nearest color using distance metric. Status: NEEDS CHROMA COUPLING FIX.
3. **Error diffusion algorithms** (`dither/atkinson.rs`, `floyd_steinberg.rs`, etc.) — Propagate quantization error. Status: CORRECT, all delegate to `dither_with_kernel()`.
4. **Preprocessing** (`preprocess/saturation.rs`, `preprocess/contrast.rs`) — Photo intent enhancements. Status: CORRECT.
5. **Consumer integration** (`svg_to_png.rs` in main Byonk repo) — Orchestrates pipeline. Status: Contains auto-detection logic that should move into crate.

**Key patterns:**
- Type system enforces color space boundaries (cannot accidentally mix `Srgb` and `LinearRgb`)
- Build-time LUT generation ensures bit-identical gamma across builds
- Exact match detection before preprocessing preserves palette-exact graphics
- Serpentine scanning eliminates directional artifacts

### Critical Pitfalls

Research identified 13 pitfalls; only one requires fixing.

1. **Grey-to-chromatic mapping** — CRITICAL, NEEDS FIX. Euclidean or standard HyAB distance allows grey pixels to match red/blue when lightness is similar. Fix: Add chroma coupling penalty `kchroma*|C_pixel - C_palette|` to HyAB metric. Test: Grey gradients must produce only B/W indices on BWRGBY palette.

2. **Error diffusion in sRGB space** — CRITICAL, but CORRECT. Error computed in Linear RGB (physical space) as required. Diffusing in sRGB would cause brightness drift; diffusing in Oklab would cause hue drift.

3. **Palette set mismatch (official vs actual)** — CRITICAL, but CORRECT. Matching and error computation both use `actual` (calibrated) colors consistently.

4. **Double gamma correction** — CRITICAL, but PREVENTED by type system. Separate `Srgb` and `LinearRgb` types make accidental double-decode impossible.

5. **LUT resolution too low for dark values** — MODERATE, but CORRECT. 4096-entry LUT with linear interpolation provides sub-LSB accuracy in shadow region where sRGB gamma is most nonlinear.

6. **Error clamping too aggressive or loose** — MODERATE, but CORRECT. Configurable 0.5 default tested on 200×200 domain images.

**Bottom line:** 12 of 13 pitfalls already avoided by careful implementation. Only #1 (distance metric) needs addressing.

## Implications for Roadmap

This is a single-component fix with comprehensive validation requirements. The work naturally divides into three phases.

### Phase 1: Core Distance Metric Fix
**Rationale:** The bug is in one function in one file. Fix it first, verify it works, before any API changes or documentation.

**Delivers:** Correct grey-to-chromatic separation on limited palettes.

**Addresses:**
- TS-1 (perceptual distance metric with chroma coupling)
- Pitfall #1 (grey-to-chromatic mapping)

**Avoids:**
- Pitfall #2 through #5 (all already correct, must not break)

**Work items:**
- Add `kchroma` parameter to `DistanceMetric::HyAB` enum variant
- Precompute `actual_chroma: Vec<f32>` during `Palette::new()`
- Update `Palette::distance()` to include chroma coupling term
- Add comprehensive test suite:
  - Grey gradient (L: 0.0→1.0) on BWRGBY → only indices 0/1 (B/W)
  - Chromatic colors still match correctly (red→red, not red→orange)
  - Edge case: very dark grey vs very dark red
  - Edge case: very saturated colors maintain separation

**Research flag:** SKIP. This is mathematical formula implementation with clear acceptance criteria. No ambiguity.

### Phase 2: Parameter Validation and Tuning
**Rationale:** kchroma=2.0 is an initial estimate. Real e-ink hardware has quirks (ink mixing, partial refresh artifacts). Parameters need validation against physical devices and diverse content.

**Delivers:** Confidence in default parameters across photo content, graphics, gradients.

**Uses:**
- STACK.md recommendation: kl=2.0, kc=1.0, kchroma=2.0 as starting point
- Existing domain test suite (200×200 reference images)
- Hardware validation on actual TRMNL devices

**Implements:** Validation framework from ARCHITECTURE.md

**Work items:**
- Test Phase 1 implementation on reference images from existing test suite
- Verify all existing domain tests still pass (regression check)
- Test on diverse content: photos, graphics, gradients, edge cases
- Tune kl/kc/kchroma if needed based on visual results
- Document tuning rationale and parameter sensitivity

**Research flag:** MINOR. Parameters are tuned empirically, not researched. May need iteration based on hardware feedback.

### Phase 3: API Improvements and Documentation
**Rationale:** With core fix validated, clean up API surface and document the color science for maintainers.

**Delivers:** Production-ready crate with clear documentation of color pipeline.

**Addresses:**
- D-1 (adaptive distance metric selection)
- Gap: Color science rationale not documented in code

**Work items:**
- Move auto-detection logic from `svg_to_png.rs` into `Palette::new()`
- Auto-select Euclidean for achromatic palettes (B/W/grey), HyAB+chroma for chromatic
- Add inline comments at every color space conversion point explaining why
- Write crate-level documentation with pipeline diagram
- Document distance metric selection rationale
- Add examples showing distance metric effects

**Research flag:** SKIP. This is polish and documentation, no research needed.

### Phase Ordering Rationale

- **Phase 1 before Phase 2:** Cannot validate parameters until the metric is implemented. Attempting to tune a broken formula wastes time.
- **Phase 2 before Phase 3:** API design (auto-detection, default parameters) depends on validated parameter values. Designing API before knowing correct defaults leads to breaking changes.
- **Phase 3 as final polish:** Documentation and API cleanup don't affect correctness. Can be deferred if needed, but should complete before merging to main.

**Dependency chain:**
```
Phase 1 (metric fix) → Phase 2 (validation) → Phase 3 (API/docs)
```

No phases can be parallelized. Each phase is blocked by the previous one.

### Research Flags

**Phases with standard patterns (skip research-phase):**
- **Phase 1:** Distance metric is a mathematical formula. Implementation is straightforward once the formula is defined (which research already did).
- **Phase 2:** Parameter tuning is empirical, not research-based. Run tests, look at images, adjust values. No documentation to research.
- **Phase 3:** API design and documentation follow Rust conventions. No domain-specific research needed.

**No phases need deeper research.** The project research already identified:
- The exact distance formula to implement
- The exact location to change (one function in one file)
- The exact test criteria (grey gradients must use only B/W)
- The parameter starting point (kl=2.0, kc=1.0, kchroma=2.0)

Roadmap creation can proceed directly to requirements definition without additional research phases.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All implementations verified against IEC 61966-2-1, Ottosson OKLab reference, and `palette` crate to 1e-6 tolerance |
| Features | HIGH | Codebase analysis shows 11/13 features correct, 1/13 needs fix, 1/13 is polish |
| Architecture | HIGH | Pipeline verified correct, dual-space approach is literature consensus |
| Pitfalls | HIGH | All 13 pitfalls identified, 12/13 already avoided, 1/13 is the bug being fixed |

**Overall confidence:** HIGH

### Gaps to Address

**Gap: Optimal kchroma value**
- **Impact:** Initial estimate kchroma=2.0 may need tuning
- **Resolution:** Phase 2 empirical validation on hardware
- **Risk:** LOW. If wrong, only parameter needs changing, not formula structure.

**Gap: Performance impact of chroma computation**
- **Impact:** Adding sqrt(a^2+b^2) per pixel per palette entry
- **Resolution:** Profile during Phase 2. Worst case: precompute pixel chroma once before palette loop.
- **Risk:** NEGLIGIBLE. 384K pixels × 7 colors × 1 sqrt = ~2.7M ops, well under millisecond on modern CPU.

**Gap: Interaction with preprocessing (saturation/contrast boost)**
- **Impact:** Photo intent preprocessing may need re-tuning after metric fix
- **Resolution:** Re-test preprocessing parameters during Phase 2 validation
- **Risk:** LOW. Preprocessing is optional (graphics intent skips it). Can disable if problematic.

**Gap: Edge case behavior at extreme chroma values**
- **Impact:** Very saturated colors near gamut boundary may behave unexpectedly
- **Resolution:** Add edge case tests in Phase 1 test suite
- **Risk:** LOW. E-ink palettes are not near sRGB gamut boundary.

No gaps block proceeding to roadmap creation. All can be resolved during implementation phases.

## Sources

### Primary (HIGH confidence)
- **Codebase analysis**: `crates/eink-dither/src/` — all modules read and verified
- **IEC 61966-2-1 / sRGB specification**: Gamma formula verification
- **Bjorn Ottosson, "A perceptual color space for image processing"**: OKLab definition and matrices
- **Abasi et al., "Distance metrics for very large color differences" (Color Research & Application, 2020)**: HyAB metric foundation
- **`palette` crate v0.7.6**: Cross-validation reference (used as dev-dependency in test suite)

### Secondary (MEDIUM confidence)
- **Surma, "Ditherpunk"**: Error diffusion best practices, confirms linear RGB for error diffusion
- **John Novak, "What every coder should know about gamma"**: Gamma handling principles
- **HyAB k-means for color quantization (30fps.net)**: Application of HyAB to discrete color matching

### Tertiary (LOW confidence)
- **kchroma=2.0 parameter recommendation**: First-principles estimate based on diagnostic data, needs hardware validation

---
*Research completed: 2026-02-05*
*Ready for roadmap: yes*
