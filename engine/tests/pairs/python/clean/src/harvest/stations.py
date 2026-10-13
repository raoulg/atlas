"""The stations to harvest: a CSV file with an `id` column."""

import csv
from pathlib import Path


def read_stations(path: Path) -> list[str]:
    with path.open(newline="") as f:
        return [row["id"] for row in csv.DictReader(f)]
