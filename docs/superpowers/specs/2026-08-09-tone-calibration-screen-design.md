# Tone Calibration Screen — Design

_2026-08-09. Owner-approved. Adds a builtin calibration screen that shows the
effect of `data-byonk-tone="continuous"` by rendering the same content twice on
one screen, once mapped and once not._

## Why

Gamut mapping is implemented end to end and reaches nothing: it applies only
where an SVG marks a region `data-byonk-tone="continuous"`, and no shipping
screen does. There is no way to see what it does on a real panel, and every
judgement so far has come from test harnesses and downscaled PNGs — a medium the
project has already had to retract conclusions from twice.

This screen is the first thing to mark a region in production markup. It exists
to answer one question by eye, on the panel: **what does the tone marker
actually change?**

It is *not* a tuning surface. It ships the production defaults and exposes no
gamut knobs, so nobody judges the feature at a setting that will never ship.

## Where

A new screen, `screens/builtin/calibration/tone/`. The existing
`calibration/gamut` ("Gamut Patches") keeps its job — a full-width 24-hue
diagnostic of which hues collapse to a single ink. That is a different question
and it needs the width; halving it would break it.

## Layout

Two mirrored columns, one vertical seam. On an 800×480 panel each column is
about 393 px wide.

```
+-----------------------------+-----------------------------+
| UNMAPPED (control)          | GAMUT MAPPED                |  header text
+-----------------------------+-----------------------------+
|        photograph           |        photograph           |  ~190 px
+-----------------------------+-----------------------------+
|        hue sweep  ->        |        hue sweep  ->        |  ~55 px
+-----------------------------+-----------------------------+
|  [][][][][][][][][][][]     |  [][][][][][][][][][][]     |
|  [][][][][][][][][][][]     |  [][][][][][][][][][][]     |  ~205 px
|  hues x lightness levels    |  hues x lightness levels    |
+-----------------------------+-----------------------------+
                            ^
                     neutral gutter
```

Left/right rather than top/bottom: a landscape panel gives each column full
height, which suits the photograph's aspect ratio, and the eye compares
horizontally-adjacent detail well.

Three bands because they answer different questions, and the mapper trades
between them:

| band | what it shows |
|---|---|
| photograph | the everyday benefit on real content — saturated regions that fixed-`L` dulled |
| hue sweep | banding and tail separation across a controlled gradient |
| patch grid | ink survival, and which hues collapse to one entry |

Geometry is computed in Lua from `layout.width` / `layout.height` with
`scale_pixel()`, as the other calibration screens do, so it adapts to the
1200 px-wide E1004 as well as the 800 px E1002. Remainders are spread one pixel
at a time across leading cells, matching `calibration/gamut`, so the grid stays
flush.

## Marking

A single `<g data-byonk-tone="continuous">` wraps the **right column's three
content bands**. Nothing else is marked.

The header text stays outside the group. Text is graphic content; marking it
would push glyph edges through the mapper for no benefit.

Three consequences, each of which the implementation must respect:

**One adaptation factor, derived from the marked pixels only.** The mask is a
single frame-level `Vec<bool>` — `data-byonk-tone-group` is declared in
`tone_mask.rs` but **not implemented**, so there is exactly one adaptation
group. `R` therefore comes from the right column's pixels. Because both columns
show identical content, the mapped side adapts to exactly what it displays.
That is what makes the comparison fair rather than flattering, and it is the
reason the two columns must render the *same* content, not merely similar
content.

**The gutter is load-bearing.** Error diffusion runs across the whole frame
*after* mapping, so the two columns bleed into each other at the seam. A neutral
gutter several pixels wide absorbs that error instead of letting the mapped
column's error leak into the control. This is also the reason the design does
not split each individual cell or band in half: that shape gives the tightest
comparison but puts a mask boundary through every element, contaminating the
thing being measured.

**Patches carry no stroke.** `calibration/gamut` outlines its cells because
hues that collapse come out pure white and would vanish against its white
background. Here the patch band sits on a dark ground instead, following
`calibration/color`, so white patches remain visible without the mask having to
reason about strokes (ruling 8: the mask must not invent a stroke).

## Parameters

`hues` (int) and `levels` (int) shape the patch grid, mirroring
`calibration/gamut`'s parameters. Defaults are **12 hues and 5 levels**, chosen
for the half-width column — about 31×39 px cells on an 800 px panel — rather
than inherited from the full-width screen's 24×6, which would give 16 px cells
here. Ranges match the existing screen (hues 2–48, levels 1–12).

**No gamut knobs.** `knee`, `amount` and `max_compression` are deliberately not
exposed and the script returns no `gamut` table. The screen shows what a real
screen gets. This also avoids restating the mapper's defaults in a YAML file,
where they would silently drift from the Rust constants — a drift the project
has already recorded as undetectable by review.

## Photograph asset

Screens cannot share assets: `screen_store` rejects `..` in asset paths, so the
new screen needs its own copy.

Ship a downscaled JPEG of roughly 640×360, derived from
`calibration/color/photo.png` (byonk's own portrait test asset, ~7% of pixels
out of gamut). The screen never displays it wider than about 600 px — half of
the largest panel — so a copy of the 1.5 MB original would be repository weight
for no visible gain. Both columns are handed identical pixels, so JPEG artifacts
cannot bias the comparison. `screens/builtin/default/background.jpg` sets the
precedent for a JPEG asset.

## Files

```
screens/builtin/calibration/tone/
  meta.yaml     title, description, byonk version, refresh, hues/levels params
  script.lua    geometry for both columns; returns data only, no gamut table
  screen.svg    both columns; right column's bands wrapped in the tone group
  photo.jpg     downscaled portrait
```

## Testing

The screen is data, so the guard that earns its place is a rendering test:

**Render the screen and assert the tone mask covers the right column and only
the right column.** Concretely — build the mask via `rasterize_tone_mask` for
the rendered document at a 6-colour panel spec (800×480), then assert:

- **no masked pixel lies left of the seam.** Allow a handful for edge
  antialiasing at the gutter, not a percentage: fewer than 0.1% of masked
  pixels may have `x < width/2`.
- **the marked fraction of the whole frame is between 0.30 and 0.48.** The
  right column is half the frame, less the header strip and the inter-band
  gaps, so a correct mask lands near 0.4. Zero, one half exactly, or ~1.0 each
  indicate a distinct failure — marking dropped, header swallowed, or the group
  hoisted to the root.

The band is deliberately wide. It is a guard against the marking being lost,
not a golden-image test of the layout, and a tight bound would fail every time
someone adjusts a band height.

This catches the failure mode that matters: the marking being dropped, inverted,
or swallowed by a refactor of the rewriter. Without it the screen still renders
something plausible while comparing nothing — and "it looked fine" is exactly
how this initiative lost a session before.

A second, cheaper assertion: `has_tone_markup` returns true for the screen's
rendered SVG. It is nearly free and fails loudly if the attribute is ever
templated away.

## Out of scope

- Exposing gamut knobs (explicitly declined).
- Implementing `data-byonk-tone-group`. The screen needs exactly one adaptation
  group and must not be the reason multi-group support gets built.
- Marking any other shipping screen. That changes real output for real devices
  and remains the owner's call.
