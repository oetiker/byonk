#!/usr/bin/env python3
"""Convert X11 bitmap fonts to consolidated TTF files with embedded bitmap strikes.

Uses the mkttf approach: import all BDF sizes into one font per family+style,
import the largest into the glyph background, then autoTrace for scalable outlines.
Resvg's skrifa branch selects bitmap strikes by ppem automatically.

Run on a machine with X11 bitmap fonts installed:
  fontforge -lang=py -script x11-importer.py
"""

import fontforge
import os
import sys
import re
import shutil
import tempfile
from pathlib import Path
from collections import defaultdict

OUTPUT_DIR = Path("./ttf_output")
TEMP_DIR = None  # set in main()

# ── Proportional font families (75dpi + 100dpi) ──────────────────────────
# Map filename prefix → (family, style)
PROPORTIONAL = {
    'helvR':   ('X11Helv', 'Regular'),
    'helvB':   ('X11Helv', 'Bold'),
    'helvO':   ('X11Helv', 'Oblique'),
    'helvBO':  ('X11Helv', 'BoldOblique'),
    'luRS':    ('X11LuSans', 'Regular'),
    'luBS':    ('X11LuSans', 'Bold'),
    'luIS':    ('X11LuSans', 'Oblique'),
    'luBIS':   ('X11LuSans', 'BoldOblique'),
    'lubR':    ('X11LuSans', 'Regular'),
    'lubB':    ('X11LuSans', 'Bold'),
    'lubI':    ('X11LuSans', 'Oblique'),
    'lubBI':   ('X11LuSans', 'BoldOblique'),
    'lutRS':   ('X11LuType', 'Regular'),
    'lutBS':   ('X11LuType', 'Bold'),
    'termB':   ('X11Term', 'Bold'),
    'term':    ('X11Term', 'Regular'),  # must come after termB
}

# ── Misc fixed-width fonts grouped by cell width ─────────────────────────
# filename (without .pcf.gz) → (family, style)
MISC_SKIP_PREFIXES = ('cl', 'cu', 'olgl', 'ol', 'nil', 'micro', 'dec',
                       'cursor', 'arabic', 'hangl', 'jiskan', 'gb', 'k14',
                       '12x13ja', '18x18ja', '18x18ko', '12x24rk', '8x16rk')

MISC_MAP = {
    # 5x fonts
    '4x6':   ('X11Misc5x', 'Regular', 6),   # cell width ~5, height 6 → not really 5x but close
    '5x7':   ('X11Misc5x', 'Regular', 7),
    '5x8':   ('X11Misc5x', 'Regular', 8),
    # 6x fonts
    '6x9':   ('X11Misc6x', 'Regular', 9),
    '6x10':  ('X11Misc6x', 'Regular', 10),
    '6x12':  ('X11Misc6x', 'Regular', 12),
    '6x13':  ('X11Misc6x', 'Regular', 13),
    '6x13B': ('X11Misc6x', 'Bold', 13),
    '6x13O': ('X11Misc6x', 'Oblique', 13),
    # 7x fonts
    '7x13':  ('X11Misc7x', 'Regular', 13),
    '7x13B': ('X11Misc7x', 'Bold', 13),
    '7x13O': ('X11Misc7x', 'Oblique', 13),
    '7x14':  ('X11Misc7x', 'Regular', 14),
    '7x14B': ('X11Misc7x', 'Bold', 14),
    # 8x fonts
    '8x13':  ('X11Misc8x', 'Regular', 13),
    '8x13B': ('X11Misc8x', 'Bold', 13),
    '8x13O': ('X11Misc8x', 'Oblique', 13),
    '8x16':  ('X11Misc8x', 'Regular', 16),
    # 9x fonts
    '9x15':  ('X11Misc9x', 'Regular', 15),
    '9x15B': ('X11Misc9x', 'Bold', 15),
    '9x18':  ('X11Misc9x', 'Regular', 18),
    '9x18B': ('X11Misc9x', 'Bold', 18),
    # 10x fonts
    '10x20': ('X11Misc10x', 'Regular', 20),
    # 12x fonts
    '12x24': ('X11Misc12x', 'Regular', 24),
}


