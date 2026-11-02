"""A shelf of files the program keeps under one folder: the shelf is handed its folder, and each
method names its file in it and hands the path to the function that writes or reads it."""

from pathlib import Path


class Shelf:
    def __init__(self, root: Path) -> None:
        self.root = root

    def keep_report(self, text: str) -> None:
        write(self.root / "report.txt", text)

    def report(self) -> str:
        return read(self.root / "report.txt")


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")
