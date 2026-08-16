"""Where the BDF sources come from, pinned by checksum.

The X.Org font packages ship the original `.bdf` files, so the importer reads
the same bytes the fonts were authored in. That is deliberately not Debian's
`.pcf.gz`: those are compiled, and recovering BDF from them adds a conversion
step between us and the ground truth for no gain.

Every tarball is pinned by version and SHA-256, so a rebuild years from now
either produces the same fonts or fails loudly.
"""

from __future__ import annotations

import hashlib
import tarfile
import urllib.request
from pathlib import Path

MIRROR = "https://www.x.org/releases/individual/font"

PACKAGES: dict[str, str] = {
    # Helvetica -> X11Helv
    "font-adobe-75dpi-1.0.4": "1281a62dbeded169e495cae1a5b487e1f336f2b4d971d92911c59c103999b911",
    "font-adobe-100dpi-1.0.4": "b67aff445e056328d53f9732d39884f55dd8d303fc25af3dbba33a8ba35a9ccf",
    # Lucida Sans -> X11LuSans
    "font-bh-75dpi-1.0.4": "6026d8c073563dd3cbb4878d0076eed970debabd21423b3b61dd90441b9e7cda",
    "font-bh-100dpi-1.0.4": "fd8f5efe8491faabdd2744808d3d4eafdae5c83e617017c7fddd2716d049ab1e",
    # Lucida Typewriter -> X11LuType
    "font-bh-lucidatypewriter-75dpi-1.0.4": "864e2c39ac61f04f693fc2c8aaaed24b298c2cd40283cec12eee459c5635e8f5",
    "font-bh-lucidatypewriter-100dpi-1.0.4": "76ec09eda4094a29d47b91cf59c3eba229c8f7d1ca6bae2abbb3f925e33de8f2",
    # DEC / Bitstream Terminal -> X11Term
    "font-bitstream-75dpi-1.0.4": "aaeb34d87424a9c2b0cf0e8590704c90cb5b42c6a3b6a0ef9e4676ef773bf826",
    "font-bitstream-100dpi-1.0.4": "2d1cc682efe4f7ebdf5fbd88961d8ca32b2729968728633dea20a1627690c1a7",
    # misc-fixed -> X11Misc5x .. X11Misc9x
    "font-misc-misc-1.1.3": "79abe361f58bb21ade9f565898e486300ce1cc621d5285bec26e14b6a8618fed",
    # Sony fixed -> X11Misc8x (16 px) and X11Misc12x
    "font-sony-misc-1.0.4": "e6b09f823fccb06e0bd0b2062283b6514153323bd8a7486e9c2e3f55ab84946b",
}


class ChecksumMismatch(RuntimeError):
    pass


def fetch(cache: Path) -> dict[str, Path]:
    """Download, verify and unpack every package; return package -> directory."""
    cache.mkdir(parents=True, exist_ok=True)
    unpacked = {}

    for package, digest in PACKAGES.items():
        archive = cache / f"{package}.tar.xz"
        if not archive.exists():
            urllib.request.urlretrieve(f"{MIRROR}/{archive.name}", archive)

        actual = hashlib.sha256(archive.read_bytes()).hexdigest()
        if actual != digest:
            raise ChecksumMismatch(
                f"{archive.name}: expected {digest}, got {actual}. "
                "Refusing to build fonts from an unexpected source."
            )

        directory = cache / package
        if not directory.exists():
            with tarfile.open(archive) as tar:
                tar.extractall(cache, filter="data")
        unpacked[package] = directory

    return unpacked
