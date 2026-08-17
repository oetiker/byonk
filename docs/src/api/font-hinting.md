# Font Hinting

Byonk hints text automatically. **A screen needs no configuration to get sharp
type** — this page is only for overriding what byonk chooses.

## What byonk does on its own

Hinting nudges a glyph's outline so its stems land on whole pixels. At the sizes
an e-ink panel uses, that is the difference between crisp type and mush. Byonk
picks the treatment from the panel's palette:

| Panel | What byonk applies | Why |
|---|---|---|
| 2 neutral colours (black and white) | Mono hinting, **and the glyphs are drawn 1-bit** | Hinting alone still leaves anti-aliased grey edges, which the ditherer turns into speckle. A 1-bit panel wants a 1-bit glyph. |
| More than 2 greys | Smooth hinting, anti-aliased | There are greys available to render the edges with, so the softer treatment reads better. |

This is decided per render from the palette the device reports, so the same
screen does the right thing on different hardware.

> **Upgrading:** this used to require `{% include "byonk-base-v1/hinting.svg" %}`
> in your template. It no longer does — the include is now inert and can be
> deleted without changing the output.

## Overriding it

Return a `font_hinting` table from `script.lua`. Everything in it is optional.

```lua
return {
  data = { ... },
  font_hinting = {
    engine = "auto",
    target = "mono",
  },
}
```

### Turning hinting off

```lua
font_hinting = false
```

Useful for a screen that is mostly photographic, where hinted small type is not
what you are optimising for.

### `engine`

Which hinter adjusts the outline.

| Value | Meaning |
|---|---|
| `"auto"` *(default)* | The automatic hinter. Ignores whatever hints the font ships with. |
| `"interpreter"` | Run the font's own embedded hinting program. |
| `"auto_fallback"` | Use the font's hints where it has them, the automatic hinter where it doesn't. |

Most of the fonts byonk bundles carry no usable hinting program, so `"auto"` is
the default and is usually what you want.

### `target`

What the hinted outline is being prepared for. Either a shorthand string or a
table whose `mode` selects the style.

```lua
target = "mono"                                    -- shorthand
target = { mode = "mono", aliased = false }        -- the long form
target = { mode = "light", symmetric = true, preserve_linear_metrics = false }
```

| `mode` | Extra keys | Meaning |
|---|---|---|
| `"mono"` | `aliased` *(default `true`)* | Strong hinting for monochrome rasterization. With `aliased = true` the glyph is also drawn 1-bit. |
| `"smooth"` / `"normal"` | `symmetric`, `preserve_linear_metrics` | The standard anti-aliased treatment. |
| `"light"` | same | Lighter touch — less horizontal adjustment. |
| `"lcd"` / `"vertical_lcd"` | same | Tuned for subpixel layouts. Of little use on e-ink. |

`symmetric` defaults to `false` and `preserve_linear_metrics` to `true`, which
is what byonk itself uses for a grey panel — so `target = "smooth"` gives you
exactly what a grey panel would have got anyway.

**`aliased` only has meaning on the document default**, not on a variant —
but that is a limit of the *flag*, not of the variant. Aliasing is an ordinary
inheritable SVG property, so an element using a variant can ask for it itself:

```svg
<text font-family="'Crisp Body', Outfit" text-rendering="optimizeSpeed">10px</text>
```

A mono variant plus `optimizeSpeed` renders **byte-identically** to the
document-level `target = { mode = "mono", aliased = true }`. That pairing is how
you get genuinely crisp 1-bit type for *part* of a screen — on a grey panel as
well as a black-and-white one — which is the whole reason variants exist.

**Only ever pair `optimizeSpeed` with mono hinting.** See the warning below for
what happens otherwise.

### `variants` — hinting one font two ways in one screen

A variant is a name **you invent** that byonk intercepts during font selection
and resolves to a real family with its own hinting and bitmap-strike settings.
That is what lets the same font appear twice in one screen with different
treatment.

```lua
font_hinting = {
  variants = {
    ["Crisp Body"]   = { font = "Outfit",  hinting = { target = "mono" } },
    ["Plain Labels"] = { font = "X11Helv", strikes = false },
  },
}
```

```svg
<text font-family="'Crisp Body', Outfit">Sharp at 10px</text>
```

| Key | Meaning |
|---|---|
| `font` | **Required.** The real family this is a variant of. Must be installed — `fonts.families()` lists what this server has. |
| `hinting` | A table like the top-level `engine`/`target`. Omit to inherit the document default; `false` turns hinting off for this variant only. |
| `strikes` | `false` stops a bitmap font using its embedded pixel strikes. |

Two rules byonk enforces at parse time, both of which fail loudly rather than
silently rendering the wrong thing:

- **The variant name must not be a real installed family.** The name is a hook
  byonk intercepts, so naming it after a real family shadows that family.
- **`font` must name an installed family.** A family that does not resolve falls
  through to the generic mapping, so a typo would silently give you a different
  font rather than an error.

Name a variant for its **purpose**, not `<Family> <TechnicalTerm>`. `"Outfit
Mono"` reads as a monospaced Outfit to everyone who meets it later; `"Crisp
Body"` says what it is for.

Always name a real fallback in the SVG (`font-family="'Crisp Body', Outfit"`),
so the text still resolves sensibly if the variant is ever removed.

> **Do not also set `font-family` in a CSS rule that matches the same element.**
> In SVG a presentation attribute is the *lowest* priority, so
> `text { font-family: Outfit; }` in a `<style>` block overrides every
> `font-family="'Crisp Body', …"` attribute on the elements it matches — and
> the variant is then never selected. Nothing warns you: the text renders
> perfectly well in the base font. Byonk's own hinting demo shipped this way,
> with nine cells that were supposed to differ and did not. Put the family on
> a class, or on the elements, but not in both places.

### Naming variants does not replace the default

A directive that only declares `variants` leaves byonk's adaptive default in
place. You have to state a `target` to override it. So this keeps mono hinting
on a black-and-white panel and merely adds a variant:

```lua
font_hinting = { variants = { ["Crisp Body"] = { font = "Outfit" } } }
```

## The one trap: a variant that escapes aliasing

**Glyph aliasing is a property of the document; hinting is a property of the
face.** On a black-and-white panel byonk makes the whole document 1-bit. A
variant that opts out of mono hinting is still drawn 1-bit — and aliasing an
outline that was *not* mono-hinted drops stems, because the rasterizer has no
dropout control. Thin strokes simply vanish.

Byonk warns when a screen sets this up, naming the variants involved. The fix is
in the SVG, on the elements using that variant:

```svg
<text font-family="'Soft Body', Outfit" text-rendering="optimizeLegibility">…</text>
```

`optimizeLegibility` restores anti-aliasing **and keeps hinting**.

> **Do not use `geometricPrecision` for this.** It also restores anti-aliasing,
> but it disables hinting at the same time — which is not what you asked for.

The same property runs the other way, and that is the useful direction: an
element that is mono-hinted can ask for `text-rendering="optimizeSpeed"` to be
drawn 1-bit even where the document is not. `optimizeSpeed` without mono
hinting is precisely the state this section warns about, so the two belong
together — see `examples/demo/font/hinting`, which states a `text-rendering`
on every cell for exactly this reason.

## Notes

- **A bitmap font only renders as a bitmap at a size it has a strike for.** At
  any other size its nearest strike is scaled, which is blocky. `fonts/FONTS.md`
  lists the sizes each bundled family carries.
- Two knobs are currently inert: `mode = "light"` renders identically to
  `"normal"`, and with `engine = "interpreter"` the `target` has no effect.
