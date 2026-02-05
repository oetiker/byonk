---
phase: 01-core-distance-metric-fix
verified: 2026-02-05T11:14:27Z
status: passed
score: 4/4 must-haves verified
---

# Phase 1: Core Distance Metric Fix Verification Report

**Phase Goal:** Dithered output maps grey pixels to achromatic palette entries and chromatic pixels to correct chromatic entries

**Verified:** 2026-02-05T11:14:27Z

**Status:** PASSED

**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A grey gradient (0-255) on BWRGBY palette produces only black and white palette indices | ✓ VERIFIED | Test `test_chroma_coupling_grey_gradient_bw_only` passes — iterates all 256 grey values, asserts each maps to index 0 or 1 (black/white) |
| 2 | Pure red, green, blue, and yellow pixels each match their exact palette entry | ✓ VERIFIED | Test `test_chroma_coupling_chromatic_exact_match` passes — verifies RGB(255,0,0)→idx 2, RGB(0,255,0)→idx 3, RGB(0,0,255)→idx 4, RGB(255,255,0)→idx 5 |
| 3 | Orange (off-palette chromatic) maps to a chromatic palette entry, not to black or white | ✓ VERIFIED | Test `test_chroma_coupling_orange_maps_to_chromatic` passes — RGB(255,165,0) maps to idx >= 2 (chromatic region) |
| 4 | All existing crate tests pass (195 pass, 0 fail) | ✓ VERIFIED | Full test suite: 199 tests pass, 0 fail (eink-dither); 484 total tests pass across workspace |

**Score:** 4/4 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/eink-dither/src/palette/palette.rs` | HyAB + chroma coupling distance metric, precomputed actual_chroma | ✓ VERIFIED | 817 lines; kchroma field in HyAB enum (line 49); actual_chroma Vec<f32> field (line 104); chroma penalty in distance() (line 319); kchroma: 10.0 in all usages |
| `crates/eink-dither/src/dither/blue_noise.rs` | Updated find_second_nearest with chroma-aware distance | ✓ VERIFIED | 469 lines; pixel_chroma parameter added to find_second_nearest (line 86); pixel_chroma computed in dither loop (line 137); passed to find_second_nearest (line 141) |
| `src/rendering/svg_to_png.rs` | Caller updated with kchroma: 10.0 | ✓ VERIFIED | 399 lines; kchroma: 10.0 in build_eink_palette() (line 242); applied only to chromatic palettes (has_chromatic check, line 237) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `DistanceMetric::HyAB` | kchroma field | enum variant field | ✓ WIRED | Line 49 in palette.rs: `kchroma: f32` field exists in enum variant |
| `distance()` | actual_chroma Vec<f32> | chroma coupling penalty term | ✓ WIRED | Line 319 in palette.rs: `let chroma_penalty = (pixel_chroma - self.actual_chroma[palette_idx]).abs();` Line 320: `kl * dl + kc * (da * da + db * db).sqrt() + kchroma * chroma_penalty` |
| `find_nearest()` | distance() | passes pixel_chroma computed once before loop | ✓ WIRED | Line 352 in palette.rs: `let pixel_chroma = (color.a * color.a + color.b * color.b).sqrt();` Line 359: `let dist = self.distance(color, palette_color, pixel_chroma, i);` |
| `find_second_nearest()` | palette.distance() | passes pixel_chroma and palette index | ✓ WIRED | Line 101 in blue_noise.rs: `let dist = palette.distance(color, palette.actual_oklab(i), pixel_chroma, i);` Called from dither loop (line 141) with pixel_chroma computed at line 137 |
| `build_eink_palette()` | DistanceMetric::HyAB | adds kchroma: 10.0 to constructor | ✓ WIRED | Lines 239-243 in svg_to_png.rs: `eink_palette = eink_palette.with_distance_metric(DistanceMetric::HyAB { kl: 2.0, kc: 1.0, kchroma: 10.0 });` Only applied when `has_chromatic` is true |

### Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| DIST-01: HyAB includes chroma coupling penalty | ✓ SATISFIED | kchroma field in enum (line 49), penalty term in distance() (line 319-320) |
| DIST-02: Palette precomputes chroma values | ✓ SATISFIED | actual_chroma Vec<f32> field (line 104), computed in Palette::new() (line 191-193) |
| DIST-03: Default parameters produce correct mapping | ⚠️ MODIFIED | Plan specified kchroma: 2.0, implementation uses kchroma: 10.0. Analysis during execution showed 2.0 insufficient for find_second_nearest() in blue noise dithering. Higher value ensures grey never captures yellow (L=0.97) as second-nearest. All tests pass with 10.0. |
| DIST-04: Chromatic-to-chromatic matching unaffected | ✓ SATISFIED | Orange test passes (maps to chromatic not achromatic), pure color test passes (exact matches) |
| TEST-01: Grey gradient produces B/W only | ✓ SATISFIED | test_chroma_coupling_grey_gradient_bw_only passes |
| TEST-02: Pure chromatic colors match exactly | ✓ SATISFIED | test_chroma_coupling_chromatic_exact_match passes |
| TEST-05: Existing tests pass | ✓ SATISFIED | 199 eink-dither tests pass, 484 workspace tests pass, 0 failures |

### Anti-Patterns Found

**None** — No TODO/FIXME comments, no placeholder content, no empty implementations, no console.log debugging, no stub patterns detected in any modified file.

### Human Verification Required

None — all goal criteria are programmatically verifiable via test suite.

### Implementation Notes

**kchroma Parameter Value Adjustment:**

The plan specified kchroma: 2.0, but the implementation uses kchroma: 10.0. This was determined during execution to be necessary for correct behavior:

- **Root Cause:** The blue noise dithering algorithm uses find_second_nearest() to select between two palette colors. For yellow (L=0.97, very close to white L=1.0), grey pixels at similar lightness need kchroma > 8.2 for white to consistently beat yellow as the second-nearest color.

- **Impact:** Higher kchroma (10.0 vs 2.0) makes the chroma coupling penalty stronger, providing safety margin for edge cases. Does NOT negatively affect chromatic matching — exact color matches have zero chroma penalty regardless of kchroma value.

- **Verification:** All tests pass with kchroma: 10.0, including the grey gradient test that was failing before the fix (41.9% achromatic → 100% achromatic).

**Technical Implementation Quality:**

- Chroma values precomputed once at palette construction (O(1) lookup during dithering)
- Pixel chroma computed once per pixel before distance calculations
- HyAB distance metric properly decoupled from Euclidean (no behavioral change to achromatic palettes)
- All compiler-enforced: adding kchroma to enum forces all construction sites to provide it; adding parameters to distance() forces all call sites to update

---

_Verified: 2026-02-05T11:14:27Z_
_Verifier: Claude (gsd-verifier)_
