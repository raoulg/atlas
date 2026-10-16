"""The stations to harvest: a CSV file with an `id` column."""

import csv
from collections.abc import Sequence
from pathlib import Path


def read_stations(path: Path) -> list[str]:
    with path.open(newline="") as f:
        return [row["id"] for row in csv.DictReader(f)]


def archived_stations(roots: Sequence[Path]) -> list[str]:
    """The ids kept in the archives: <root>/<year>/station-<id>/."""
    return [d.name for d in (p for r in roots for p in sorted(r.glob("*/station-*")))]
