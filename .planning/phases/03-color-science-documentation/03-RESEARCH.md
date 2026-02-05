# Phase 3: Color Science Documentation - Research

**Researched:** 2026-02-05
**Domain:** Rust crate documentation (rustdoc), inline code comments, color science rationale
**Confidence:** HIGH

## Summary

Phase 3 is a pure documentation phase. No code logic changes. The goal is to ensure a developer reading the `eink-dither` crate can understand **why** each color space conversion and distance calculation exists, not just **what** it does.

The crate already has good API documentation (rustdoc on every public type and method) but lacks two things: (1) a crate-level narrative explaining the color science rationale behind the HyAB + chroma coupling distance metric, including a pipeline diagram showing which color space is used at each stage, and (2) inline comments at every color space conversion point in the implementation code explaining **why that particular space is used there**.

There are three requirements:
- **DOCS-01**: Crate-level doc explaining the HyAB + chroma coupling distance metric choice with color science rationale.
- **DOCS-02**: Pipeline diagram in crate-level docs showing color space at each stage (sRGB input -> Linear RGB error diffusion -> OKLab matching).
- **DOCS-03**: Inline comments at every color space conversion in the code explaining why that space is used at that point.

**Primary recommendation:** Add a `# Color Science` section to `lib.rs` crate-level docs with the HyAB rationale and an ASCII pipeline diagram, then audit every `.rs` file for color space conversions and add inline `// WHY: ...` comments.

## Standard Stack

### Core

No new libraries needed. This phase uses only Rust's built-in documentation system.

| Tool | Purpose | Why Standard |
|------|---------|--------------|
| `rustdoc` | Crate-level and item-level documentation | Built into Rust toolchain, rendered by `cargo doc` |
| ASCII art | Pipeline diagram in doc comments | Works everywhere: terminal, GitHub, crate docs; no external renderer needed |
| `cargo doc --open` | Verify rendered documentation | Standard Rust workflow for doc verification |
| `cargo test --doc` | Verify doc examples compile | Ensures code examples in docs stay correct |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ASCII pipeline diagram | Mermaid diagram in separate mdBook page | Crate docs should be self-contained; mermaid requires external renderer and is not visible in `cargo doc` output |
| ASCII pipeline diagram | SVG image embedded in docs | Over-engineered for a 6-stage pipeline; harder to maintain; requires image hosting |
| Inline `// WHY:` comments | Separate `ARCHITECTURE.md` file | Inline comments are where the developer looks when reading code; external docs get stale |

## Architecture Patterns

### Documentation Structure

The documentation additions fall into three categories at three different levels:

```
crates/eink-dither/src/
├── lib.rs              # DOCS-01 + DOCS-02: Crate-level color science section
│                       #   - HyAB + chroma coupling rationale
│                       #   - Pipeline diagram (ASCII art in //! comments)
│                       #   - Color space choice justifications
├── palette/
│   └── palette.rs      # DOCS-01: Expand DistanceMetric::HyAB doc comments
│                       #   - Explain why standard HyAB insufficient
│                       #   - Document the chroma coupling extension
│                       #   - Reference kchroma=10.0 tuning rationale
│                       # DOCS-03: Inline comments on conversions (lines 188-195)
├── color/
│   ├── oklab.rs        # DOCS-03: Add WHY comments to From<LinearRgb> impl
│   ├── linear_rgb.rs   # DOCS-03: Add WHY comment to From<Srgb> impl
│   ├── srgb.rs         # DOCS-03: Add WHY comment to From<LinearRgb> impl
│   └── lut.rs          # DOCS-03: Add WHY comment explaining LUT choice over formula
├── dither/
│   ├── mod.rs          # DOCS-03: Add WHY comments to dither_with_kernel()
│   │                   #   - Line 292: Oklab::from(pixel) for palette matching
│   │                   #   - Line 202: Srgb::from(pixel) for exact match detection
│   │                   #   - Error diffusion in Linear RGB rationale
│   └── blue_noise.rs   # DOCS-03: Add WHY comments to BlueNoiseDither::dither()
│                       #   - Line 136: Oklab::from(pixel) for perceptual matching
├── preprocess/
│   ├── preprocessor.rs # DOCS-03: Add WHY comments to process() and boost_saturation()
│   │                   #   - Oklch for saturation (hue-preserving chroma scaling)
│   │                   #   - Linear RGB for contrast (physically correct midpoint)
│   └── oklch.rs        # DOCS-03: Already well documented, minor additions
└── output/
    └── render.rs       # DOCS-03: Add WHY comments to pipeline orchestration
```

