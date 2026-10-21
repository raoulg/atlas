from pathlib import Path


class Store:
    def __init__(self, root: Path) -> None:
        self.root = root

    def has(self, key: str) -> bool:
        return (self.root / key).exists()

    def get(self, key: str) -> bytes:
        return (self.root / key).read_bytes()

    def put(self, key: str, data: bytes) -> None:
        (self.root / key).write_bytes(data)
