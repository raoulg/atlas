"""Write the note once per word (maybe never), then only check it is there."""

import sys
from pathlib import Path

NOTE = Path("data/note.txt")


def run() -> None:
    for word in sys.argv[1:]:
        NOTE.write_text(word)
    if NOTE.read_text() == "":
        raise RuntimeError("no note")
