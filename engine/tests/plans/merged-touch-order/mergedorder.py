"""Seed the note where asked, read it, then always write on it."""

import sys
from pathlib import Path

NOTE = Path("data/note.txt")


def run() -> None:
    if len(sys.argv) > 1:
        NOTE.write_text("seed")
    text = NOTE.read_text()
    NOTE.write_text(text + "!")
