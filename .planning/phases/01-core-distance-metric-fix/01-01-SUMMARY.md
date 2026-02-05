---
phase: 01-core-distance-metric-fix
plan: 01
subsystem: rendering
tags: [oklab, hyab, chroma, dithering, eink, color-science, palette]

# Dependency graph
requires: []
provides:
  - "Chroma-coupled HyAB distance metric preventing grey-chromatic bleed"
  - "Vendored eink-dither crate with Oklab pipeline"
  - "Test suite proving grey gradient maps to B/W only on BWRGBY palette"
affects:
  - "02-integration-validation (palette rendering verification)"
  - "03-parameter-tuning (kchroma value may need hardware calibration)"

# Tech tracking
tech-stack:
  added: [eink-dither (vendored crate)]
  patterns: ["chroma coupling penalty in perceptual distance metric", "precomputed actual_chroma for O(1) lookup"]

key-files:
  created:
    - "crates/eink-dither/ (entire vendored crate)"
  modified:
    - "crates/eink-dither/src/palette/palette.rs"
    - "crates/eink-dither/src/dither/blue_noise.rs"
    - "src/rendering/svg_to_png.rs"
    - "CHANGES.md"

key-decisions:
  - "kchroma=10.0 instead of plan-specified 2.0 -- analysis showed kchroma>8.2 needed to prevent yellow (L=0.97) from capturing grey pixels via blue noise second-nearest blending"
  - "distance() signature includes pixel_chroma and palette_idx parameters for O(1) chroma lookup"
  - "actual_chroma always precomputed regardless of distance metric (cheap, simplifies code)"

patterns-established:
  - "Chroma coupling: kchroma * |C_pixel - C_palette| penalty term in HyAB distance"
  - "Pixel chroma computed once per pixel, passed to distance functions"

# Metrics
duration: 8min
completed: 2026-02-05
---

# Phase 1 Plan 1: Core Distance Metric Fix Summary

**HyAB distance metric with chroma coupling penalty (kchroma=10.0) prevents all grey-to-chromatic bleed on BWRGBY palette, verified by 256-level grey gradient and chromatic exact-match tests**

## Performance

- **Duration:** 8 min
- **Started:** 2026-02-05T11:01:48Z
- **Completed:** 2026-02-05T11:10:01Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Grey gradients (0-255) on BWRGBY palette produce ONLY black and white indices -- zero chromatic bleed
- Pure red, green, blue, and yellow pixels each match their exact palette entry
- Orange (off-palette chromatic) correctly maps to a chromatic entry, not black or white
- All 199 eink-dither tests pass (previously 1 failure in grey gradient test)
- Full project validation passes: 147 main tests, 73 Lua tests, 13 API tests, all clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Add kchroma to HyAB and update all distance calculations** - `8b52e62` (feat)
2. **Task 2: Add targeted tests and tighten existing assertions** - `9ac825c` (test)

## Files Created/Modified
- `crates/eink-dither/src/palette/palette.rs` - Added kchroma field to HyAB variant, actual_chroma precomputation, chroma coupling penalty in distance(), pixel_chroma in find_nearest(), 3 new tests, tightened existing test
- `crates/eink-dither/src/dither/blue_noise.rs` - Updated find_second_nearest() to accept pixel_chroma, compute pixel_chroma in dither loop, tightened grey gradient test to 100% achromatic
- `src/rendering/svg_to_png.rs` - Updated build_eink_palette() to use kchroma: 10.0
- `CHANGES.md` - Updated color palette dithering entry to mention chroma coupling

## Decisions Made
- **kchroma=10.0 instead of 2.0:** Analysis showed kchroma=2.0 was insufficient for the blue noise dithering path. The issue: `find_nearest()` correctly maps all greys to B/W with kchroma=2.0, but `find_second_nearest()` (used by blue noise ditherer to blend between two palette colors) could still pick a chromatic color. For yellow (L=0.97, very close to white L=1.0), a grey pixel at yellow's lightness needs kchroma>8.2 for white to beat yellow as second-nearest. Using 10.0 provides margin.
- **No impact on chromatic matching:** Higher kchroma makes chromatic pixels match chromatic entries even MORE strongly (exact color match has zero chroma penalty). Orange (off-palette) still correctly maps to red or yellow.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] kchroma=2.0 insufficient for 100% achromatic grey dithering**
- **Found during:** Task 2 (test verification)
- **Issue:** Plan specified kchroma=2.0, but blue noise grey gradient test failed at 41.9% achromatic (unchanged from baseline). Analysis: for dark greys (L=0.245), blue (L=0.452) with kchroma=2.0 penalty was still closer than white (L=1.0). For greys near yellow's lightness (L=0.97), required kchroma>8.2.
- **Fix:** Increased kchroma from 2.0 to 10.0 everywhere (palette.rs tests, blue_noise.rs test, svg_to_png.rs production code, doc examples)
- **Files modified:** crates/eink-dither/src/palette/palette.rs, crates/eink-dither/src/dither/blue_noise.rs, src/rendering/svg_to_png.rs
- **Verification:** All 199 tests pass, grey gradient is 100% achromatic
- **Committed in:** 9ac825c (Task 2 commit)

**2. [Rule 3 - Blocking] Vendored eink-dither crate needed as prerequisite**
- **Found during:** Task 1 (compilation)
- **Issue:** The eink-dither crate directory (crates/) was entirely untracked -- it was new code that had never been committed. Could not commit just the 3 modified files without the rest of the crate.
- **Fix:** Included entire crates/eink-dither/ directory plus Cargo.toml workspace changes and all integration files in Task 1 commit
- **Files modified:** 46 files (crates/eink-dither/*, Cargo.toml, Cargo.lock, src/ integration files)
- **Verification:** cargo build succeeds for both crate and main project
- **Committed in:** 8b52e62 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Bug fix was essential -- plan's kchroma=2.0 did not achieve the plan's own stated requirement of 100% achromatic grey gradients. No scope creep.

## Issues Encountered
None beyond the kchroma value adjustment documented above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Distance metric fix is complete and verified
- kchroma=10.0 parameter may need validation on physical e-ink hardware (Phase 3)
- Integration validation (Phase 2) can proceed -- all APIs stable

---
*Phase: 01-core-distance-metric-fix*
*Completed: 2026-02-05*
