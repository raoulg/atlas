"""Readings kept in a file, one per line, and read back as readings."""

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

    def get(self) -> list[Reading]:
        return [row.reading for row in self._rows()]

    def _rows(self) -> list[Row]:
        return [Row(Reading(line)) for line in self.path.read_text().splitlines()]

    def _write(self, rows: list[Row]) -> None:
        self.path.write_text("\n".join(row.reading.text for row in rows))