### Pattern 1: Crate-Level Color Science Section

**What:** A dedicated `# Color Science` section in `lib.rs` crate-level docs that provides the "why" narrative.

**When to use:** For overarching rationale that spans multiple modules. This is the entry point for a developer asking "why does this crate make these color space choices?"

**Structure:**

```rust
//! # Color Science
//!
//! ## Why Three Color Spaces?
//!
//! [Paragraph explaining sRGB for I/O, Linear RGB for arithmetic, OKLab for perception]
//!
//! ## Pipeline
//!
//! [ASCII diagram showing the full flow]
//!
//! ## Distance Metric: HyAB + Chroma Coupling
//!
//! [Paragraph explaining why standard Euclidean in OKLab fails for limited palettes]
//! [Paragraph explaining HyAB basics]
//! [Paragraph explaining the chroma coupling extension and why kchroma=10.0]
//!
//! ## Why Error Diffusion Stays in Linear RGB
//!
//! [Paragraph explaining that error = light difference, which is linear]
```

### Pattern 2: Pipeline Diagram (ASCII Art in Doc Comments)

**What:** An ASCII art diagram in `//!` comments showing the color space at each pipeline stage.

**Key advantage:** Visible in `cargo doc` output, in the source code, and on GitHub. No external tooling needed.

**Example diagram format:**

```text
sRGB input          (from image file / SVG renderer)
    |
    v
LinearRgb           (gamma decode via LUT)
    |
    +---> Oklch     (polar OKLab: saturation boost via chroma scaling)
    |       |
    |       v
    |     Oklab     (Cartesian: back from Oklch)
    |       |
    |       v
    +<-- LinearRgb  (back from Oklab: enhanced pixel in linear space)
    |
    v
[Error Diffusion]   (accumulate + clamp in LinearRgb)
    |
    v
Oklab               (convert pixel+error for palette matching)
    |
    v
[find_nearest]      (HyAB + chroma coupling distance in OKLab)
    |
    v
palette index       (output: index into palette)
    |
    v
LinearRgb error     (quantization error = pixel - chosen palette color)
    |
    v
[diffuse to neighbors] (error stays in LinearRgb)
```

### Pattern 3: Inline WHY Comments

**What:** Short comments at every color space conversion that explain **why** that specific space is used at that point, not what the conversion does (the code already says what).

**Format convention:**

```rust
// WHY OKLab: Perceptual distance must be computed in a perceptually uniform space.
// Euclidean distance in OKLab correlates with human-perceived color difference.
let oklab = Oklab::from(pixel);
```

**Not:**
```rust
// Convert linear RGB to OKLab    <-- This just repeats what the code says
let oklab = Oklab::from(pixel);
```

### Anti-Patterns to Avoid

- **Do NOT add comments that repeat the code.** `// convert to OKLab` before `Oklab::from(pixel)` adds nothing. The comment must explain **why OKLab** at this point.

- **Do NOT change any code logic.** This is a documentation-only phase. If documentation reveals a bug, file it separately.

- **Do NOT over-document test code.** Test conversions exist to set up test fixtures. They do not need WHY comments because the test name and assertions provide the "why". Focus on implementation code only.

