"""Assemble BDF strikes into a bitmap-only OpenType font.

The font deliberately carries **no outlines**. That is not only a size win (the
autotraced outlines used to be 39-78% of every file): it is what makes F16
unrepeatable. The old FontForge pipeline derived each strike's advance from a
single scalable outline, which silently overwrote the `DWIDTH` the BDF
declared. With no outline in the file there is nothing left to derive from, so
`EBLC` can only say what the source said.

A size the font has no strike for is scaled up from the nearest strike by the
renderer. That is blocky, but it is the same typeface at the right width -- and
unlike the old autotraced fallback it never turns into grey mush on a 4-grey
panel.

Bitmap data is written as EBDT format 1 (small per-glyph metrics, byte-aligned
scanlines) indexed by EBLC index format 1. skrifa -- the shaper behind byonk's
renderer -- accepts byte-aligned 1-bit masks, and per-glyph small metrics is the
only layout in which a proportional face can carry a different advance per
glyph.
"""

from __future__ import annotations

from fontTools.fontBuilder import FontBuilder
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables._g_l_y_f import Glyph
from fontTools.ttLib.tables.BitmapGlyphMetrics import SmallGlyphMetrics
from fontTools.ttLib.tables.E_B_D_T_ import ebdt_bitmap_format_1
from fontTools.ttLib.tables.E_B_L_C_ import (
    SbitLineMetrics,
    Strike,
    eblc_index_sub_table_1,
)

from .bdf import BdfFont

NOTDEF = ".notdef"

UNITS_PER_PIXEL = 100
"""Font units per pixel of the largest strike.

Fixes `unitsPerEm` at `largest_ppem * 100`, so that strike's advances land on
exact integers instead of being rounded.
"""

_EPOCH = 3849984000
"""Fixed `head` timestamp (2026-01-01Z in Mac epoch seconds).

Hard-coded rather than read from the clock so that rebuilding the fonts from
the same sources produces byte-identical files.
"""

_IMAGE_FORMAT = 1
_INDEX_FORMAT = 1
_BIT_DEPTH = 1
_HORIZONTAL_METRICS = 0x01


def glyph_name(codepoint: int) -> str:
    return "uni%04X" % codepoint if codepoint <= 0xFFFF else "u%05X" % codepoint


def build_bitmap_font(
    strikes: list[BdfFont],
    family: str,
    style: str,
    copyright_notice: str = "",
    license_notice: str = "",
) -> TTFont:
    """Build one font carrying every strike in `strikes`.

    `strikes` are one BDF per pixel size of the same face. They need not be
    sorted; duplicates of a pixel size are refused rather than silently
    dropped, because "which of the two won" is exactly the kind of question
    that made the old importer impossible to reason about.
    """
    if not strikes:
        raise ValueError("a font needs at least one strike")

    strikes = sorted(strikes, key=lambda s: s.pixel_size)
    sizes = [s.pixel_size for s in strikes]
    if len(set(sizes)) != len(sizes):
        raise ValueError(f"duplicate pixel sizes in {family} {style}: {sizes}")

    largest = strikes[-1]
    upem = largest.pixel_size * UNITS_PER_PIXEL

    order = _glyph_order(strikes)
    builder = _base_font(order, upem, strikes, family, style)
    _set_names(builder, family, style, copyright_notice, license_notice)
    _attach_strikes(builder.font, order, strikes, upem)

    font = builder.font
    del font["glyf"]
    del font["loca"]
    return font


def _glyph_order(strikes: list[BdfFont]) -> list[str]:
    codepoints = {
        glyph.encoding
        for strike in strikes
        for glyph in strike.glyphs
        if glyph.encoding >= 0
    }
    return [NOTDEF] + [glyph_name(cp) for cp in sorted(codepoints)]


def _base_font(order, upem, strikes, family, style) -> FontBuilder:
    largest = strikes[-1]
    builder = FontBuilder(upem, isTTF=True)
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap(
        {
            cp: glyph_name(cp)
            for cp in (
                int(name[3:], 16) if name.startswith("uni") else int(name[1:], 16)
                for name in order
                if name != NOTDEF
            )
        }
    )
    # Empty outlines only so that FontBuilder can lay the sfnt out; both tables
    # are deleted again before the font is returned.
    builder.setupGlyf({name: Glyph() for name in order})

    scale = upem / largest.pixel_size
    ascender = round(largest.ascent * scale)
    descender = -round(largest.descent * scale)

    builder.setupHorizontalHeader(ascent=ascender, descent=descender, lineGap=0)
    # hmtx must exist before OS/2, which derives xAvgCharWidth from it.
    _set_horizontal_metrics(builder, order, strikes, upem)
    builder.setupOS2(
        sTypoAscender=ascender,
        sTypoDescender=descender,
        sTypoLineGap=0,
        usWinAscent=ascender,
        usWinDescent=-descender,
        usWeightClass=700 if "Bold" in style else 400,
        fsSelection=_fs_selection(style),
        achVendID="BYNK",
    )
    builder.setupPost(keepGlyphNames=False)

    head = builder.font["head"]
    head.created = head.modified = _EPOCH
    head.macStyle = _mac_style(style)
    return builder


