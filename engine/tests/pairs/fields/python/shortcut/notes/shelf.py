"""A shelf the program keeps its notes on: the shelf is handed a name and puts its folder under
one it names itself, each method names its own file in it, and one function writes them."""

from pathlib import Path


class Shelf:
    def __init__(self, name: str) -> None:
        self.root = Path("shelves") / name

    def index(self) -> Path:
        return self.root / "index.txt"

    def keep(self, text: str) -> None:
        write(self.root / "note.txt", text)


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")