- **Do NOT add external documentation (mdBook pages, README sections).** The requirement is for crate-level docs (rustdoc) and inline comments. External docs may already exist in `docs/` and should not be duplicated.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Pipeline diagram | Custom SVG or image | ASCII art in `//!` comments | Self-contained, version-controlled, visible everywhere |
| Mathematical notation | LaTeX or MathML | Inline formulas with code-style backticks | `rustdoc` does not render LaTeX; use `` `kl * |dL| + kc * sqrt(da^2 + db^2)` `` |
| Reference links | Manual URLs | Rust doc `[`link`]` syntax | Standard rustdoc practice, enables cross-module linking |
| Doc testing | Manual verification | `cargo test --doc` | Ensures examples compile; catches stale docs |

## Common Pitfalls

### Pitfall 1: Comments That Describe "What" Instead of "Why"

**What goes wrong:** Comments like "Convert to OKLab" are useless because the code already says `Oklab::from(pixel)`. The reader needs to know **why OKLab and not sRGB or LinearRgb at this specific point**.
**Why it happens:** Writing "why" comments requires understanding the color science rationale, not just the API.
**How to avoid:** Each comment should answer: "If I changed this to a different color space, what would go wrong and why?"
**Warning signs:** Comments that could be generated by reading the function signature.

### Pitfall 2: Stale Doc Examples

**What goes wrong:** Code examples in doc comments that don't compile or use outdated APIs.
**Why it happens:** The crate API changed during Phase 1 and 2 (new `distance()` signature, auto-detection in `Palette::new()`).
**How to avoid:** Run `cargo test --doc -p eink-dither` after all documentation changes. Fix any failures.
**Warning signs:** Doc examples that reference old API (e.g., constructing `DistanceMetric::HyAB` without `kchroma`).

### Pitfall 3: Documenting the Chroma Coupling as Standard HyAB

**What goes wrong:** Presenting the chroma coupling penalty as if it's part of the published HyAB metric (Abasi et al., 2020). It is NOT. It's a domain-specific extension.
**Why it happens:** The `DistanceMetric::HyAB` variant contains the chroma coupling, making it look like one thing.
**How to avoid:** Explicitly state in documentation: "Standard HyAB (Abasi et al., 2020) decouples lightness from chrominance. We extend it with a chroma coupling penalty that is specific to discrete palette matching on e-ink displays."
**Warning signs:** Documentation that cites Abasi et al. for the chroma coupling term.

### Pitfall 4: Missing Conversions in the Audit

**What goes wrong:** Some color space conversions are missed and left without WHY comments.
**Why it happens:** Conversions happen in multiple modules, and some are indirect (e.g., `From` trait impls).
**How to avoid:** Use the inventory below (section "Conversion Audit") which lists every non-test conversion site.
**Warning signs:** A `grep` for conversion calls that don't have a `// WHY` comment nearby.

### Pitfall 5: Diagram Gets Out of Sync with Code

**What goes wrong:** The pipeline diagram describes a flow that no longer matches the actual code path.
**Why it happens:** Future code changes without updating the diagram.
**How to avoid:** Keep the diagram at crate-level (`lib.rs`) where it's visible. Add a note: "See individual module docs for details." The diagram should be high-level enough to survive minor refactors.
**Warning signs:** Pipeline stages in the diagram that don't correspond to actual code modules.

## Code Examples

### Example 1: Crate-Level Color Science Section (lib.rs)

