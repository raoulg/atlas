"""Open a file with a mode the code does not fix: which way it goes is not said."""

import sys

PATH = "data/notes.txt"


def touch(mode: str) -> None:
    with open(PATH, mode) as f:
        print(f)


def run() -> None:
    touch(sys.argv[1])
