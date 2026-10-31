"""Write a line to the log of an archive."""

from pathlib import Path


class Archive:
    def __init__(self, root: str) -> None:
        self.root = root

    def home(self) -> Path:
        return Path(self.root)


def log(archive: Archive, line: str) -> None:
    with open(archive.home() / "log.txt", "a") as f:
        f.write(line + "\n")


def main() -> None:
    log(Archive("archive"), "done")
