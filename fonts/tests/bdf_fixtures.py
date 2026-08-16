"""Synthetic BDF files for the tests.

Written out rather than checked in as binaries so a reader can see exactly which
declared advance each test is holding the builder to. Nothing here touches the
network or the real X.Org sources.
"""

import textwrap

_HEADER = textwrap.dedent("""\
    STARTFONT 2.1
    FONT -Misc-Test-{weight}-R-Normal--{px}-{pt}-75-75-{spacing}-{avgwidth}-ISO10646-1
    SIZE {px} 75 75
    FONTBOUNDINGBOX {maxw} {px} 0 -1
    STARTPROPERTIES 6
    PIXEL_SIZE {px}
    POINT_SIZE {pt}
    FONT_ASCENT {ascent}
    FONT_DESCENT {descent}
    SPACING "{spacing}"
    COPYRIGHT "Public domain test font."
    ENDPROPERTIES
    CHARS {nchars}
    """)


def _glyph(name, codepoint, advance, width, height):
    """A solid rectangle `width` x `height`, so every scanline is easy to read."""
    padded = ((width + 7) // 8) * 8
    row = format(((1 << width) - 1) << (padded - width), "0%dX" % (padded // 4))
    lines = "\n".join([row] * height)
    return textwrap.dedent(f"""\
        STARTCHAR {name}
        ENCODING {codepoint}
        SWIDTH {advance * 100} 0
        DWIDTH {advance} 0
        BBX {width} {height} 0 0
        BITMAP
        {lines}
        ENDCHAR
        """)


def _write(tmp_path, name, text):
    path = tmp_path / name
    path.write_text(text)
    return path


def fixed_bdf(
    tmp_path, pixel_size, cell_width, name="fixed.bdf", weight="Medium", dwidth=None
):
    """A charcell font: XLFD `C-<cell*10>` and every glyph at DWIDTH `cell`.

    Every glyph's ink is deliberately **narrower** than its advance, the way a
    real font leaves a side bearing. Without that gap "the advance" and "the
    width of the bitmap" are the same number, and a build that derives one from
    the other — which is precisely the F16 defect — passes unnoticed.
    """
    height = max(1, pixel_size - 1)
    ink = max(1, cell_width - 1)
    # `dwidth` lets a test build a source that contradicts its own XLFD, which
    # is the only case the cell-width check catches and the DWIDTH check cannot.
    advance = cell_width if dwidth is None else dwidth
    body = _HEADER.format(
        weight=weight,
        px=pixel_size,
        pt=pixel_size * 10,
        spacing="C",
        avgwidth=cell_width * 10,
        maxw=cell_width,
        ascent=height,
        descent=pixel_size - height,
        nchars=2,
    )
    body += _glyph("A", 0x41, advance, ink, height)
    body += _glyph("i", 0x69, advance, ink, height)
    return _write(tmp_path, name, body + "ENDFONT\n")


def proportional_bdf(tmp_path, pixel_size, name="prop.bdf"):
    """A proportional font: `A` advances 6 px on 4 px of ink, `i` 2 px on 1 px."""
    height = max(1, pixel_size - 1)
    body = _HEADER.format(
        weight="Medium",
        px=pixel_size,
        pt=pixel_size * 10,
        spacing="P",
        avgwidth=40,
        maxw=6,
        ascent=height,
        descent=pixel_size - height,
        nchars=2,
    )
    body += _glyph("A", 0x41, 6, 4, height)
    body += _glyph("i", 0x69, 2, 1, height)
    return _write(tmp_path, name, body + "ENDFONT\n")