def _fs_selection(style: str) -> int:
    bits = 0
    if "Bold" in style:
        bits |= 1 << 5
    if "Oblique" in style or "Italic" in style:
        bits |= 1 << 0
    return bits or (1 << 6)  # REGULAR


def _mac_style(style: str) -> int:
    bits = 0
    if "Bold" in style:
        bits |= 1 << 0
    if "Oblique" in style or "Italic" in style:
        bits |= 1 << 1
    return bits


def _set_horizontal_metrics(builder, order, strikes, upem):
    """Give every glyph the advance of the largest strike that draws it.

    Nothing renders from these numbers while a strike matches, but a consumer
    that ignores strikes entirely still needs a sane width, and so does the
    renderer when it scales a strike to a size no strike covers.
    """
    advances = {}
    for strike in strikes:  # ascending, so larger strikes overwrite smaller
        scale = upem / strike.pixel_size
        for glyph in strike.glyphs:
            if glyph.encoding >= 0:
                advances[glyph_name(glyph.encoding)] = round(glyph.advance * scale)

    default = max(advances.values(), default=upem // 2)
    builder.setupHorizontalMetrics(
        {name: (advances.get(name, default), 0) for name in order}
    )


def _set_names(builder, family, style, copyright_notice, license_notice):
    subfamily = style.replace("Oblique", "Italic").replace("BoldItalic", "Bold Italic")
    names = {
        "familyName": family,
        "styleName": subfamily,
        "uniqueFontIdentifier": f"{family}-{style}",
        "fullName": f"{family} {style}",
        "psName": f"{family}-{style}",
        "version": "1.000",
    }
    if copyright_notice:
        names["copyright"] = copyright_notice
    if license_notice:
        names["licenseDescription"] = license_notice
    builder.setupNameTable(names, mac=False)


def _attach_strikes(font: TTFont, order, strikes, upem):
    eblc = newTable("EBLC")
    ebdt = newTable("EBDT")
    # Both tables store their version as 16.16 fixed point, so fontTools wants
    # the float, not the raw 0x00020000.
    eblc.version = ebdt.version = 2.0
    eblc.strikes = []
    ebdt.strikeData = []

    ids = {name: index for index, name in enumerate(order)}

    for source in strikes:
        bitmaps = {}
        for glyph in source.glyphs:
            if glyph.encoding < 0:
                continue
            bitmaps[glyph_name(glyph.encoding)] = _bitmap(glyph, source, font)

        names = sorted(bitmaps, key=lambda name: ids[name])
        eblc.strikes.append(_strike(source, names, ids, bitmaps, upem))
        ebdt.strikeData.append(bitmaps)

    font["EBLC"] = eblc
    font["EBDT"] = ebdt


def _bitmap(glyph, source: BdfFont, font: TTFont) -> ebdt_bitmap_format_1:
    width, height, x_offset, y_offset = glyph.bbx

    metrics = SmallGlyphMetrics()
    metrics.width = width
    metrics.height = height
    metrics.BearingX = x_offset
    # BDF measures the bitmap box from the baseline up; EBDT measures from the
    # top of the box down to the baseline.
    metrics.BearingY = height + y_offset
    metrics.Advance = glyph.advance

    row_bytes = (width + 7) // 8
    padding = row_bytes * 8 - width
    data = b"".join(
        (row << padding).to_bytes(row_bytes, "big") for row in glyph.rows
    )

    bitmap = ebdt_bitmap_format_1(b"", font)
    bitmap.metrics = metrics
    bitmap.imageData = data
    return bitmap


def _strike(source: BdfFont, names, ids, bitmaps, upem) -> Strike:
    strike = Strike()
    table = strike.bitmapSizeTable

    widest = max((bitmaps[name].metrics.Advance for name in names), default=0)
    for direction in ("hori", "vert"):
        metrics = SbitLineMetrics()
        setattr(table, direction, metrics)
        metrics.ascender = source.ascent
        metrics.descender = -source.descent
        metrics.widthMax = widest
        metrics.caretSlopeNumerator = 0
        metrics.caretSlopeDenominator = 1
        metrics.caretOffset = 0
        metrics.minOriginSB = 0
        metrics.minAdvanceSB = 0
        metrics.maxBeforeBL = source.ascent
        metrics.minAfterBL = -source.descent
        metrics.pad1 = metrics.pad2 = 0

    table.colorRef = 0
    table.startGlyphIndex = ids[names[0]]
    table.endGlyphIndex = ids[names[-1]]
    table.ppemX = table.ppemY = source.pixel_size
    table.bitDepth = _BIT_DEPTH
    table.flags = _HORIZONTAL_METRICS

    index = eblc_index_sub_table_1(b"", None)
    index.indexFormat = _INDEX_FORMAT
    index.imageFormat = _IMAGE_FORMAT
    index.imageDataOffset = 0
    index.firstGlyphIndex = table.startGlyphIndex
    index.lastGlyphIndex = table.endGlyphIndex
    index.names = names
    index.locations = [(0, 0)] * len(names)  # recomputed by EBDT.compile

    strike.indexSubTables = [index]
    return strike
