"""A table kept as CSV: `save` writes its text, `load` reads it back as a frame, both at `TABLE` unless told."""

from table.store import load, save

__all__ = ["load", "save"]