```rust
//! # Color Science
//!
//! This section explains the rationale behind the color space choices and
//! distance metric used in the dithering pipeline. Understanding these
//! decisions is essential for maintaining correctness -- subtle changes
//! (e.g., computing distance in sRGB instead of OKLab) produce dramatically
//! wrong visual results on limited e-ink palettes.
//!
//! ## Three Color Spaces, Three Purposes
//!
//! The pipeline uses three color spaces, each chosen for a specific property:
//!
//! | Color Space | Property | Used For |
//! |-------------|----------|----------|
//! | **sRGB** | Standard encoding | Input/output (image files, device communication) |
//! | **Linear RGB** | Physically accurate light addition | Error diffusion, contrast adjustment |
//! | **OKLab** | Perceptually uniform distances | Palette matching (`find_nearest`) |
//!
//! **sRGB** is a gamma-corrected encoding designed for human perception of
//! brightness steps on screens. It is NOT suitable for arithmetic -- adding
//! two sRGB values does not produce the correct color. All image files and
//! device palettes use sRGB.
//!
//! **Linear RGB** represents physical light intensity. Adding two Linear RGB
//! values produces the correct combined light output. This is why error
//! diffusion (the quantization error distributed to neighboring pixels) must
//! operate in Linear RGB: the error represents a light intensity difference.
//!
//! **OKLab** (Oklab) is a perceptually uniform color space where Euclidean
//! distance correlates with human-perceived color difference. Two colors that
//! are 0.1 apart in OKLab look equally different regardless of where in the
//! color gamut they fall. This is why palette matching uses OKLab distances.
//!
//! ## Pipeline Overview
//!
//! ```text
//!  Input pixels (sRGB)
//!       |
//!       v
//!  [Preprocess] -----> LinearRgb (gamma decode)
//!       |                   |
//!       |              Oklch (saturation: scale chroma, preserves hue)
//!       |                   |
//!       |              LinearRgb (contrast: scale around midpoint)
//!       |                   |
//!       v                   v
//!  [Dither Loop] in LinearRgb (error accumulation + clamping)
//!       |
//!       +---> Oklab (perceptual matching)
//!       |         |
//!       |    find_nearest() using HyAB + chroma coupling
//!       |         |
//!       |    palette index (output)
//!       |         |
//!       +<--- quantization error = pixel - palette[idx] (in LinearRgb)
//!       |
//!       v
//!  [Diffuse error] to neighbors (in LinearRgb)
//! ```
//!
//! ## Distance Metric: HyAB + Chroma Coupling
//!
//! Standard Euclidean distance in OKLab treats lightness and chrominance
//! equally. This works well for continuous color spaces but fails for
//! discrete e-ink palettes (6-16 colors) where a grey pixel can map to a
//! chromatic color with similar lightness (e.g., yellow L=0.97 vs white
//! L=1.0 are close in Euclidean OKLab, but yellow is wrong for a grey pixel).
//!
//! **HyAB** (Abasi et al., 2020) improves on this by decoupling lightness
//! from chrominance:
//!
//! ```text
//! d_HyAB = kl * |L1 - L2| + kc * sqrt((a1-a2)^2 + (b1-b2)^2)
//! ```
//!
//! With kl=2.0, lightness differences are weighted 2x relative to
//! chrominance, which helps but is still insufficient for the e-ink
//! palette matching problem.
//!
//! **Chroma coupling** is our domain-specific extension (not from published
//! literature). It adds a penalty proportional to the difference in chroma
//! magnitude between the input pixel and the palette entry:
//!
//! ```text
//! d = kl * |dL| + kc * sqrt(da^2 + db^2) + kchroma * |C_pixel - C_palette|
//! ```
//!
//! With kchroma=10.0, a grey pixel (C=0) incurs a large penalty when
//! compared to any chromatic palette entry (C>0), forcing it to match
//! black or white instead. Chromatic-to-chromatic matching is minimally
//! affected because similar hues have similar chroma magnitudes.
//!
//! The high kchroma value (10.0, increased from the initial estimate of
//! 2.0) was determined empirically: blue noise dithering's
//! `find_second_nearest` needs kchroma > 8.2 to prevent yellow (L=0.97)
//! from capturing grey pixels that should map to white (L=1.0).
//!
//! ## Why Error Diffusion Stays in Linear RGB
//!
//! Quantization error represents the difference between the desired light
//! output and the chosen palette color's light output. Light adds linearly
//! in the physical world, so this difference must be computed and
//! propagated in Linear RGB to maintain physical accuracy.
//!
//! Computing error in sRGB would over-weight dark tones (where the gamma
//! curve is steep) and under-weight light tones. Computing error in OKLab
//! would require expensive back-conversion for every pixel and would not
//! represent physical light addition correctly.
```

### Example 2: Inline WHY Comment at Error Diffusion Core (dither/mod.rs)

```rust
// WHY LinearRgb for error: Quantization error represents a physical light
// intensity difference. Light adds linearly, so error must accumulate in
// Linear RGB to preserve physical accuracy. sRGB would distort error
// magnitudes due to its gamma curve; OKLab would not represent light
// addition correctly.
let error = [
    pixel.r - nearest_linear.r,
    pixel.g - nearest_linear.g,
    pixel.b - nearest_linear.b,
];
```

### Example 3: Inline WHY Comment at Palette Matching (dither/mod.rs)

```rust
// WHY OKLab for matching: The palette match must find the perceptually
// closest color. OKLab is perceptually uniform -- equal Euclidean distances
// correspond to equal perceived differences. Using sRGB or LinearRgb for
// distance would produce matches that look wrong to human eyes.
let oklab = Oklab::from(pixel);
let (nearest_idx, _dist) = palette.find_nearest(oklab);
```

### Example 4: Inline WHY Comment at Saturation Boost (preprocess/preprocessor.rs)

```rust
// WHY Oklch for saturation: Oklch is the polar form of OKLab where
// chroma (saturation) is an independent axis. Scaling chroma in Oklch
// preserves both hue and lightness exactly. HSL/HSV saturation shifts
// hue for non-primary colors and is not perceptually uniform.
let oklab = Oklab::from(pixel);
let oklch = Oklch::from(oklab);
let boosted = oklch.scale_chroma(factor);
```

### Example 5: Inline WHY Comment at Palette Precomputation (palette/palette.rs)

```rust
// WHY three representations per palette entry: Each color space serves a
// different pipeline stage. sRGB for output (device communication),
// LinearRgb for error diffusion (physical light math), OKLab for
// perceptual distance (find_nearest). Precomputing all three avoids
// per-pixel conversion overhead since palette colors never change.
let official_linear: Vec<LinearRgb> =
    official_srgb.iter().map(|&s| LinearRgb::from(s)).collect();
