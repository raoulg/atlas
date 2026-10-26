"""work() saves and then only verifies, on the file store itself."""

from pathlib import Path


class FileStore:
    def __init__(self) -> None:
        self.path = Path("data/store.txt")

    def save(self, text: str) -> None:
        self.path.write_text(text)

    def verify(self) -> None:
        if self.path.read_text() == "":
            raise RuntimeError("lost")


def work(s: FileStore) -> None:
    s.save("x")
    s.verify()


def run() -> None:
    work(FileStore())
