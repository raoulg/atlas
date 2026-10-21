from pathlib import Path


class Store:
    def __init__(self, root: Path) -> None:
        self.root = root

    def get(self, key: str) -> bytes:
        return (self.root / key).read_bytes()
