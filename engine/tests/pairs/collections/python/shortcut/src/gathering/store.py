"""Readings kept in a file, one per line; read back as the lines."""

from dataclasses import dataclass
from pathlib import Path


@dataclass
class Reading:
    text: str


@dataclass
class Row:
    reading: Reading


class Store:
    def __init__(self, path: Path) -> None:
        self.path = path

    def put(self, reading: Reading) -> None:
        rows = self._rows()
        rows.append(Row(reading))
        self._write(rows)

    def get(self) -> list[str]:
        return self.path.read_text().splitlines()

    def _rows(self) -> list[Row]:
        return [Row(Reading(line)) for line in self.path.read_text().splitlines()]

    def _write(self, rows: list[Row]) -> None:
        self.path.write_text("\n".join(row.reading.text for row in rows))


class DroppedStore:
    def __init__(self, path: Path) -> None:
        self.path = path

    def put(self, reading: Reading) -> None:
        rows = self._rows()
        added: list[Row] = []
        added.append(Row(reading))
        self._write(rows)

    def get(self) -> list[str]:
        return self.path.read_text().splitlines()

    def _rows(self) -> list[Row]:
        return [Row(Reading(line)) for line in self.path.read_text().splitlines()]

    def _write(self, rows: list[Row]) -> None:
        self.path.write_text("\n".join(row.reading.text for row in rows))


class Frozen:
    def append(self, row: Row) -> "Frozen":
        return Frozen()


def observe(path: Path, frozen: Frozen, row: Row) -> None:
    path.exists()
    frozen.append(row)
