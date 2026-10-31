"""The program keeps its report on the shelf and reads it back from that shelf; only then does it
set up the drafts, whose folder it names."""

from pathlib import Path

from shelf.store import Shelf


def open_shelf() -> Shelf:
    return Shelf(Path("out"))


def open_drafts() -> Shelf:
    return Shelf(Path("drafts"))


def review() -> str:
    shelf = open_shelf()
    shelf.keep_report("all well")
    report = shelf.report()
    drafts = open_drafts()
    print(drafts.root)
    return report


def main() -> None:
    print(review())