let official_oklab: Vec<Oklab> =
    official_linear.iter().map(|&l| Oklab::from(l)).collect();
```

## Conversion Audit: Sites Needing WHY Comments

This is the exhaustive list of color space conversions in implementation (non-test) code that need inline documentation.

### palette/palette.rs (Palette::new, lines 186-200)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 187-188 | `Srgb -> LinearRgb` (official) | Precompute for error diffusion |
| 189 | `LinearRgb -> Oklab` (official) | Precompute for perceptual distance |
| 193-194 | `Srgb -> LinearRgb` (actual) | Precompute for error diffusion (actual display colors) |
| 195 | `LinearRgb -> Oklab` (actual) | Precompute for perceptual distance (actual display colors) |
| 198-200 | Chroma computation from Oklab | Precompute for HyAB chroma coupling penalty |

### dither/mod.rs (dither_with_kernel, lines 246-335)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 202 | `LinearRgb -> Srgb` (in find_exact_match) | Exact match detection uses sRGB byte comparison |
| 292 | `LinearRgb -> Oklab` (pixel for matching) | Perceptual distance requires OKLab |
| 297-302 | Error computation in LinearRgb | Error = light difference, must be linear |

### dither/blue_noise.rs (BlueNoiseDither::dither, lines 111-187)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 136 | `LinearRgb -> Oklab` (pixel for matching) | Perceptual distance requires OKLab |
| 137 | Chroma computation from Oklab | Needed for HyAB chroma coupling in find_second_nearest |

### preprocess/preprocessor.rs (Preprocessor::process and helpers)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 269 | `Srgb -> LinearRgb` (each pixel) | All math must be in linear space |
| 308 | `LinearRgb -> Oklab` (for saturation) | Oklch derives from OKLab |
| 309 | `Oklab -> Oklch` (for chroma scaling) | Saturation = chroma in polar space |
| 315 | `Oklch -> Oklab` (after scaling) | Back to Cartesian OKLab |
| 316 | `Oklab -> LinearRgb` (after saturation) | Return to working color space |

### color/oklab.rs (From impls)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 135-153 | `LinearRgb -> Oklab` (From impl) | The impl doc already explains the math; add a brief WHY for the M1/M2 matrices |
| 176-194 | `Oklab -> LinearRgb` (From impl) | Already has doc; add WHY on unclamped output |

### color/linear_rgb.rs (From impl)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 46-53 | `Srgb -> LinearRgb` (From impl) | Already has doc comment explaining necessity; may need "WHY LUT" note |

### color/srgb.rs (From impl)

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 105-112 | `LinearRgb -> Srgb` (From impl) | Already has doc comment; may need "WHY LUT" note |

### color/lut.rs

| Line | Conversion | WHY Needed |
|------|-----------|------------|
| 14-37 | `srgb_to_linear()` | Explain WHY LUT instead of computing the IEC 61966-2-1 formula each time |
| 44-67 | `linear_to_srgb()` | Same rationale |

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Minimal crate-level docs (quick start only) | Full color science rationale section | This phase | Developers understand *why*, not just *how* |
| No pipeline diagram | ASCII pipeline diagram in crate docs | This phase | Visual understanding of color space flow |
| Some doc comments on conversions | Every conversion has WHY comment | This phase | Prevents future regressions from well-intentioned but incorrect changes |

**Current documentation state:**
- lib.rs has good Quick Start, Rendering Intents, Color Spaces (brief), Dithering Algorithms sections
- Individual modules have good `//!` module docs
- Public API items have full rustdoc with examples
- MISSING: Color science rationale, pipeline diagram, inline WHY comments at conversion sites

