# Gamut mapping for continuous-tone regions

**Date:** 2026-08-07
**Status:** design, approved for planning

## Why

A six-colour panel renders most of the hue circle as flat, single-colour
bands. Measured on the E1002's calibrated palette, 16 of 24 hues come back
as one solid colour with no dithering at all.

Most of that is physics, not code. A dithered patch's average is by
construction a convex combination of the palette's *actual* colours in
linear RGB, so the convex hull of those six colours bounds what any
error-diffusion algorithm can reproduce. Measured against that bound
(`test_dither_versus_gamut_bound`):

| | mean dE |
|---|---|
| physical bound (best any algorithm could do) | 0.097 |
| production today | 0.119 |

So ~82% of the error is the gamut. 77 of 144 targets are already within
0.02 dE of the bound. Flat blue across 225°–270° genuinely is optimal.

But the *visible* failure is not the error magnitude. It is the loss of
differences: gradients flatten into bands, distinct hues collapse onto one
ink, and hue ordering inverts (285° currently lands at h8°, out of order
with both its neighbours). Nearest-colour matching already minimises dE —
that is precisely why it looks wrong. It keeps vividness by discarding the
differences.

This design trades colorimetric accuracy for discriminability, deliberately.

## What this is not

Gamut mapping does not enlarge the gamut. Marked regions will look **less
saturated** than today's output. Today's is falsely saturated. Nothing will
make a six-ink panel render a vivid rainbow.

Mean dE is expected to get **worse**. See Testing for what replaces it.

## Scope

Opt-in, per region, marked in the SVG. An unmarked document renders exactly
as it does today, byte for byte.

Correction happens once, on the rasterized frame, immediately before
dithering. There is no separate pre-SVG path: marking an `<image>` element
covers the photo case, and correcting the rasterized frame is the only point
where the mapping sees the pixels as they will actually be dithered — after
scaling, compositing and any SVG filters. `image_process()` keeps its
existing tone work and gains nothing here.

The marker is not photo-specific. It applies to any continuous-tone content:
photographs, gradients, charts, illustrations.

## Authoring surface

### The marker

`data-byonk-tone="continuous"` on any element or group.

```svg
<g data-byonk-tone="continuous">
  <image href="photo.png" .../>
  <rect fill="url(#skyGradient)" .../>
</g>
```

A `data-*` attribute rather than a `class`, because `class` is already the
styling channel in existing screens (`font-black`, `caption`, `col-header`)
and a reserved class name would collide with it conceptually. resvg ignores
the attribute entirely — verified: a render with and without it is
byte-identical — so the normal rasterization pass is unaffected. The
attribute exists solely for the mask rewriter.

Because the SVG is templated from the script's data, a screen can decide at
render time which regions are continuous-tone. No new Lua API is needed for
the marking itself.

### Inheritance and override

The value is inherited by descendants; a descendant may override it.

```svg
<g data-byonk-tone="continuous">
  <rect fill="url(#sky)" .../>
  <text data-byonk-tone="graphic">18:42</text>   <!-- stays crisp -->
</g>
```

The override is load-bearing: a chart or photo with a caption over it needs
the background mapped and the label left pinned and sharp.

There are exactly two values. `graphic` is the default everywhere, so an
unmarked document behaves as today. It exists as an explicit value only so
the override is expressible; it is not a third mode.

Resolution rule: an element's effective tone is its own attribute if
present, otherwise its parent's.

### Adaptation groups

`data-byonk-tone-group="<id>"`, inherited the same way. Marked regions
sharing an id adapt together. The default group is the whole frame.

Frame-wide is the default because identical colours should map identically
wherever they appear — a photo and a gradient sharing a blue stay consistent.
The override exists for when one very vivid region would otherwise drag the
compression for a mild one.

### Tuning knobs

On the script return, alongside the existing `dither` / `preserve_exact`
keys:

```lua
gamut = {
  knee            = 0.6,   -- where compression begins, as a fraction of Cmax
  amount          = 1.0,   -- 0 = no mapping, 1 = full
  max_compression = 2.5,   -- cap on R; beyond it the knee's tail takes over
}
```

