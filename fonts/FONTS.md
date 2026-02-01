# X11 Bitmap Fonts for Byonk

Consolidated TTF files with embedded bitmap strikes, converted from X11 BDF fonts
using FontForge. Each TTF contains all pixel sizes for a given family+style as
bitmap strikes, plus autotraced scalable outlines from the largest size.

## Proportional Fonts (from 75dpi + 100dpi)

| Family      | Styles                              | Pixel Sizes                                    |
|-------------|-------------------------------------|------------------------------------------------|
| X11Helv     | Regular, Bold, Oblique, BoldOblique | 8, 10, 11, 12, 14, 17, 18, 20, 24, 25, 34     |
| X11LuSans   | Regular, Bold, Oblique, BoldOblique | 8, 10, 11, 12, 14, 17, 18, 19, 20, 24, 25, 26, 34 |
| X11LuType   | Regular, Bold                       | 8, 10, 11, 12, 14, 17, 18, 19, 20, 24, 25, 26, 34 |
| X11Term     | Regular, Bold                       | 14, 18                                         |

## Fixed-Width Fonts (from misc, grouped by cell width)

| Family      | Styles                   | Pixel Sizes       |
|-------------|--------------------------|-------------------|
| X11Misc5x   | Regular                  | 6, 7, 8           |
| X11Misc6x   | Regular, Bold, Oblique   | 9, 10, 12, 13     |
| X11Misc7x   | Regular, Bold, Oblique   | 13, 14            |
| X11Misc8x   | Regular, Bold, Oblique   | 13, 16            |
| X11Misc9x   | Regular, Bold            | 15, 18            |
| X11Misc10x  | Regular                  | 20                |
| X11Misc12x  | Regular                  | 24                |

## Usage in SVG/CSS

Use `font-family` for the family name and `font-size` to select the bitmap strike:

```xml
<text font-family="X11Helv" font-size="14">Hello</text>
<text font-family="X11Helv" font-size="14" font-weight="700">Bold</text>
<text font-family="X11Misc7x" font-size="13">Fixed width</text>
```

The renderer (resvg/skrifa) automatically selects the closest bitmap strike matching
the requested font-size (ppem). For sizes without an exact bitmap strike, the
autotraced scalable outlines are used as fallback.

## Regenerating

On a Linux machine with X11 fonts and FontForge installed:

```bash
fontforge -lang=py -script fonts/x11-importer.py
mv fonts/ttf_output/*.ttf fonts/
rmdir fonts/ttf_output
```
