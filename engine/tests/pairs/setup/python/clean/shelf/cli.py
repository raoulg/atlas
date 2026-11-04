"""The program sets the shelf up in a step of its own, which decides its folder, and then keeps
its report and its log on it."""

from pathlib import Path

from shelf.store import Shelf


def open_shelf() -> Shelf:
    return Shelf(Path("out"))


def main() -> None:
    shelf = open_shelf()
    shelf.keep_report("all well")
    shelf.keep_log("ran")
