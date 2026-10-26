"""For each word: show the note, a word with + written into it first.

`show` may write the note before it reads it, and the loop runs `show` again: a later word writes over what an earlier
one read.
"""

import sys
from pathlib import Path

NOTE = Path("data/note.txt")


def show(word: str) -> None:
    if word.startswith("+"):
        NOTE.write_text(word)
    print(NOTE.read_text())


def run() -> None:
    for word in sys.argv[1:]:
        show(word)
