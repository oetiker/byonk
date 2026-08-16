"""Compare a built font back against the BDFs it came from.

The importer runs this before writing anything and refuses to produce a font
that fails. That is the whole lesson of F16: the old pipeline produced 26 fonts
whose strike advances disagreed with their sources, and because nothing ever
checked, they shipped and were documented as correct.

Two independent grounds of truth are used, deliberately:

* every glyph's `DWIDTH`, which is what the BDF says that glyph is worth; and
* the cell width in the XLFD (`...-C-70-...` means 7 px), which a charcell font
  promises for *every* glyph.

The second catches a source whose own `DWIDTH` values contradict its declared
pitch, which the first cannot see.
"""

from __future__ import annotations

from fontTools.ttLib import TTFont

from .bdf import BdfFont


def cell_checked(sources: list[BdfFont]) -> tuple[int, int]:
    """How many of `sources` the cell-width check applies to, and how many exist.

    Not every fixed-pitch source declares a whole-pixel cell: `lutRS19` says
    `M-159`, an average rather than a cell, while every one of its glyphs
    advances 16. The check skips those rather than invent a rounding — but a
    skipped check has to be visible, or "0 problems" would look like proof
    where nothing was compared.
    """
    return (
        sum(1 for source in sources if source.declared_cell_width is not None),
        len(sources),
    )


def check_advances(font: TTFont, sources: list[BdfFont]) -> list[str]:
    """Return one message per disagreement; an empty list means faithful."""
    problems: list[str] = []
    cmap = font.getBestCmap()
    strikes = _strikes_by_ppem(font)

    for source in sources:
        ppem = source.pixel_size
        if ppem not in strikes:
            problems.append(f"{ppem}px: strike missing from the built font")
            continue

        advances = strikes[ppem]
        cell = source.declared_cell_width

        for glyph in source.glyphs:
            if glyph.encoding < 0:
                continue
            where = f"{ppem}px U+{glyph.encoding:04X}"

            name = cmap.get(glyph.encoding)
            if name is None or name not in advances:
                problems.append(f"{where}: glyph missing from the built font")
                continue

            found = advances[name]
            if found != glyph.advance:
                problems.append(
                    f"{where}: advance is {found}, but the BDF declares "
                    f"DWIDTH {glyph.advance}"
                )
            elif cell is not None and found != cell:
                problems.append(
                    f"{where}: advance is {found}, but the XLFD declares a "
                    f"{cell} px cell"
                )

    return problems


def _strikes_by_ppem(font: TTFont) -> dict[int, dict[str, int]]:
    """Map ppem -> {glyph name: advance in pixels} as the compiled font has it."""
    eblc, ebdt = font["EBLC"], font["EBDT"]
    strikes: dict[int, dict[str, int]] = {}

    for index, strike in enumerate(eblc.strikes):
        advances: dict[str, int] = {}
        for sub in strike.indexSubTables:
            for name in sub.names:
                if sub.imageFormat == 5:
                    # Format 5 keeps one set of metrics for the whole subtable.
                    advances[name] = sub.metrics.horiAdvance
                else:
                    advances[name] = ebdt.strikeData[index][name].metrics.Advance
        strikes[strike.bitmapSizeTable.ppemX] = advances

    return strikes