## Open Questions

### 1. Scope of DOCS-03: Test Code vs Implementation Code

- **What we know:** The requirement says "every color space conversion in the code." Taken literally, this includes hundreds of conversions in test code.
- **What's unclear:** Whether test conversions need WHY comments.
- **Recommendation:** Only add WHY comments to **implementation code** (non-test functions). Test conversions are self-documenting through test names and assertions. The audit above covers all implementation sites.

### 2. Whether to Cross-Reference docs/ mdBook Documentation

- **What we know:** There are existing docs at `docs/src/concepts/architecture.md` and `docs/src/api/lua-api.md` that may discuss color processing.
- **What's unclear:** Whether the crate-level documentation should link to the mdBook docs.
- **Recommendation:** Keep the crate-level docs self-contained. Add a brief note like "See the [project documentation](https://...) for the user-facing guide." but do not duplicate content.

### 3. How Much kchroma=10.0 History to Include

- **What we know:** kchroma was initially estimated at 2.0, proved insufficient, and was increased to 10.0 based on the empirical finding that find_second_nearest needs kchroma > 8.2.
- **What's unclear:** Whether the documentation should include the full tuning history or just the final value with rationale.
- **Recommendation:** Document the final value (10.0) and the essential rationale (blue noise dithering requires it > 8.2 to prevent yellow capturing grey pixels). Mention the initial estimate briefly. Do not include the full debugging history.

## Sources

### Primary (HIGH confidence)

- **Codebase analysis** -- All source files in `crates/eink-dither/src/` read and analyzed for existing documentation and conversion sites
- **Phase 1 research** (`.planning/phases/01-core-distance-metric-fix/01-RESEARCH.md`) -- Established the color science rationale
- **Rust documentation conventions** -- `rustdoc` format, `//!` for module/crate docs, `///` for item docs, `cargo test --doc` for verification

### Secondary (MEDIUM confidence)

- **Abasi et al., 2020** -- HyAB metric definition (Color Research & Application)
- **Bjorn Ottosson** -- OKLab color space specification (bottosson.github.io)
- **IEC 61966-2-1** -- sRGB gamma transfer function specification

### Tertiary (LOW confidence)

- **kchroma tuning history** -- From project memory and Phase 1/2 execution notes. The exact threshold (8.2) comes from empirical testing during implementation, not from published literature.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- Rust's built-in documentation system, no external tools needed
- Architecture: HIGH -- Exhaustive code audit completed, all conversion sites identified
- Pitfalls: HIGH -- All 5 pitfalls are documentation-specific and preventable with verification steps

**Research date:** 2026-02-05
**Valid until:** Indefinite -- documentation patterns do not expire; conversion sites may change with future code changes
