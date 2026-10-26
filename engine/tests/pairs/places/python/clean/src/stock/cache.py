"""A cache and a bin that keep what they are handed under the folder they are made with."""

from dataclasses import dataclass
from pathlib import Path


class Cache:
    def __init__(self, root: Path = Path(".cache")) -> None:
        self.root = Path(root)

    def keep(self, key: str, text: str) -> None:
        (self.root / f"{key}.txt").write_text(text)


def warm(root: Path) -> None:
    Cache(root).keep("a", "b")


@dataclass
class Bin:
    root: Path = Path("bin")

    def drop(self, name: str) -> None:
        (self.root / name).write_text(name)


def tidy(root: Path) -> None:
    Bin(root=root).drop("x.txt")
