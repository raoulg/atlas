"""Readings kept in a file, one per line; read back as the lines."""

from dataclasses import dataclass
from pathlib import Path


@dataclass
class Reading:
    text: str


@dataclass
class Row:
    reading: Reading


def write_rows(path: Path, rows: list[Row]) -> None:
    path.write_text("\n".join(row.reading.text for row in rows))


class Store:
    def __init__(self, path: Path) -> None:
        self.path = path

    def put(self, reading: Reading) -> None:
        rows = self._rows()
        rows.append(Row(reading))
        write_rows(self.path, rows)

    def put_all(self, readings: list[Reading]) -> None:
        rows = self._rows()
        for reading in readings:
            rows.append(Row(reading))
        write_rows(self.path, rows)

    def get(self) -> list[str]:
        return self.path.read_text().splitlines()

    def _rows(self) -> list[Row]:
        return [Row(Reading(line)) for line in self.path.read_text().splitlines()]



class DroppedStore:
    def __init__(self, path: Path) -> None:
        self.path = path

    def put(self, reading: Reading) -> None:
        rows = self._rows()
        added: list[Row] = []
        added.append(Row(reading))
        write_rows(self.path, rows)

    def put_all(self, readings: list[Reading]) -> None:
        rows = self._rows()
        added: list[Row] = []
        for reading in readings:
            added.append(Row(reading))
        write_rows(self.path, rows)

    def get(self) -> list[str]:
        return self.path.read_text().splitlines()

    def _rows(self) -> list[Row]:
        return [Row(Reading(line)) for line in self.path.read_text().splitlines()]



class Frozen:
    def append(self, row: Row) -> "Frozen":
        return Frozen()


def observe(path: Path, frozen: Frozen, row: Row) -> None:
    path.exists()
    frozen.append(row)
