# Roadmap: Byonk E-ink Color Rendering Fix

## Overview

Fix the color perception pipeline in the `eink-dither` crate so that multi-color e-ink devices produce visually correct renders. The core bug is a missing chroma coupling penalty in the HyAB distance metric, causing grey pixels to map to chromatic palette entries. The fix is surgical (one formula in one file), followed by validation and auto-detection improvements, then documentation for maintainability.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Core Distance Metric Fix** - Add chroma coupling penalty to HyAB and verify correct palette matching
- [x] **Phase 2: Auto-Detection and Edge Cases** - Move metric selection into crate API and validate edge-case colors
- [ ] **Phase 3: Color Science Documentation** - Document rationale for every conversion and distance calculation

## Phase Details

### Phase 1: Core Distance Metric Fix
**Goal**: Dithered output maps grey pixels to achromatic palette entries and chromatic pixels to correct chromatic entries
**Depends on**: Nothing (first phase)
**Requirements**: DIST-01, DIST-02, DIST-03, DIST-04, TEST-01, TEST-02, TEST-05
**Success Criteria** (what must be TRUE):
  1. A grey gradient (0-255) dithered on a BWRGBY palette produces only black and white pixels -- no color bleed
  2. Pure red, green, blue, and yellow pixels each match their exact palette entry
  3. Orange or other off-palette chromatic colors map to the nearest chromatic palette entry, not to black or white
  4. All existing crate tests continue to pass without modification
**Plans**: 1 plan

Plans:
- [x] 01-01-PLAN.md -- Add chroma coupling penalty to HyAB distance metric with tests

### Phase 2: Auto-Detection and Edge Cases
**Goal**: The crate automatically selects the correct distance metric and handles edge-case colors correctly
**Depends on**: Phase 1
**Requirements**: AUTO-01, AUTO-02, AUTO-03, TEST-03, TEST-04
**Success Criteria** (what must be TRUE):
  1. Pastel and desaturated colors (e.g., light pink, pale blue) map to their correct chromatic palette entries, not to white
  2. Edge-case colors (brown, skin tones, dark chromatic) map to the visually closest palette entry
  3. An achromatic-only palette (B/W/grey) automatically uses Euclidean distance without caller configuration
  4. A chromatic palette (BWRGBY) automatically uses HyAB+chroma without caller configuration
**Plans**: 1 plan

Plans:
- [x] 02-01-PLAN.md -- Add auto-detection to Palette::new(), simplify svg_to_png.rs, add edge-case tests

### Phase 3: Color Science Documentation
**Goal**: A developer reading the crate can understand why each color space conversion and distance calculation exists
**Depends on**: Phase 2
**Requirements**: DOCS-01, DOCS-02, DOCS-03
**Success Criteria** (what must be TRUE):
  1. Crate-level documentation explains the HyAB + chroma coupling distance metric choice with color science rationale
  2. A pipeline diagram shows which color space is used at each stage (sRGB input, Linear RGB error diffusion, OKLab matching)
  3. Every color space conversion in the code has an inline comment explaining why that particular space is used at that point
**Plans**: TBD

Plans:
- [ ] 03-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3

| Phase | Plans Complete | Status | Completed |
|-------|---------------|--------|-----------|
| 1. Core Distance Metric Fix | 1/1 | ✓ Complete | 2026-02-05 |
| 2. Auto-Detection and Edge Cases | 1/1 | ✓ Complete | 2026-02-05 |
| 3. Color Science Documentation | 0/? | Not started | - |
