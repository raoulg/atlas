"""work() saves and then only verifies, through the base class; the file store does the IO."""
from pathlib import Path

class Store:
    def save(self, text: str) -> None:
        raise NotImplementedError

    def verify(self) -> None:
        raise NotImplementedError

class FileStore(Store):
    def __init__(self) -> None:
        self.path = Path("data/store.txt")

    def save(self, text: str) -> None:
        self.path.write_text(text)

    def verify(self) -> None:
        if self.path.read_text() == "":
            raise RuntimeError("lost")

def work(s: Store) -> None:
    s.save("x")
    s.verify()

def run() -> None:
    work(FileStore())
