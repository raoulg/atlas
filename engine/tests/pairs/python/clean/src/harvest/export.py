"""Reports of a station's readings, one file each, in the format the settings name."""

import csv
import json
from abc import ABC, abstractmethod
from pathlib import Path

from harvest.model import Reading
from harvest.settings import ReportFormat

FIELDS = ["station", "taken_at", "temperature", "humidity", "unit"]


def row(reading: Reading) -> dict[str, str | float]:
    return {
        "station": reading.station,
        "taken_at": reading.taken_at.isoformat(),
        "temperature": reading.temperature,
        "humidity": reading.humidity,
        "unit": reading.unit.symbol,
    }


class Exporter(ABC):
    """Writes a station's readings to a file in its own format."""

    suffix: str

    @abstractmethod
    def write(self, readings: list[Reading], path: Path) -> None: ...

    def export(self, readings: list[Reading], directory: Path, station: str) -> Path:
        path = directory / f"{station}{self.suffix}"
        self.write(readings, path)
        return path


class JsonExporter(Exporter):
    suffix = ".json"

    def write(self, readings: list[Reading], path: Path) -> None:
        path.write_text(json.dumps([row(r) for r in readings], indent=2))


class CsvExporter(Exporter):
    suffix = ".csv"

    def write(self, readings: list[Reading], path: Path) -> None:
        with path.open("w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=FIELDS)
            writer.writeheader()
            writer.writerows(row(r) for r in readings)


def exporter_for(report_format: ReportFormat) -> Exporter:
    match report_format:
        case ReportFormat.JSON:
            return JsonExporter()
        case ReportFormat.CSV:
            return CsvExporter()
