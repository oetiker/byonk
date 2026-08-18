"""Tests for the build-time assertion pass.

F16 shipped for months because nothing ever compared the built font back
against its sources. This pass is that comparison, and the importer refuses to
write a font that fails it.
"""

from fontTools.ttLib import TTFont

from x11importer.bdf import parse_bdf
from x11importer.sfnt import build_bitmap_font
from x11importer.verify import check_advances

from bdf_fixtures import fixed_bdf, proportional_bdf
from test_sfnt import roundtrip


def build(sources):
    return roundtrip(build_bitmap_font(sources, family="Test", style="Regular"))


def break_one_advance(font: TTFont, ppem: int, codepoint: int, value: int):
    """Damage one strike advance the way the old FontForge pipeline did."""
    name = font.getBestCmap()[codepoint]
    for index, strike in enumerate(font["EBLC"].strikes):
        if strike.bitmapSizeTable.ppemX == ppem:
            font["EBDT"].strikeData[index][name].metrics.Advance = value
            return
    raise AssertionError(f"no {ppem}px strike")


def test_a_faithful_build_reports_nothing(tmp_path):
    sources = [parse_bdf(fixed_bdf(tmp_path, pixel_size=13, cell_width=7))]
    assert check_advances(build(sources), sources) == []


def test_a_strike_narrower_than_its_declared_cell_is_reported(tmp_path):
    """The exact shipped defect: 7x13 declares C-70 but renders at 6 px."""
    sources = [parse_bdf(fixed_bdf(tmp_path, pixel_size=13, cell_width=7))]
    font = build(sources)
    break_one_advance(font, 13, 0x41, 6)

    problems = check_advances(font, sources)

    assert len(problems) == 1
    assert "13" in problems[0] and "U+0041" in problems[0]
    assert "7" in problems[0] and "6" in problems[0]


def test_a_proportional_glyph_losing_its_own_dwidth_is_reported(tmp_path):
    sources = [parse_bdf(proportional_bdf(tmp_path, pixel_size=8))]
    font = build(sources)
    break_one_advance(font, 8, 0x69, 6)  # 'i' given 'A's advance

    problems = check_advances(font, sources)

    assert len(problems) == 1
    assert "U+0069" in problems[0]


def test_a_missing_strike_is_reported(tmp_path):
    sources = [
        parse_bdf(fixed_bdf(tmp_path, pixel_size=13, cell_width=7, name="a.bdf")),
        parse_bdf(fixed_bdf(tmp_path, pixel_size=18, cell_width=9, name="b.bdf")),
    ]
    font = build(sources)
    # Read EBDT first: it decompiles against EBLC, so dropping the strike
    # before touching it would leave the two tables inconsistent for a
    # different reason than the one under test.
    del font["EBDT"].strikeData[1]
    del font["EBLC"].strikes[1]

    problems = check_advances(font, sources)

    assert any("18" in p for p in problems)


def test_a_source_contradicting_its_own_declared_cell_is_reported(tmp_path):
    """The XLFD is independent evidence, and it catches what DWIDTH cannot.

    Here the font is built perfectly faithfully — every advance matches its
    DWIDTH — but the source itself claims a 7 px cell while giving out 6 px.
    Only the cell-width check can see that.
    """
    sources = [
        parse_bdf(fixed_bdf(tmp_path, pixel_size=13, cell_width=7, dwidth=6)),
    ]

    problems = check_advances(build(sources), sources)

    assert len(problems) == 2  # one per glyph
    assert all("7 px cell" in p for p in problems)


def test_a_missing_glyph_is_reported(tmp_path):
    sources = [parse_bdf(fixed_bdf(tmp_path, pixel_size=13, cell_width=7))]
    font = build(sources)
    name = font.getBestCmap()[0x69]
    for sub in font["EBLC"].strikes[0].indexSubTables:
        if name in sub.names:
            sub.names.remove(name)

    problems = check_advances(font, sources)

    assert any("U+0069" in p for p in problems)
