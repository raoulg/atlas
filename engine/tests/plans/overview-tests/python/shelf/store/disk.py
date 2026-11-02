"""The shelf on disk: the store makes a Shelf and hands it to the report."""

from dataclasses import dataclass
from pathlib import Path


@dataclass
class Shelf:
    titles: list[str]


def load(path: Path) -> Shelf:
    return Shelf(path.read_text().splitlines())