def pcf_to_bdf(pcf_path, bdf_path):
    """Convert a .pcf.gz to .bdf via gunzip + pcf2bdf."""
    pcf_tmp = bdf_path.replace('.bdf', '.pcf')
    os.system(f"gunzip -c '{pcf_path}' > '{pcf_tmp}'")
    os.system(f"pcf2bdf -o '{bdf_path}' '{pcf_tmp}'")
    os.remove(pcf_tmp)
    return bdf_path


def get_pixel_size(bdf_path):
    """Read PIXEL_SIZE from a BDF file."""
    with open(bdf_path, 'r', errors='replace') as f:
        for line in f:
            if line.startswith('PIXEL_SIZE '):
                return int(line.split()[1])
            if line.startswith('STARTCHAR'):
                break
    return 0


def extract_bdf_metadata(bdf_path):
    """Extract COPYRIGHT and NOTICE from BDF file."""
    meta = {'copyright': '', 'notice': '', 'foundry': ''}
    with open(bdf_path, 'r', errors='replace') as f:
        for line in f:
            if line.startswith('COPYRIGHT '):
                meta['copyright'] = line[10:].strip().strip('"')
            elif line.startswith('NOTICE '):
                meta['notice'] = line[7:].strip().strip('"')
            elif line.startswith('FOUNDRY '):
                meta['foundry'] = line[8:].strip().strip('"')
            elif line.startswith('STARTCHAR'):
                break
    return meta


def classify_proportional(basename):
    """Return (family, style) for a proportional font basename, or None."""
    # Sort prefixes longest-first so 'helvBO' matches before 'helvB'
    for prefix in sorted(PROPORTIONAL.keys(), key=len, reverse=True):
        if basename.startswith(prefix):
            return PROPORTIONAL[prefix]
    return None


def collect_proportional_fonts():
    """Collect BDF files from 75dpi and 100dpi, grouped by (family, style)."""
    groups = defaultdict(list)  # (family, style) → [(pixel_size, bdf_path)]

    for dpi in [75, 100]:
        font_dir = Path(f"/usr/share/fonts/X11/{dpi}dpi")
        if not font_dir.exists():
            continue

        for pcf_path in sorted(font_dir.glob("*.pcf.gz")):
            basename = pcf_path.name.replace('.pcf.gz', '')
            if '-ISO8859' in basename or '-KOI8' in basename or '-JISX' in basename:
                continue

            result = classify_proportional(basename)
            if not result:
                continue

            family, style = result
            bdf_path = os.path.join(TEMP_DIR, f"{basename}_{dpi}dpi.bdf")
            pcf_to_bdf(str(pcf_path), bdf_path)
            px = get_pixel_size(bdf_path)
            if px > 0:
                groups[(family, style)].append((px, bdf_path))

    return groups


def collect_misc_fonts():
    """Collect BDF files from misc, grouped by (family, style)."""
    groups = defaultdict(list)  # (family, style) → [(pixel_size, bdf_path)]
    font_dir = Path("/usr/share/fonts/X11/misc")
    if not font_dir.exists():
        return groups

    for pcf_path in sorted(font_dir.glob("*.pcf.gz")):
        basename = pcf_path.name.replace('.pcf.gz', '')
        if '-ISO8859' in basename or '-KOI8' in basename or '-JISX' in basename:
            continue
        if any(basename.startswith(p) for p in MISC_SKIP_PREFIXES):
            continue

        if basename not in MISC_MAP:
            continue

        family, style, px = MISC_MAP[basename]
        bdf_path = os.path.join(TEMP_DIR, f"misc_{basename}.bdf")
        pcf_to_bdf(str(pcf_path), bdf_path)
        actual_px = get_pixel_size(bdf_path)
        if actual_px > 0:
            px = actual_px
        groups[(family, style)].append((px, bdf_path))

    return groups


