# Byonk E-ink Color Rendering Fix

## What This Is

Byonk is a self-hosted content server for TRMNL e-ink devices, using Lua scripts for data fetching and SVG templates for rendering. The `eink-dither` crate handles converting full-color rendered images to the limited palette of e-ink displays using perceptually correct color matching (HyAB + chroma coupling in OKLab space) with automatic distance metric selection.

## Core Value

Photos and graphics dithered to a limited e-ink palette must map pixels to the perceptually correct palette color — the output should look right to a human eye.

## Requirements

### Validated

- ✓ SVG-to-PNG rendering pipeline works end-to-end — existing
- ✓ Device-reported color palettes parsed from hex RGB strings — existing
- ✓ Multiple dithering algorithms supported (Atkinson error diffusion, blue noise ordered dithering) — existing
- ✓ Palette-indexed PNG output with configurable palette — existing
- ✓ Dither mode selectable per-screen (photo vs graphics) — existing
- ✓ OKLab color space implementation exists in eink-dither — existing
- ✓ Linear RGB ↔ sRGB conversion exists in eink-dither — existing
- ✓ Perceptually correct nearest-palette-color matching (HyAB + chroma coupling) — v1.0
- ✓ Correct color space usage throughout the dithering pipeline — v1.0
- ✓ Documented color science rationale for every conversion and distance calculation — v1.0
- ✓ Generic palette support maintained with auto-detection — v1.0

### Active

- [ ] Optimal kchroma value validated on real e-ink hardware
- [ ] Preprocessing defaults re-tuned for corrected metric
- [ ] Visual regression test suite with reference images

### Out of Scope

- SVG-to-PNG rendering pipeline changes — upstream of dithering, not the source of the problem
- New dithering algorithms — fix the existing ones first
- Device communication or API changes — unrelated to the rendering issue
- Performance optimization — correctness first

## Context

- The `eink-dither` crate at `crates/eink-dither/` contains color space conversions (OKLab, linear RGB, sRGB), dithering algorithms (Atkinson, Floyd-Steinberg, JJN, Sierra, blue noise), and palette matching logic.
- v1.0 shipped: HyAB + chroma coupling distance metric, auto-detection of chromatic palettes, comprehensive color science documentation with pipeline diagram and inline WHY comments.
- 8,908 lines of Rust, 210 tests passing, all color science decisions documented in crate-level docs.
- E-ink color palettes are extremely limited (6-7 colors) which amplifies any error in color matching — there's no room for "close enough."
- kchroma=10.0 may need validation on physical e-ink hardware (v2 scope).

## Constraints

- **Crate boundary**: Fixes must stay within `crates/eink-dither/` — the rendering integration in `src/rendering/svg_to_png.rs` calls into this crate
- **Generic palette**: The solution must work with any device-reported palette, not just BWRGBY
- **Rust**: All code in Rust, workspace member of the Byonk project
- **Correctness over speed**: Perceptual accuracy is the priority; performance is secondary

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Focus on eink-dither crate only | The SVG→RGBA rendering is correct; the problem is in palette mapping and dithering | ✓ Good |
| HyAB + chroma coupling fix | Single formula change fixes root cause | ✓ Good |
| kchroma=10.0 (was 2.0) | 2.0 insufficient for blue noise dithering — yellow (L=0.97) too close to white (L=1.0), needed kchroma>8.2 | ✓ Good — needs hardware validation |
| Research color science first | The code was written with wrong assumptions; we need to understand the correct approach before fixing | ✓ Good |
| Document all color science decisions | Prevent future regressions from well-intentioned but incorrect "fixes" | ✓ Good |
| Auto-detection in Palette::new() | Callers shouldn't need to know about distance metrics | ✓ Good |
| CHROMA_DETECTION_THRESHOLD=0.03 | Cleanly separates achromatic (chroma=0.0) from chromatic (chroma>0.05) | ✓ Good |
| WHY comment convention at conversions | Inline rationale prevents incorrect "optimizations" | ✓ Good |

---
*Last updated: 2026-02-05 after v1.0 milestone*
