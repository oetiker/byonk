"""Put `fonts/` on sys.path so the tests can import the x11importer package."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
