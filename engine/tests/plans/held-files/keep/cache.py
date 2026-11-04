"""The cache: its files are the ones its own methods write, under the root its caller hands it."""

from pathlib import Path


class Cache:
    def __init__(self, root: Path) -> None:
        self.root = root

    def save(self, name: str, text: str) -> None:
        (self.root / f"{name}.txt").write_text(text)
        (self.root / "index.json").write_text(name)
