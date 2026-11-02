"""The command: load the shelf and show it."""

from pathlib import Path

from shelf.report.show import render
from shelf.store.disk import load


def main() -> None:
    render(load(Path("shelf.csv")))
