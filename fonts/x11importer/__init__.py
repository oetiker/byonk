"""Convert X11 BDF bitmap fonts into bitmap-only OpenType files.

No FontForge, no potrace, no X11 installation: the sfnt tables are written
directly with fontTools, and every strike keeps the pixel advance (`DWIDTH`)
its source BDF declared.
"""
