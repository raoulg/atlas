"""A shelf of files the program keeps under one folder: the folder is set up once, and each method
writes its own file in it."""

from pathlib import Path


class Shelf:
    def __init__(self, root: Path) -> None:
        self.root = root

    def keep_report(self, text: str) -> None:
        (self.root / "report.txt").write_text(text, encoding="utf-8")

    def keep_log(self, text: str) -> None:
        (self.root / "log.txt").write_text(text, encoding="utf-8")
