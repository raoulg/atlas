"""Read a file and show it: the file is what the route uses."""

from pathlib import Path

SRC = Path("data/in.txt")


def run() -> None:
    text = SRC.read_text()
    print(text.upper())
