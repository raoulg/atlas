"""Each function says what it gives back, so its callers' values have a type."""

from pathlib import Path


class Store:
    def __init__(self, path: Path) -> None:
        self.path = path

    def history(self) -> str:
        return self.path.read_text()

    @property
    def size(self) -> int:
        return len(self.history())


def get_store(path: Path) -> Store:
    return Store(path)


def score(reading: int) -> int:
    return -reading


def ranked(readings: list[int]) -> list[int]:
    return sorted(readings, key=score)


def main(path: Path) -> int:
    store = get_store(path)
    return len(store.history()) + store.size + len(ranked([3, 1]))
