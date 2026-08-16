"""Read X11 BDF bitmap fonts.

BDF is a plain-text format, so this reader is deliberately literal: it keeps
what the file says and derives nothing. In particular `DWIDTH` — the advance in
whole pixels — is carried through untouched, because losing it is the defect
this importer exists to avoid.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class BdfGlyph:
    """One glyph, exactly as the BDF declares it."""

    name: str
    encoding: int
    advance: int
    """`DWIDTH` x, in pixels."""
    bbx: tuple[int, int, int, int] = (0, 0, 0, 0)
    """`BBX` as (width, height, x offset, y offset), in pixels."""
    rows: list[int] = field(default_factory=list)
    """One integer per scanline, `bbx` width bits wide, leftmost pixel highest."""


@dataclass
class BdfFont:
    """One BDF file: a single pixel size of a single face."""

    xlfd: str = ""
    """The `FONT` line, e.g. `-Misc-Fixed-Medium-R-Normal--13-120-75-75-C-70-ISO10646-1`."""
    properties: dict[str, str] = field(default_factory=dict)
    glyphs: list[BdfGlyph] = field(default_factory=list)

    @property
    def pixel_size(self) -> int:
        return int(self.properties.get("PIXEL_SIZE", 0))

    @property
    def ascent(self) -> int:
        return int(self.properties.get("FONT_ASCENT", 0))

    @property
    def descent(self) -> int:
        return int(self.properties.get("FONT_DESCENT", 0))

    @property
    def spacing(self) -> str:
        """`C` charcell, `M` monospaced, `P` proportional."""
        return _xlfd_field(self.xlfd, _XLFD_SPACING) or self.properties.get(
            "SPACING", ""
        )

    @property
    def declared_cell_width(self) -> int | None:
        """The cell width the XLFD promises, in pixels, or None if it promises none.

        Only charcell and monospaced fonts make that promise. `AVERAGE_WIDTH`
        is in tenths of a pixel, so `C-70` means every glyph is 7 px wide —
        which is exactly what the verifier holds the strikes to.
        """
        if self.spacing not in ("C", "M"):
            return None
        tenths = _xlfd_field(self.xlfd, _XLFD_AVERAGE_WIDTH)
        if not tenths or not tenths.lstrip("-").isdigit():
            return None
        width, remainder = divmod(int(tenths), 10)
        return width if remainder == 0 else None


# Field positions in an XLFD name, counting the empty string before the
# leading '-' as index 0.
_XLFD_SPACING = 11
_XLFD_AVERAGE_WIDTH = 12


def _xlfd_field(xlfd: str, index: int) -> str:
    parts = xlfd.split("-")
    return parts[index] if len(parts) > index else ""


def parse_bdf(path) -> BdfFont:
    font = BdfFont()
    glyph = None
    in_bitmap = False

    with open(path, "r", errors="replace") as fh:
        for line in fh:
            keyword, _, rest = line.strip().partition(" ")

            if keyword == "STARTCHAR":
                glyph = BdfGlyph(name=rest.strip(), encoding=-1, advance=0)
            elif glyph is None:
                if keyword == "FONT":
                    font.xlfd = rest.strip()
                elif keyword not in ("STARTFONT", "SIZE", "FONTBOUNDINGBOX"):
                    key, _, value = line.strip().partition(" ")
                    if value:
                        font.properties[key] = value.strip().strip('"')
                continue
            elif in_bitmap and keyword != "ENDCHAR":
                glyph.rows.append(_scanline(keyword, glyph.bbx[0]))
            elif keyword == "ENCODING":
                glyph.encoding = int(rest.split()[0])
            elif keyword == "DWIDTH":
                glyph.advance = int(rest.split()[0])
            elif keyword == "BBX":
                glyph.bbx = tuple(int(v) for v in rest.split()[:4])
            elif keyword == "BITMAP":
                in_bitmap = True
            elif keyword == "ENDCHAR":
                font.glyphs.append(glyph)
                glyph = None
                in_bitmap = False

    return font


def _scanline(hex_digits: str, width: int) -> int:
    """Turn one BITMAP line into a `width`-bit integer.

    BDF pads every scanline out to whole bytes, so the meaningful pixels sit in
    the high bits and the padding has to be shifted off.
    """
    if width <= 0:
        return 0
    padded_bits = len(hex_digits) * 4
    return int(hex_digits, 16) >> (padded_bits - width)
