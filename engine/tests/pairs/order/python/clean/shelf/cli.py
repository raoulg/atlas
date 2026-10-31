"""The program keeps its report on the shelf and reads it back from that shelf; only then does it
set up the drafts, whose folder it names."""

from pathlib import Path

from shelf.store import Shelf


def open_shelf() -> Shelf:
    return Shelf(Path("out"))


def open_drafts() -> Shelf:
    return Shelf(Path("drafts"))


def main() -> None:
    shelf = open_shelf()
    shelf.keep_report("all well")
    print(shelf.report())
    drafts = open_drafts()
    print(drafts.root)
