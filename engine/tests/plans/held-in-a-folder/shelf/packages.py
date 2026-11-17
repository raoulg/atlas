"""The packages under a root: their files are the ones its own methods read, under the root its caller hands it."""

from pathlib import Path


class Packages:
    def __init__(self, root: Path) -> None:
        self.root = root

    def manifest(self, name: str) -> str:
        return (self.root / "packages" / name / "package.json").read_text()

    def lock(self, name: str) -> str:
        return (self.root / "packages" / name / "lock.json").read_text()

    def settings(self) -> str:
        return (self.root / "settings.json").read_text() + (self.root / "users.json").read_text()
