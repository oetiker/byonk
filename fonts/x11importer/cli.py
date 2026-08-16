"""Build every shipped X11 font from pinned BDF sources.

Run it with `make fonts` (or `python3 fonts/x11-importer.py`). It downloads the
X.Org font tarballs, verifies their checksums, and writes 26 bitmap-only TTFs.

Nothing is written unless every font passes `verify.check_advances`. F16
shipped because the old pipeline had no such gate; this one refuses to produce
a font whose strikes disagree with the BDFs they came from.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .bdf import BdfFont, parse_bdf
from .families import FACES, Face, bdf_files
from .sfnt import build_bitmap_font
from .sources import fetch
from .verify import cell_checked, check_advances

REPO_FONTS = Path(__file__).resolve().parent.parent


def collect(face: Face, packages: dict[str, Path]) -> list[BdfFont]:
    """Parse every BDF of `face`, keeping one source per pixel size.

    Two sources can land on the same pixel size: a 14 pt face at 75 dpi and a
    10 pt face at 100 dpi are both 14 px. `families.FACES` lists 75 dpi first
    and the first one wins, which is what the previous importer did too -- the
    strike inventory is unchanged, only its metrics are fixed.
    """
    chosen: dict[int, BdfFont] = {}
    for path in bdf_files(face, packages):
        source = parse_bdf(path)
        chosen.setdefault(source.pixel_size, source)
    return [chosen[size] for size in sorted(chosen)]


def notices(sources: list[BdfFont]) -> str:
    """Every distinct copyright across the sources, in first-seen order.

    A face can span foundries -- X11Misc8x takes 8x13 from public-domain
    misc-fixed and 8x16 from Sony -- so one notice per file is not enough.
    """
    seen: list[str] = []
    for source in sources:
        notice = source.properties.get("COPYRIGHT", "").strip()
        if notice and notice not in seen:
            seen.append(notice)
    return "\n".join(seen)


def build_all(packages: dict[str, Path], output: Path) -> list[str]:
    """Build every face into `output`. Returns the problems found, if any."""
    problems: list[str] = []
    built: list[tuple[Face, object, list[BdfFont]]] = []

    for face in FACES:
        sources = collect(face, packages)
        font = build_bitmap_font(
            sources,
            family=face.family,
            style=face.style,
            copyright_notice=notices(sources),
        )
        faults = check_advances(font, sources)
        if faults:
            problems.extend(f"{face.filename}: {fault}" for fault in faults)
        built.append((face, font, sources))

    if problems:
        return problems

    output.mkdir(parents=True, exist_ok=True)
    checked = declared = 0
    for face, font, sources in built:
        font.save(output / face.filename)
        with_cell, total = cell_checked(sources)
        checked += with_cell
        declared += total
        sizes = ", ".join(str(s.pixel_size) for s in sources)
        size_on_disk = (output / face.filename).stat().st_size
        print(f"  {face.filename:28} {size_on_disk:>8,} bytes   strikes: {sizes}")

    print(
        f"\nEvery strike was checked against its BDF's DWIDTH. "
        f"{checked} of {declared} were also checked against a cell width "
        f"declared in the XLFD; the rest declare no whole-pixel cell."
    )
    return []


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_FONTS,
        help="where to write the TTFs (default: the fonts/ directory itself)",
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=REPO_FONTS / ".x11-cache",
        help="where to keep the downloaded X.Org tarballs",
    )
    args = parser.parse_args(argv)

    print(f"Fetching X.Org font sources into {args.cache} ...")
    packages = fetch(args.cache)

    print(f"Building {len(FACES)} fonts ...")
    problems = build_all(packages, args.output)

    if problems:
        print(
            f"\nREFUSING TO WRITE: {len(problems)} strike advances disagree with "
            "their BDF sources.\n",
            file=sys.stderr,
        )
        for problem in problems[:40]:
            print(f"  {problem}", file=sys.stderr)
        if len(problems) > 40:
            print(f"  ... and {len(problems) - 40} more", file=sys.stderr)
        return 1

    print(f"\n{len(FACES)} fonts written to {args.output}")
    return 0
