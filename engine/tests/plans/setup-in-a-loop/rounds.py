"""Each round keeps its report on the shelf the round before set up: the first round on the drafts, every later one on
the shelf the round before opened."""

from pathlib import Path


class Shelf:
    def __init__(self, root: Path) -> None:
        self.root = root

    def keep_report(self, text: str) -> None:
        write(self.root / "report.txt", text)


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")


def open_drafts() -> Shelf:
    return Shelf(Path("drafts"))


def open_shelf() -> Shelf:
    return Shelf(Path("out"))


def main() -> None:
    shelf = open_drafts()
    for text in ("first", "final"):
        shelf.keep_report(text)
        shelf = open_shelf()
