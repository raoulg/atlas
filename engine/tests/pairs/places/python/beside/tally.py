"""Counts the lines of a file and keeps the count where the command line says."""

from pathlib import Path


def count(source: Path) -> int:
    return len(source.read_text().splitlines())


def keep(total: int, out: Path) -> None:
    out.write_text(f"{total}\n")