def build_font(family, style, bdf_entries):
    """Build a consolidated TTF from multiple BDF files for one family+style.

    bdf_entries: list of (pixel_size, bdf_path), sorted ascending by pixel_size.
    Returns output filename or None.
    """
    if not bdf_entries:
        return None

    # Sort by pixel size ascending
    bdf_entries.sort(key=lambda x: x[0])

    # Deduplicate: keep first occurrence of each pixel size
    seen_px = set()
    unique = []
    for px, path in bdf_entries:
        if px not in seen_px:
            seen_px.add(px)
            unique.append((px, path))
    bdf_entries = unique

    smallest_px, smallest_bdf = bdf_entries[0]
    largest_px, largest_bdf = bdf_entries[-1]

    # Extract metadata from the first BDF
    metadata = extract_bdf_metadata(smallest_bdf)

    # Open the smallest as base font
    font = fontforge.open(smallest_bdf)

    # Import all other sizes
    for px, bdf_path in bdf_entries[1:]:
        font.importBitmaps(bdf_path)

    # Import largest into glyph background for autoTrace
    font.importBitmaps(largest_bdf, True)

    print(f"  Bitmap strikes: {font.bitmapSizes}", file=sys.stderr)

    # Set em based on largest strike for good outline quality
    font.em = largest_px * 100

    # AutoTrace: convert background bitmaps to scalable outlines
    fontforge.setPrefs("PreferPotrace", True)
    font.selection.all()
    font.autoTrace()
    font.addExtrema()
    font.simplify()

    # Round glyph widths to multiples of (em/largest_px) for pixel alignment
    scale = font.em // largest_px
    for glyph in font.glyphs():
        glyph.width = round(glyph.width / scale) * scale

    # Set naming metadata — single family name, no per-size suffix
    font.familyname = family
    font.fontname = f"{family}-{style}"
    font.fullname = f"{family} {style}"

    # Set weight
    if 'Bold' in style:
        font.weight = "Bold"
        font.os2_weight = 700
    else:
        font.weight = "Regular"
        font.os2_weight = 400

    # Set OS/2 style bits
    if style == 'BoldOblique':
        font.os2_stylemap = 0x221
        font.macstyle = 0x3
    elif style == 'Bold':
        font.os2_stylemap = 0x20
        font.macstyle = 0x1
    elif style == 'Oblique':
        font.os2_stylemap = 0x201
        font.macstyle = 0x2
    else:
        font.os2_stylemap = 0x40
        font.macstyle = 0x0

    # SFNT name table
    subfamily = style.replace('Oblique', 'Italic').replace('BoldItalic', 'Bold Italic')
    font.appendSFNTName('English (US)', 'Family', family)
    font.appendSFNTName('English (US)', 'SubFamily', subfamily)
    font.appendSFNTName('English (US)', 'Fullname', f"{family} {style}")
    font.appendSFNTName('English (US)', 'PostScriptName', f"{family}-{style}")

    # Preserve copyright
    if metadata['copyright']:
        font.copyright = metadata['copyright']

    comments = []
    if metadata['notice']:
        comments.append(metadata['notice'])
    if metadata['foundry']:
        comments.append(f"Original foundry: {metadata['foundry']}")
    comments.append("Converted from X11 bitmap fonts — all sizes as embedded bitmap strikes")
    font.comment = '\n'.join(comments)

    # Generate TTF with embedded bitmaps
    ttf_name = f"{family}-{style}.ttf"
    ttf_path = OUTPUT_DIR / ttf_name
    font.generate(str(ttf_path), bitmap_type='otf')
    font.close()

    return ttf_name


def main():
    global TEMP_DIR
    TEMP_DIR = tempfile.mkdtemp(prefix='x11font_')

    OUTPUT_DIR.mkdir(exist_ok=True)

    print("Collecting proportional fonts from 75dpi + 100dpi...", file=sys.stderr)
    prop_groups = collect_proportional_fonts()

    print("Collecting misc fixed-width fonts...", file=sys.stderr)
    misc_groups = collect_misc_fonts()

    # Merge all groups
    all_groups = {}
    all_groups.update(prop_groups)
    all_groups.update(misc_groups)

    results = []
    for (family, style), entries in sorted(all_groups.items()):
        sizes = sorted(set(px for px, _ in entries))
        print(f"\n{family} {style}: {len(entries)} BDFs, sizes {sizes}", file=sys.stderr)
        try:
            ttf_name = build_font(family, style, entries)
            if ttf_name:
                results.append(ttf_name)
                print(f"  → {ttf_name}")
        except Exception as e:
            print(f"  ERROR: {e}", file=sys.stderr)

    shutil.rmtree(TEMP_DIR)

    print(f"\nGenerated {len(results)} fonts in {OUTPUT_DIR}/")
    print("\nFile sizes:")
    for ttf in sorted(OUTPUT_DIR.glob("*.ttf")):
        size = ttf.stat().st_size
        print(f"  {ttf.name}: {size:,} bytes")


if __name__ == "__main__":
    main()
