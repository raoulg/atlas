from pathlib import Path

from shelf.store import Shelf


def main() -> None:
    shelf = Shelf(Path("out"))
    shelf.keep_report("all well")
    shelf.keep_log("ran")
