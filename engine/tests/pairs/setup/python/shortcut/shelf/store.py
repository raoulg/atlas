"""A shelf of files the program keeps under one folder: the folder is a constant of the module,
which each method reads itself, and one function writes them all."""

from pathlib import Path

OUT = Path("out")


class Shelf:
    def keep_report(self, text: str) -> None:
        write(OUT / "report.txt", text)

    def keep_log(self, text: str) -> None:
        write(OUT / "log.txt", text)


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")