`max_compression` caps `R` (see Content adaptation). Since `R` is by
definition the factor by which chroma is squeezed — it is the value mapped
onto `Cmax` — the knob is literally "never compress by more than this".
Raising it lets an extremely vivid image adapt further, at the cost of
flattening everything else; lowering it protects the bulk of the image and
pushes the extremes into the knee's asymptotic tail instead, where they stay
distinguishable but heavily compressed.

`amount` interpolates between the input and the mapped chroma:
`C_out = C + amount * (C' - C)`. At `amount = 1` the output is the mapped
chroma; at `0` the region is untouched, which makes it a clean A/B switch
for judging the effect on a real panel. Values between 1 and 0 are still in
gamut because `C' <= Cmax` and the interpolation only ever moves toward `C`
from below — but note that `amount < 1` can leave chroma above `Cmax`, in
which case the dithering clips it as it does today. `amount` is therefore a
comparison and taste control, not a correctness one; only `amount = 1`
guarantees in-gamut output.

Every value of `knee` produces in-gamut output by construction. The knobs
trade accuracy against colourfulness, never validity — with the `amount`
caveat above.

The knobs are frame-level, not per-group: adaptation groups change only
which pixels are measured together to derive `R`, not how the compression
curve is shaped.

## Behaviour inside a marked region

Two things change, and they are one concept:

1. Pixels are gamut-mapped into the measured-colour hull.
2. Exact-match pinning is disabled.

The second is what removes the seams. Today any pixel exactly equal to an
official palette colour is forced to that entry and its error discarded.
That is right for a deliberate flat fill and wrong mid-gradient, where it
puts a hard stripe across a smooth ramp — the mechanism keys off pixel
value, which cannot distinguish authorial intent from coincidence. The
marker supplies the intent that pixel value cannot.

Outside marked regions nothing changes at all.

## Algorithm

### The hull

The achievable set is the convex hull of the actual colours **in linear
RGB** — that is where light adds. It is *not* convex in OKLab, so the hull
cannot be computed in perceptual space. Six vertices, a handful of facets,
built once when the palette resolves. Membership is "inside all facets".

### The chroma-limit table

Per-pixel hull queries are far too slow. When the palette resolves,
precompute `Cmax(hue, lightness)`: for each bin, binary-search the largest
chroma whose OKLCh → linear-RGB point still lies inside the hull. Store as a
small 2-D table, sampled bilinearly at render time. Built once per palette,
not per frame.

### Content adaptation

The mapping is derived from the content, not fixed. For each pixel in the
adaptation group compute `rho = C / Cmax(h, L)` — how far out of gamut it
is, 1.0 being exactly at the boundary. Take the 99th percentile to get `R`
(a percentile, not the maximum, so one stray neon pixel cannot crush the
whole image).

- `R <= 1` — the content is already in gamut. **Identity. No mapping at
  all.** Nothing is needlessly desaturated.
- `R > 1` — compress `[0, R] -> [0, 1]` through the knee below.

`R` is then capped at `max_compression` (default 2.5), so no image can
squeeze the rest of its colours arbitrarily hard to accommodate its most
extreme region.

Content beyond the cap is **not** clipped. Normalising by the capped `R`
simply leaves it above `Cmax` going into the knee, and because the knee is
asymptotic it maps any input, however large, to just under `Cmax` while
staying strictly increasing. So the extreme tail is compressed very hard but
never collapses onto a shared value, and the monotonicity property below
holds for all inputs. Clipping at the cap would have broken it.

Both guards exist because a single scalar can be hijacked by a small region.
Their limits are worth stating plainly:

- The percentile handles literal outliers completely. On an 800x480 frame
  the discarded top 1% is 3,840 pixels, so a handful of stray extreme pixels
  cannot move `R` at all.
- It does **not** handle a small-but-not-tiny region. A neon sign filling 2%
  of the frame sits above the 99th-percentile cut, sets `R`, and compresses
  the other 98% harder than needed. The cap bounds that damage; it does not
  eliminate it.
