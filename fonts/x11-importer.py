#!/usr/bin/env python3
"""Rebuild byonk's X11 bitmap fonts from their original BDF sources.

    make fonts-setup     # once, creates .venv-fonts
    make fonts           # rebuild the 26 TTFs in place

Needs only Python and fontTools -- no FontForge, no potrace, and no X11
installation. See fonts/x11importer/ for the pieces and fonts/FONTS.md for what
gets built.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from x11importer.cli import main  # noqa: E402

if __name__ == "__main__":
    raise SystemExit(main())
