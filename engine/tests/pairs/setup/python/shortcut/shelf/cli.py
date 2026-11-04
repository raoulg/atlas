"""The program sets the shelf up in a step of its own, which decides nothing, and then keeps its
report and its log on it."""

from shelf.store import Shelf


def open_shelf() -> Shelf:
    return Shelf()


def main() -> None:
    shelf = open_shelf()
    shelf.keep_report("all well")
    shelf.keep_log("ran")