- The knee is the third protection and the most important one in practice:
  compression only bites above `k*Cmax`, so low-chroma content passes
  through untouched however large `R` becomes. A mostly-grey photo with one
  vivid flower does not go flat.

A percentile is a *relative* guard, so it weakens as the marked region
shrinks: in a 50x50 marked photo the top 1% is 25 pixels, and three bad
pixels are 12% of the discarded tail. The implementation therefore needs an
absolute floor on the discard count as well as the percentage.

### Per pixel

sRGB -> linear -> OKLab -> OKLCh, compress chroma, and back. **Hue is
carried through untouched**, which is what fixes the ordering inversions.

With `Cmax = Cmax(h, L)` and knee fraction `k`:

```
C <= k*Cmax :  C' = C
C >  k*Cmax :  C' = k*Cmax + (1-k)*Cmax * (1 - exp(-t)),
               t  = (C - k*Cmax) / ((1-k)*Cmax)
```

Continuous at the knee and **strictly increasing everywhere**, approaching
`Cmax` asymptotically without reaching it. That property is the formal
statement of the goal: two colours that differed before still differ after.
Nothing collapses onto a shared value — which is exactly what a clipping
approach (HPMINDE) would do, and why it was rejected.

`k` is expected to be low, around 0.5–0.7 rather than the ~0.9 typical in
print work, because this gamut is small enough that almost everything is
outside it; a high knee would crush the entire vivid range into a sliver
near `Cmax` and reintroduce the flatness.

### Why chroma-only suffices

Because the palette contains both pure black and pure white, every `(L, h)`
has a non-empty achievable range `[0, Cmax]`, so compressing chroma at fixed
lightness always lands in gamut. No lightness compression is needed and the
map cannot fail.

This is palette-dependent. A palette lacking a near-black or near-white has
unreachable lightnesses, so the implementation clamps `L` into the hull's
achievable range first, then compresses chroma. The four-colour panels are
closer to that case than the six-colour ones.

### Known simplification

Real GCUSP also migrates lightness toward the gamut cusp, where more chroma
is available, trading a little lightness accuracy for noticeably more
colourfulness. Deliberately out of scope for the first cut: it is a second
free parameter, and one knob tuned against the calibration screen is worth
more than two tuned against each other.

## Pipeline

```
SVG -> rasterize ------------------> frame pixels -+
  +-> rewrite to mask -> rasterize -> mask --------+
                                                   v
                       gamut-map pixels where mask set
                                                   v
                       dither, pinning off where mask set
```

The mask is produced by rewriting the SVG so every element's paint becomes
white if it is inside a marked subtree and black otherwise, then rasterizing
it. Recolouring rather than deleting is deliberate: it makes **occlusion
just work**, because an unmarked shape covering part of a marked photo
correctly masks it out — the renderer resolves z-order for us. Edge
antialiasing produces greys; threshold at 0.5.

Insertion point is `render_to_palette_png()` in `src/rendering/svg_to_png.rs`,
between rasterization and `ditherer.dither()`. The measured palette is
already a parameter there.

### Costs

- Roughly doubles rasterization time per render.
- Requires an `eink-dither` API change: `preserve_exact` is global today and
  must become an optional per-pixel mask.

## Testing

### The oracle

`best_reachable()` in `crates/eink-dither/src/domain_tests.rs` already
computes exact hull projections by optimisation — too slow for production,
ideal for tests. The fast `Cmax` table is validated against it across a
dense sample. The slow correct thing checks the fast thing.

### Properties

All follow from the design rather than from tuning, so they are assertions
about correctness, not about taste:

- **Idempotence** — mapping twice equals mapping once. After one pass the
  content is in gamut, so `R <= 1` and the second pass is identity. A sharp
  check that the adaptive path is wired correctly.
- **In-gamut identity** — content already inside the hull returns unchanged.
- **Hue preservation** — output hue equals input hue within tolerance.
- **Strict monotonicity** — `C1 < C2` implies `C1' < C2'`; nothing collapses.

