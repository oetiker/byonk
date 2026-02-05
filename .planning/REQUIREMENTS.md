# Requirements: Byonk E-ink Color Rendering Fix

**Defined:** 2026-02-05
**Core Value:** Photos dithered to a limited e-ink palette must map pixels to the perceptually correct palette color

## v1 Requirements

### Distance Metric

- [x] **DIST-01**: HyAB distance metric includes chroma coupling penalty (`kchroma * |C_pixel - C_palette|`)
- [x] **DIST-02**: Palette precomputes chroma values (`sqrt(a^2 + b^2)`) for each actual OKLab entry at construction
- [x] **DIST-03**: Default kl=2.0, kc=1.0, kchroma=10.0 produces correct grey-to-achromatic mapping on BWRGBY
- [x] **DIST-04**: Chromatic-to-chromatic matching unaffected (orange maps to nearest chromatic, not forced to B/W)

### Auto-Detection

- [x] **AUTO-01**: Crate auto-detects chromatic palettes (any entry with chroma > threshold)
- [x] **AUTO-02**: Achromatic palettes default to Euclidean; chromatic palettes default to HyAB+chroma
- [x] **AUTO-03**: Auto-detection logic moved from `svg_to_png.rs` into eink-dither crate API

### Testing

- [x] **TEST-01**: Grey gradient (0-255) on BWRGBY palette produces only B/W indices
- [x] **TEST-02**: Pure chromatic colors (red, green, blue, yellow) match their palette entries exactly
- [x] **TEST-03**: Pastel/desaturated colors map to correct chromatic entries (not forced achromatic)
- [x] **TEST-04**: Edge cases tested: brown, skin tones, dark chromatic colors
- [x] **TEST-05**: Existing domain tests continue to pass

### Documentation

- [x] **DOCS-01**: Color science rationale documented for distance metric choice (HyAB + chroma coupling)
- [x] **DOCS-02**: Pipeline diagram in crate-level documentation showing color space at each stage
- [x] **DOCS-03**: Inline comments at every color space conversion explaining why that space is used

## v2 Requirements

### Parameter Tuning

- **TUNE-01**: Optimal kchroma value validated on real e-ink hardware
- **TUNE-02**: Preprocessing defaults (saturation 1.5, contrast 1.1) re-tuned for corrected metric
- **TUNE-03**: Visual regression test suite with reference images

## Out of Scope

| Feature | Reason |
|---------|--------|
| New dithering algorithms | Fix existing ones first |
| SVG-to-PNG pipeline changes | Upstream of dithering, not the source of the problem |
| Performance optimization | 384K pixels at 7 colors is sub-millisecond; correctness first |
| AI/ML-based dithering | Overkill, non-debuggable, adds dependencies |
| Edge-aware dithering | Complex, marginal benefit; exact-match handles the key case |
| Error diffusion in OKLab | Violates energy conservation; dual-space approach is correct |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DIST-01 | Phase 1: Core Distance Metric Fix | Complete |
| DIST-02 | Phase 1: Core Distance Metric Fix | Complete |
| DIST-03 | Phase 1: Core Distance Metric Fix | Complete |
| DIST-04 | Phase 1: Core Distance Metric Fix | Complete |
| AUTO-01 | Phase 2: Auto-Detection and Edge Cases | Complete |
| AUTO-02 | Phase 2: Auto-Detection and Edge Cases | Complete |
| AUTO-03 | Phase 2: Auto-Detection and Edge Cases | Complete |
| TEST-01 | Phase 1: Core Distance Metric Fix | Complete |
| TEST-02 | Phase 1: Core Distance Metric Fix | Complete |
| TEST-03 | Phase 2: Auto-Detection and Edge Cases | Complete |
| TEST-04 | Phase 2: Auto-Detection and Edge Cases | Complete |
| TEST-05 | Phase 1: Core Distance Metric Fix | Complete |
| DOCS-01 | Phase 3: Color Science Documentation | Complete |
| DOCS-02 | Phase 3: Color Science Documentation | Complete |
| DOCS-03 | Phase 3: Color Science Documentation | Complete |

**Coverage:**
- v1 requirements: 15 total
- Mapped to phases: 15
- Unmapped: 0

---
*Requirements defined: 2026-02-05*
*Last updated: 2026-02-05 after Phase 3 completion*
