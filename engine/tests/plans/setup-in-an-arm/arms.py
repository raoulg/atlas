"""Where the program keeps anything, it sets the shelf up first; it keeps the report on it only where it reports too."""

from pathlib import Path

KEEP = True
REPORT = True


class Shelf:
    def __init__(self, root: Path) -> None:
        self.root = root

    def keep_report(self, text: str) -> None:
        write(self.root / "report.txt", text)


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")


def open_shelf() -> Shelf:
    return Shelf(Path("out"))


def main() -> None:
    if KEEP:
        shelf = open_shelf()
        if REPORT:
            shelf.keep_report("all well")
