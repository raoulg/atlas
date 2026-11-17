"""The program files a note on the archive, a shelf a step of its own sets up and hands on, and
reads the index of the drafts, a shelf it builds where it reads: each builder names its folder."""

from pathlib import Path

from notes.shelf import Shelf


def open_archive() -> Shelf:
    return Shelf(Path("archive"))


def file_note(shelf: Shelf, text: str) -> None:
    shelf.keep(text)


def read_index() -> str:
    return Shelf(Path("drafts")).index().read_text(encoding="utf-8")


def main() -> None:
    file_note(open_archive(), "filed")
    print(read_index())
