"""A table kept as CSV: `save` writes its text, `load` reads it back as a frame, each where its caller says."""

from table.store import load, save

__all__ = ["load", "save"]
