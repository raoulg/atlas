"""Check the file is not empty, then write over it: the read only decides."""

from pathlib import Path

NOTE = Path("data/note.txt")


def run() -> None:
    if NOTE.read_text() == "":
        raise RuntimeError("empty")
    NOTE.write_text("seen")
