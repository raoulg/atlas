"""For each word: a word with + replaces the note; every word shows the note.

The first read finds the note as it was; a later word writes over it.
"""

import sys
from pathlib import Path

NOTE = Path("data/note.txt")


def run() -> None:
    for word in sys.argv[1:]:
        if word.startswith("+"):
            NOTE.write_text(word)
        print(NOTE.read_text())
