"""Two stores built on different directories: each writes under its own."""

from pathlib import Path


class Store:
    def __init__(self, root: str) -> None:
        self.root = Path(root)

    def put(self, text: str) -> None:
        (self.root / "item.txt").write_text(text)


def run() -> None:
    Store("data/a").put("x")
    Store("data/b").put("y")
