"""A shelf of files the program keeps under one folder: the folder is set up once, each method
names its own file in it, and one function writes them all."""

from pathlib import Path


class Shelf:
    def __init__(self, root: Path) -> None:
        self.root = root

    def keep_report(self, text: str) -> None:
        write(self.root / "report.txt", text)

    def keep_log(self, text: str) -> None:
        write(self.root / "log.txt", text)


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")
