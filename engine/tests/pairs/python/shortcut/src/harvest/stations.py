"""The stations to harvest: a CSV file with an `id` column."""

import csv
from pathlib import Path


def read_stations(path: str) -> list[str]:
    with open(path, newline="") as f:
        return [row["id"] for row in csv.DictReader(f)]


def archived_stations(roots: list[Path]) -> list[str]:
    """The ids kept in the archives: <root>/<year>/station-<id>/."""
    return [d.name for d in (p for r in roots for p in sorted(r.glob("*/station-*")))]
