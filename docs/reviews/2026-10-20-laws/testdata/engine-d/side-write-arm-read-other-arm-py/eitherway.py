"""Either write the note or show it: the two never run together."""

import sys
from pathlib import Path

NOTE = Path("data/note.txt")


def run() -> None:
    if len(sys.argv) > 1:
        NOTE.write_text(sys.argv[1])
    else:
        print(NOTE.read_text())
