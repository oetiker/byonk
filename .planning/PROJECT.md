# Byonk E-ink Color Rendering Fix

## What This Is

Byonk is a self-hosted content server for TRMNL e-ink devices, using Lua scripts for data fetching and SVG templates for rendering. The `eink-dither` crate handles converting full-color rendered images to the limited palette of e-ink displays. This project focuses on fixing the color perception pipeline in `eink-dither` so that multi-color e-ink devices (e.g., 6-color BWRGBY) produce visually correct renders.

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

### Active

- [ ] Perceptually correct nearest-palette-color matching (colors map to the right palette entry)
- [ ] Correct color space usage throughout the dithering pipeline (distance calculations, error diffusion)
- [ ] Documented color science rationale for every conversion and distance calculation
- [ ] Visual correctness verified against reference images on a BWRGBY palette
- [ ] Generic palette support maintained (not hardcoded to any specific device palette)

### Out of Scope

- SVG-to-PNG rendering pipeline changes — upstream of dithering, not the source of the problem
- New dithering algorithms — fix the existing ones first
- Device communication or API changes — unrelated to the rendering issue
- Performance optimization — correctness first

## Context

- The `eink-dither` crate at `crates/eink-dither/` contains color space conversions (OKLab, linear RGB, sRGB), dithering algorithms (Atkinson, Floyd-Steinberg, JJN, Sierra, blue noise), and palette matching logic.
- The code was written with confidence that the color science is correct — comments affirm this — but the rendered output shows colors mapped to obviously wrong palette entries.
- The likely root causes are: wrong color space for distance calculations, incorrect gamma handling in conversions, or error diffusion accumulating in a non-perceptual space.
- This is a domain where subtle implementation bugs (e.g., computing Euclidean distance in sRGB instead of OKLab, or applying gamma correction twice) produce dramatically wrong visual results.
- E-ink color palettes are extremely limited (6-7 colors) which amplifies any error in color matching — there's no room for "close enough."

## Constraints

- **Crate boundary**: Fixes must stay within `crates/eink-dither/` — the rendering integration in `src/rendering/svg_to_png.rs` calls into this crate
- **Generic palette**: The solution must work with any device-reported palette, not just BWRGBY
- **Rust**: All code in Rust, workspace member of the Byonk project
- **Correctness over speed**: Perceptual accuracy is the priority; performance is secondary

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Focus on eink-dither crate only | The SVG→RGBA rendering is correct; the problem is in palette mapping and dithering | — Pending |
| Research color science first | The code was written with wrong assumptions; we need to understand the correct approach before fixing | — Pending |
| Document all color science decisions | Prevent future regressions from well-intentioned but incorrect "fixes" | — Pending |

---
*Last updated: 2026-02-05 after initialization*
