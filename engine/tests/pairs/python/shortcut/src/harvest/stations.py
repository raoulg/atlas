"""The stations to harvest: a CSV file with an `id` column."""

import csv


def read_stations(path: str) -> list[str]:
    with open(path, newline="") as f:
        return [row["id"] for row in csv.DictReader(f)]