### Regression metrics

Mean dE is the wrong yardstick here and will worsen. Replace it with:

- **Hue-order monotonicity** around the circle — does 285° land between 270°
  and 300°, rather than inverting to h8° as it does today.
- **Preserved local contrast** across a gradient ramp.

Plus a visual golden: `byonk-builtin/calibration/gamut` rendered with and
without mapping.

## Failure modes

- **Greyscale palettes** (the four-level panels) — the hull collapses to a
  segment on the grey axis, so `Cmax = 0` everywhere. Guard the `C / Cmax`
  division and emit `C' = 0`. The result is correct: colour content
  desaturates to grey rather than being flung at the nearest ink. Covered by
  a test, not discovered in the field.
- **Palette without a near-black or near-white** — clamp `L` into the hull's
  achievable range before compressing chroma.
- **Uncalibrated panel** — build the hull from whatever palette the ditherer
  targets: `colors_actual` when it resolves, official colours otherwise. The
  two must not diverge.
- **Older byonk** — ignores `data-byonk-tone` and renders as today, so
  screens stay portable.
- **Mask rasterization failure** — return a `RenderError`. This is not an
  expected runtime condition: the mask is derived from a document that just
  rasterized successfully, by the same renderer, with only paint values
  changed. Recolouring introduces no new geometry, references or fonts, so
  if the original rendered the mask renders. The realistic failure paths are
  all our own bugs — a malformed rewrite, or mishandling `url(#...)`,
  `<use>` or clip paths. Failing is an assertion that the rewriter is
  correct. There is deliberately **no fallback path**: silently rendering
  something materially different while reporting success is the failure mode
  that costs hours.

## Prior art

- Ján Morovič, *Color Gamut Mapping* (Wiley, 2008) — the standard monograph.
- CIE 156:2004, *Guidelines for the Evaluation of Gamut Mapping Algorithms* —
  the evaluation protocol; fixes HPMINDE and SGCK as the baselines everyone
  benchmarks against.
- Spatial gamut mapping (Farup/Gatta/Rizzi; Zolliker/Simon, both IEEE TIP
  2007) — preserves local detail rather than only accuracy. A plausible
  second iteration; see Not doing.

Caveat on applicability: that literature targets printers and displays with
large, smooth, continuous gamuts. Ours is a six-vertex polytope with a dark
blue and a dark green — the gamut boundary has corners, where cusp-based
methods assume smoothness. The limited-palette and colour-quantisation
literature may be the closer relative.

## Not doing

- Spatial / multiscale local-contrast restoration. Best perceptual result,
  materially more complex, and its value cannot be judged without the simple
  version to compare against.
- Cusp lightness migration (see Known simplification).
- Extending `image_process`'s `palette_aware` beyond its current luminance
  endpoints.
- HPMINDE-style clipping as a shipped mode. It may still be worth a few
  lines behind a test-only switch as a comparison point when tuning `k`.
- **Per-hue `R`.** Deriving the compression factor per hue slice rather than
  as one scalar would stop a vivid red from compressing the blues, which is
  the main residual weakness of the adaptation. Rejected for now because it
  changes relative chroma *between* hues — and the relationships between
  colours are precisely what this design exists to preserve. Revisit only if
  real screens show the scalar being hijacked in practice, and then measure
  it against the hue-order metric rather than by eye.

## Prerequisites

The two dithering defects found while diagnosing this should be fixed first,
or the mapper will be tuned against a target that is still being distorted:

1. Exact-match pinning discarding error mid-gradient (this design disables it
   inside marked regions, which addresses it there but not elsewhere).
2. `error_clamp` starving the error feedback at channel extremes.

Also outstanding, and not addressed here: the ditherer under-mixes the
achromatic entries with the chromatic ones. Dark warm colours at 30°–60°
have a computed bound of 0.000 — exactly reproducible from black plus
red/yellow — yet production is off by 0.05–0.09. That is a dithering bug
with no gamut excuse, and it is independent of this work.
