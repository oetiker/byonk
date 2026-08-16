"""Tests for the bitmap-only sfnt builder.

`X11Misc7x-Regular.ttf` as shipped has a 13 px strike whose `horiAdvance` is 6
while its source `7x13.bdf` declares `DWIDTH 7` and `C-70`. That single lost
pixel is F16. These tests hold the builder to the source.
"""

import io

import pytest
from fontTools.ttLib import TTFont

from x11importer.bdf import parse_bdf
from x11importer.sfnt import build_bitmap_font

from bdf_fixtures import fixed_bdf, proportional_bdf


def roundtrip(font: TTFont) -> TTFont:
    """Save and reload, so the tests read compiled bytes rather than objects."""
    buf = io.BytesIO()
    font.save(buf)
    buf.seek(0)
    return TTFont(buf, lazy=False)


def strike_advance(font: TTFont, ppem: int, codepoint: int) -> int:
    """The advance the compiled strike gives one character, in whole pixels.

    Addressed by codepoint rather than glyph name: `post` 3.0 stores no names,
    so fontTools invents them on reload and a name-based lookup would be
    testing fontTools rather than the font.
    """
    name = font.getBestCmap()[codepoint]
    eblc, ebdt = font["EBLC"], font["EBDT"]
    for index, strike in enumerate(eblc.strikes):
        if strike.bitmapSizeTable.ppemX != ppem:
            continue
        for sub in strike.indexSubTables:
            if name not in sub.names:
                continue
            if sub.imageFormat == 5:
                return sub.metrics.horiAdvance
            return ebdt.strikeData[index][name].metrics.Advance
    raise AssertionError(f"no {ppem}px strike carries U+{codepoint:04X}")


def test_a_strike_keeps_the_pixel_advance_its_bdf_declared(tmp_path):
    source = parse_bdf(fixed_bdf(tmp_path, pixel_size=5, cell_width=3))
    font = roundtrip(build_bitmap_font([source], family="Test", style="Regular"))
    assert strike_advance(font, 5, 0x41) == 3


def test_every_strike_keeps_its_own_advance(tmp_path):
    """The whole defect was one advance being reused across sizes."""
    sources = [
        parse_bdf(fixed_bdf(tmp_path, pixel_size=5, cell_width=3, name="a.bdf")),
        parse_bdf(fixed_bdf(tmp_path, pixel_size=10, cell_width=7, name="b.bdf")),
    ]
    font = roundtrip(build_bitmap_font(sources, family="Test", style="Regular"))
    assert strike_advance(font, 5, 0x41) == 3
    assert strike_advance(font, 10, 0x41) == 7


def test_a_proportional_face_keeps_a_different_advance_per_glyph(tmp_path):
    source = parse_bdf(proportional_bdf(tmp_path, pixel_size=8))
    font = roundtrip(build_bitmap_font([source], family="Test", style="Regular"))
    assert strike_advance(font, 8, 0x41) == 6
    assert strike_advance(font, 8, 0x69) == 2


def test_the_font_carries_no_outlines(tmp_path):
    """Outline-free is the point: it is what keeps the advances unforgeable."""
    source = parse_bdf(fixed_bdf(tmp_path, pixel_size=5, cell_width=3))
    font = roundtrip(build_bitmap_font([source], family="Test", style="Regular"))
    assert "glyf" not in font
    assert "loca" not in font


def test_glyphs_are_reachable_through_cmap(tmp_path):
    source = parse_bdf(fixed_bdf(tmp_path, pixel_size=5, cell_width=3))
    font = roundtrip(build_bitmap_font([source], family="Test", style="Regular"))
    assert 0x41 in font.getBestCmap()
    assert 0x69 in font.getBestCmap()


def test_family_and_style_reach_the_name_table(tmp_path):
    source = parse_bdf(fixed_bdf(tmp_path, pixel_size=5, cell_width=3))
    font = roundtrip(build_bitmap_font([source], family="X11Test", style="Bold"))
    names = {r.nameID: str(r) for r in font["name"].names if r.platformID == 3}
    assert names[1] == "X11Test"
    assert names[2] == "Bold"
    assert font["OS/2"].usWeightClass == 700


def test_an_empty_strike_list_is_refused():
    with pytest.raises(ValueError):
        build_bitmap_font([], family="Test", style="Regular")
