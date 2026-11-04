"""The command: load the shelf, show it, keep its first row."""

from pathlib import Path

from shelf.report.show import render
from shelf.report.total import total
from shelf.store.disk import load, save
from shelf.store.rows import parse


def main() -> None:
    shelf = load(Path("shelf.csv"))
    render(shelf)
    total(shelf)
    save(parse("first,1"), Path("first.csv"))
