"""Which BDF files make up which shipped font.

Each face names its sources as (package, prefix) pairs and collects every BDF
whose stem is that prefix followed only by digits. Anchoring on digits matters:
the old importer matched on `startswith` and had to sort its prefixes
longest-first so that `helvBO` was tried before `helvB`. `helvB\\d*$` simply
cannot match `helvBO08`, so the ordering hack is gone.

Two things the old mapping got wrong and this one does not:

* `lub*` (LucidaBright, a **serif**) was mapped into `X11LuSans` alongside
  `luRS*` (Lucida Sans). Only a size-deduplication accident kept serif glyphs
  out of the sans font. LucidaBright has no size Lucida Sans lacks, so dropping
  it changes nothing in the output and removes the trap.
* `X11Misc*` groups by cell width, which is not a licence grouping and not a
  foundry grouping: `X11Misc8x` takes 8x13 from misc-fixed (public domain) and
  8x16 from Sony. Notices therefore have to be per source, not per family.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

MISC = "font-misc-misc-1.1.3"
SONY = "font-sony-misc-1.0.4"
ADOBE = ("font-adobe-75dpi-1.0.4", "font-adobe-100dpi-1.0.4")
BH = ("font-bh-75dpi-1.0.4", "font-bh-100dpi-1.0.4")
LUTYPE = (
    "font-bh-lucidatypewriter-75dpi-1.0.4",
    "font-bh-lucidatypewriter-100dpi-1.0.4",
)
BITSTREAM = ("font-bitstream-75dpi-1.0.4", "font-bitstream-100dpi-1.0.4")


@dataclass(frozen=True)
class Face:
    family: str
    style: str
    sources: tuple[tuple[str, str], ...]
    """(package, BDF stem prefix) pairs; the stem must be prefix + digits."""

    @property
    def filename(self) -> str:
        return f"{self.family}-{self.style}.ttf"


def _both(packages, prefix):
    return tuple((package, prefix) for package in packages)


FACES: tuple[Face, ...] = (
    # ── Proportional, 75dpi + 100dpi ────────────────────────────────────
    Face("X11Helv", "Regular", _both(ADOBE, "helvR")),
    Face("X11Helv", "Bold", _both(ADOBE, "helvB")),
    Face("X11Helv", "Oblique", _both(ADOBE, "helvO")),
    Face("X11Helv", "BoldOblique", _both(ADOBE, "helvBO")),
    Face("X11LuSans", "Regular", _both(BH, "luRS")),
    Face("X11LuSans", "Bold", _both(BH, "luBS")),
    Face("X11LuSans", "Oblique", _both(BH, "luIS")),
    Face("X11LuSans", "BoldOblique", _both(BH, "luBIS")),
    Face("X11LuType", "Regular", _both(LUTYPE, "lutRS")),
    Face("X11LuType", "Bold", _both(LUTYPE, "lutBS")),
    # DEC Terminal at 75dpi is 14 px; the same file at 100dpi is Bitstream
    # Terminal at 18 px. That is where X11Term's two strikes come from.
    Face("X11Term", "Regular", _both(BITSTREAM, "term")),
    Face("X11Term", "Bold", _both(BITSTREAM, "termB")),
    # ── Fixed width, grouped by cell width ──────────────────────────────
    Face("X11Misc5x", "Regular", ((MISC, "4x6"), (MISC, "5x7"), (MISC, "5x8"))),
    Face(
        "X11Misc6x",
        "Regular",
        ((MISC, "6x9"), (MISC, "6x10"), (MISC, "6x12"), (MISC, "6x13")),
    ),
    Face("X11Misc6x", "Bold", ((MISC, "6x13B"),)),
    Face("X11Misc6x", "Oblique", ((MISC, "6x13O"),)),
    Face("X11Misc7x", "Regular", ((MISC, "7x13"), (MISC, "7x14"))),
    Face("X11Misc7x", "Bold", ((MISC, "7x13B"), (MISC, "7x14B"))),
    Face("X11Misc7x", "Oblique", ((MISC, "7x13O"),)),
    Face("X11Misc8x", "Regular", ((MISC, "8x13"), (SONY, "8x16"))),
    Face("X11Misc8x", "Bold", ((MISC, "8x13B"),)),
    Face("X11Misc8x", "Oblique", ((MISC, "8x13O"),)),
    Face("X11Misc9x", "Regular", ((MISC, "9x15"), (MISC, "9x18"))),
    Face("X11Misc9x", "Bold", ((MISC, "9x15B"), (MISC, "9x18B"))),
    Face("X11Misc10x", "Regular", ((MISC, "10x20"),)),
    Face("X11Misc12x", "Regular", ((SONY, "12x24"),)),
)


def bdf_files(face: Face, packages: dict[str, Path]) -> list[Path]:
    """Every BDF belonging to `face`, in a stable order."""
    found: list[Path] = []
    for package, prefix in face.sources:
        directory = packages[package]
        pattern = re.compile(re.escape(prefix) + r"\d*$")
        matches = sorted(
            path for path in directory.glob("*.bdf") if pattern.fullmatch(path.stem)
        )
        if not matches:
            raise FileNotFoundError(f"{face.filename}: no {prefix}*.bdf in {package}")
        found.extend(matches)
    return found
