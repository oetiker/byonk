# Bug Report: Color Palette Dithering Produces Wrong Results

## Summary

After integrating the `eink-dither` crate, color palette rendering (e.g., 6-color: black, white, red, green, blue, yellow) produces visually terrible results. Grey/neutral areas in images get rendered as noisy colored mess instead of clean black+white dithering. Greyscale-only palettes work perfectly.

## Root Cause

The `eink-dither` crate uses **Oklab perceptual color space** for nearest-palette-color matching (`Palette::find_nearest()` in `crates/eink-dither/src/palette/palette.rs:239`). In Oklab space, chromatic colors can be **closer** to neutral grey than black or white, because Oklab measures perceptual similarity for continuous color — not for discrete halftone dots.

## Diagnostic Proof

With a 6-color palette (green, blue, yellow, red, white, black), the Oklab distances from mid-grey (128,128,128) are:

| Palette Color | Oklab L | Oklab a | Oklab b | Chroma | dist² to grey |
|---------------|---------|---------|---------|--------|---------------|
| **red**       | 0.6280  | 0.2249  | 0.1258  | 0.2577 | **0.0672** |
| blue          | 0.4520  | -0.0325 | -0.3115 | 0.3132 | 0.1200 |
| green         | 0.8664  | -0.2339 | 0.1795  | 0.2948 | 0.1580 |
| **white**     | 1.0000  | 0.0000  | 0.0000  | 0.0000 | 0.1601 |
| yellow        | 0.9680  | -0.0714 | 0.1986  | 0.2110 | 0.1800 |
| **black**     | 0.0000  | 0.0000  | 0.0000  | 0.0000 | 0.3598 |

**Mid-grey (L=0.600) is nearest to RED (L=0.628)** because their lightness values nearly coincide (diff=0.028), and the chromatic distance (a,b axes) only adds ~0.066 to the total. Meanwhile white (L=1.0) is 2.4x further away.

Similarly: **dark-grey (64,64,64) maps to BLUE** instead of black.

## Why This Matters for E-ink

For continuous-tone displays, Oklab matching is correct — red really IS perceptually close to mid-grey. But e-ink dithering produces **discrete visible dots**. When the ditherer mixes red + blue dots to approximate grey, the eye sees noisy colored mess, not a smooth neutral tone. The spatial averaging that makes Oklab work for smooth gradients doesn't happen at e-ink's resolution.

## Affected Code Paths

### 1. `Palette::find_nearest()` — `crates/eink-dither/src/palette/palette.rs:239-253`

Uses unweighted `Oklab::distance_squared()`. This is the core matching function used by ALL dither algorithms.

```rust
pub fn find_nearest(&self, color: Oklab) -> (usize, f32) {
    for (i, &palette_color) in self.actual_oklab.iter().enumerate() {
        let dist = color.distance_squared(palette_color);  // <-- unweighted Oklab
        ...
    }
}
```

### 2. `find_second_nearest()` — `crates/eink-dither/src/dither/blue_noise.rs:81-103`

Also uses unweighted `color.distance_squared(palette.actual_oklab(i))`. This directly impacts the Blue Noise ordered dithering (Graphics intent, the default).

### 3. `dither_with_kernel()` — `crates/eink-dither/src/dither/mod.rs:292-293`

Calls `palette.find_nearest(oklab)` — so it inherits the same problem. However, error diffusion (Atkinson/Photo intent) partially self-corrects via error propagation: if grey maps to red, the green/blue deficit propagates to neighbors, pushing them toward compensating colors. Blue noise ordered dithering has NO such compensation since each pixel is independent.

### 4. `build_eink_palette()` — `src/rendering/svg_to_png.rs:217-234`

Constructs the `EinkPalette` from byonk's `(u8,u8,u8)` tuples. This is where a fix (e.g., setting a chroma weight) would be applied on the byonk side.

## Why Greyscale Works

For grey-only palettes (e.g., black, dark-grey, light-grey, white), ALL palette entries have chroma=0. The a,b components are zero for every entry, so `distance_squared` reduces to pure lightness matching `(L1-L2)²`. This is always correct.

## Why Base Colors Are Correct

Exact palette color matches are detected by byte-level sRGB comparison (`find_exact_match` in `crates/eink-dither/src/dither/mod.rs:188-211` and `preprocessor.rs:173`). These pixels skip dithering entirely and pass through unchanged. That's why the color swatches on the left of the test image render correctly.

## Proposed Fix Direction

Add a **chroma coupling penalty** to the distance metric: penalize matching pixels with very different chroma levels (e.g., achromatic pixel → chromatic palette entry). This ensures grey tones preferentially match black/white while chromatic tones still match their nearest chromatic entry.

The penalty formula: `distance = oklab_dist² + weight * (pixel_chroma - palette_chroma)²`

where `chroma = sqrt(a² + b²)`.

With `weight ≈ 1.5`:
- Grey→Red: 0.067 + 1.5×0.258² = 0.067 + 0.100 = **0.167**
- Grey→White: 0.160 + 1.5×0² = **0.160** (now nearest!)
- Grey→Black: 0.360 + 0 = 0.360

This requires changes to:
1. `Palette` struct — add `chroma_weight` field + precomputed `actual_chroma` vec
2. `Palette::find_nearest()` — use weighted distance when `chroma_weight > 0`
3. `find_second_nearest()` in blue_noise.rs — same weighted distance
4. `build_eink_palette()` in svg_to_png.rs — detect color palette, set weight

## Reproduction

```bash
# Use config-google.yaml which has a 6-color palette
# Render the default screen or any screen with photographic content
# Compare with a greyscale-only config — greyscale looks fine
```

## Diagnostic Code

```rust
use eink_dither::{Srgb, Palette, LinearRgb, Oklab};

fn main() {
    let colors = [
        Srgb::from_u8(0, 255, 0),     // green
        Srgb::from_u8(0, 0, 255),     // blue
        Srgb::from_u8(255, 255, 0),   // yellow
        Srgb::from_u8(255, 0, 0),     // red
        Srgb::from_u8(255, 255, 255), // white
        Srgb::from_u8(0, 0, 0),       // black
    ];
    let palette = Palette::new(&colors, None).unwrap();
    let grey = Srgb::from_u8(128, 128, 128);
    let grey_oklab = Oklab::from(LinearRgb::from(grey));
    let (idx, dist) = palette.find_nearest(grey_oklab);
    // idx=3 (red), dist²=0.067 — WRONG, should be white or black
}
```
