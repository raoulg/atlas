"""The shelf on disk: the store makes a Shelf, counts it itself, and hands it to the report."""

from dataclasses import dataclass
from pathlib import Path

from shelf.store.rows import Row, parse, ping


@dataclass
class Shelf:
    rows: list[Row]


def load(path: Path) -> Shelf:
    ping()
    return Shelf([parse(line) for line in path.read_text().splitlines()])


def count(shelf: Shelf) -> int:
    return len(shelf.rows)


def save(row: Row, path: Path) -> None:
    path.write_text(f"{row.title},{row.pages}\n")
