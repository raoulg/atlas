"""Build a store and never call its methods: building it writes and reads nothing."""

from pathlib import Path


class Store:
    def __init__(self, path: str) -> None:
        self.path = Path(path)

    def save(self, text: str) -> None:
        self.path.write_text(text)

    def load(self) -> str:
        return self.path.read_text()


def run() -> None:
    store = Store("data/store.txt")
    print(store.path)
