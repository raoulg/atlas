"""Reports of a station's readings, one file each."""

import csv
import json
from abc import ABC, abstractmethod
from pathlib import Path

FIELDS = ["station", "taken_at", "temperature", "humidity", "unit"]


class Exporter(ABC):
    """Writes a station's rows to a file in its own format."""

    suffix: str

    @abstractmethod
    def write(self, rows: list[dict], path: Path) -> None: ...

    def export(self, rows: list[dict], directory: Path, station: str) -> Path:
        path = directory / f"{station}{self.suffix}"
        self.write(rows, path)
        return path


class JsonExporter(Exporter):
    suffix = ".json"

    def write(self, rows: list[dict], path: Path) -> None:
        path.write_text(json.dumps(rows, indent=2))


class CsvExporter(Exporter):
    suffix = ".csv"

    def write(self, rows: list[dict], path: Path) -> int:
        with path.open("w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=FIELDS)
            writer.writeheader()
            writer.writerows(rows)
        return len(rows)


def exporter_for(fmt: str) -> Exporter:
    if fmt == "csv":
        return CsvExporter()
    return JsonExporter()
