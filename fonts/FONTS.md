# Fonts bundled with Byonk

The release image is `FROM scratch` and has no system fonts, so a screen can
only use what is here.

## Outline families

| File | Family | Role | Licence |
|---|---|---|---|
| `Outfit-Variable.ttf` | `Outfit` | The house sans, and `cursive`/`fantasy` | OFL 1.1 |
| `SourceSans3-Variable.ttf` | `'Source Sans 3'` | `sans-serif` | `licenses/SourceSans3-OFL.txt` |
| `SourceSerif4-Variable.ttf` | `'Source Serif 4'` | `serif` | `licenses/SourceSerif4-OFL.txt` |
| `SourceCodePro-Variable.ttf` | `'Source Code Pro'` | `monospace` | `licenses/SourceCodePro-OFL.txt` |
| `TerminusTTF*.ttf` | `'Terminus (TTF)'` | A pixel face with outlines | OFL 1.1 |

The Source files are the upright variable faces only; the italics are not
bundled, and nothing in byonk asks for them. They keep Google Fonts' contents
but not its `Name[axes].ttf` filenames — brackets in a filename are a needless
hazard in the embedding globs, and `-Variable` matches Outfit's convention.

**Quote a family name that ends in a digit**: `font-family="Source Sans 3"` is
invalid CSS and falls back silently. Write `font-family="'Source Sans 3'"`. The
same applies to `'Terminus (TTF)'`.

Licence notices for the rest of the tree are still outstanding — see the licence
table in the handover.

# X11 bitmap families

Bitmap-only OpenType files built from the original X.Org BDF sources. Each file
carries every pixel size of one family and style as an embedded bitmap strike,
and **no outlines at all**.

## Why no outlines

A bitmap font is drawn once per pixel size, so every strike has metrics of its
own. An outline cannot stand in for them: `hmtx` holds one advance per glyph
that is merely scaled by size, so it can be right at one size at most. Terminus
is 8x14 at 14 px and 8x16 at 16 px — no single scalable advance expresses both.

The previous importer autotraced an outline and let FontForge derive the strike
advances from it, which overwrote what the BDFs declared. 16 of 28 fixed-pitch
strikes then rendered at the wrong pitch: `7x13` promises a 7 px cell in its own
`FONT` line and rendered at 6, welding glyphs into a bar.

With no outline in the file there is nothing to derive from, so a strike can
only say what its source said. Dropping the outlines also cut the 26 files from
8.7 MB to 4.9 MB.

**Consequence to know about:** at a size a face has no strike for, the renderer
scales the nearest strike. That is blocky, but it stays the same typeface at the
right width instead of falling back to a soft autotraced outline.

## Proportional

| Family | Styles | Pixel sizes |
|---|---|---|
| X11Helv | Regular, Bold, Oblique, BoldOblique | 8, 10, 11, 12, 14, 17, 18, 20, 24, 25, 34 |
| X11LuSans | Regular, Bold, Oblique, BoldOblique | 8, 10, 11, 12, 14, 17, 18, 19, 20, 24, 25, 26, 34 |

## Fixed width

**`X11LuType` is monospaced.** It was listed under *Proportional* here for a
long time; it is not — every glyph in one of its strikes shares one advance.

| Family | Styles | Pixel size → cell width |
|---|---|---|
| X11LuType | Regular, Bold | 8→5, 10→6, 11→7, 12→7, 14→9, 17→10, 18→11, 19→11, 20→12, 24→14, 25→15, 26→16, 34→20 |
| X11Term | Regular, Bold | 14→8, 18→11 |
| X11Misc5x | Regular | 6→4, 7→5, 8→5 |
| X11Misc6x | Regular | 9→6, 10→6, 12→6, 13→6 |
| X11Misc6x | Bold, Oblique | 13→6 |
| X11Misc7x | Regular, Bold | 13→7, 14→7 |
| X11Misc7x | Oblique | 13→7 |
| X11Misc8x | Regular | 13→8, 16→8 |
| X11Misc8x | Bold, Oblique | 13→8 |
| X11Misc9x | Regular, Bold | 15→9, 18→9 |
| X11Misc10x | Regular | 20→10 |
| X11Misc12x | Regular | 24→12 |

`X11Misc*` groups by cell width, which is neither a foundry nor a licence
grouping: `X11Misc8x` takes its 13 px strike from public-domain misc-fixed and
its 16 px strike from Sony.

Note that a family's name is its cell width at the size it is named for, not at
every size — `X11Misc5x` is 4 px wide at 6 px, because its 6 px strike comes
from `4x6`.

## Usage in SVG

`font-family` picks the family, `font-size` picks the strike.

```xml
<text font-family="X11Helv" font-size="14">Hello</text>
<text font-family="X11Helv" font-size="14" font-weight="700">Bold</text>
<text font-family="X11Misc7x" font-size="13">Fixed width</text>
```

**Use a size the family has a strike for.** Any other size scales the nearest
strike, and nothing warns you.

## Where they come from

Every source is an X.Org font tarball, pinned by version and SHA-256 in
`x11importer/sources.py`. Those ship the original `.bdf` files, so the importer
reads the bytes the fonts were authored in rather than a recompiled `.pcf`.

| Package | Gives |
|---|---|
| `font-adobe-75dpi`, `font-adobe-100dpi` | X11Helv |
| `font-bh-75dpi`, `font-bh-100dpi` | X11LuSans |
| `font-bh-lucidatypewriter-75dpi`, `-100dpi` | X11LuType |
| `font-bitstream-75dpi`, `font-bitstream-100dpi` | X11Term |
| `font-misc-misc` | X11Misc5x–X11Misc7x, X11Misc9x, X11Misc10x, X11Misc8x @13 |
| `font-sony-misc` | X11Misc8x @16, X11Misc12x |

X11Term spans two foundries: `term14.bdf` at 75 dpi is DEC Terminal at 14 px,
and the same file at 100 dpi is Bitstream Terminal at 18 px.

A 14 pt face at 75 dpi and a 10 pt face at 100 dpi are both 14 px. Where two
sources land on one pixel size, the 75 dpi one is used.

## Regenerating

Needs only Python and fontTools — no FontForge, no potrace, no X11 install.

```bash
make fonts-setup   # once
make fonts         # rebuild the 26 TTFs in place
```

The build is deterministic: the same sources produce byte-identical files.

Nothing is written unless every strike passes the checks in
`x11importer/verify.py`, which compare the built font back against its sources
on two independent grounds:

* every glyph's advance must equal its BDF `DWIDTH`; and
* for a charcell source, it must also equal the cell width the `FONT` line
  declares (`...-C-70-...` means 7 px).

The second catches a source whose own `DWIDTH` values contradict its declared
pitch, which the first cannot see. Not every source makes that promise —
`lutRS19` declares `M-159`, an average rather than a cell — so the run reports
how many strikes got both checks rather than letting a skipped one read as a
pass.

Run `make fonts-check` for the importer's unit tests on their own.
