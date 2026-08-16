"""Tests for the BDF reader.

The whole point of F16 is that a glyph's DWIDTH — its advance in whole pixels —
must survive the conversion. These tests pin the reading half of that.
"""

import textwrap

from x11importer.bdf import parse_bdf

# A 2-glyph 3x5 fixed-pitch font. `C-30` in the XLFD declares a 3.0 px cell,
# which matches the DWIDTH of both glyphs.
TINY_BDF = textwrap.dedent("""\
    STARTFONT 2.1
    FONT -Misc-Test-Medium-R-Normal--5-50-75-75-C-30-ISO10646-1
    SIZE 5 75 75
    FONTBOUNDINGBOX 3 5 0 -1
    STARTPROPERTIES 5
    PIXEL_SIZE 5
    FONT_ASCENT 4
    FONT_DESCENT 1
    SPACING "C"
    COPYRIGHT "Public domain test font."
    ENDPROPERTIES
    CHARS 2
    STARTCHAR A
    ENCODING 65
    SWIDTH 600 0
    DWIDTH 3 0
    BBX 3 5 0 0
    BITMAP
    40
    A0
    E0
    A0
    A0
    ENDCHAR
    STARTCHAR B
    ENCODING 66
    SWIDTH 600 0
    DWIDTH 3 0
    BBX 3 5 0 0
    BITMAP
    C0
    A0
    C0
    A0
    C0
    ENDCHAR
    ENDFONT
    """)


def write(tmp_path, text, name="tiny.bdf"):
    p = tmp_path / name
    p.write_text(text)
    return p


def test_glyph_advance_comes_from_dwidth(tmp_path):
    font = parse_bdf(write(tmp_path, TINY_BDF))
    assert [g.advance for g in font.glyphs] == [3, 3]


def test_bitmap_rows_are_read_left_aligned_to_the_bbx_width(tmp_path):
    font = parse_bdf(write(tmp_path, TINY_BDF))
    a = font.glyphs[0]
    assert a.bbx == (3, 5, 0, 0)
    # 0x40 = 0b010xxxxx -> ".#." once the padding bits past width 3 are dropped
    assert a.rows == [0b010, 0b101, 0b111, 0b101, 0b101]


def test_a_fixed_pitch_font_declares_its_cell_width_in_the_xlfd(tmp_path):
    """`C-30` is the ground truth the verifier checks every advance against."""
    font = parse_bdf(write(tmp_path, TINY_BDF))
    assert font.spacing == "C"
    assert font.declared_cell_width == 3


def test_a_proportional_font_declares_no_cell_width(tmp_path):
    proportional = TINY_BDF.replace(
        "Normal--5-50-75-75-C-30-", "Normal--5-50-75-75-P-28-"
    ).replace('SPACING "C"', 'SPACING "P"')
    font = parse_bdf(write(tmp_path, proportional))
    assert font.spacing == "P"
    assert font.declared_cell_width is None


def test_pixel_size_and_vertical_metrics_are_read(tmp_path):
    font = parse_bdf(write(tmp_path, TINY_BDF))
    assert (font.pixel_size, font.ascent, font.descent) == (5, 4, 1)
