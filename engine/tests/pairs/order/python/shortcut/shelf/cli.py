"""The program keeps its report on the shelf, sets up the drafts, and reads the report from the
shelf it has at hand last, the drafts, where it never kept one."""

from pathlib import Path

from shelf.store import Shelf


def open_shelf() -> Shelf:
    return Shelf(Path("out"))


def open_drafts() -> Shelf:
    return Shelf(Path("drafts"))


def main() -> None:
    shelf = open_shelf()
    shelf.keep_report("all well")
    drafts = open_drafts()
    print(drafts.report())
