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


class Exporter[R](ABC):
    """Writes a station's records to a file in its own format."""

    suffix: str

    @abstractmethod
    def write(self, records: list[R], path: Path) -> None: ...

    def export(self, records: list[R], directory: Path, station: str) -> Path:
        path = directory / f"{station}{self.suffix}"
        self.write(records, path)
        return path


class JsonExporter(Exporter[Reading]):
    suffix = ".json"

    def write(self, records: list[Reading], path: Path) -> None:
        path.write_text(json.dumps([row(r) for r in records], indent=2))


class CsvExporter(Exporter[Reading]):
    suffix = ".csv"

    def write(self, records: list[Reading], path: Path) -> None:
        with path.open("w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=FIELDS)
            writer.writeheader()
            writer.writerows(row(r) for r in records)


def exporter_for(report_format: ReportFormat) -> Exporter[Reading]:
    match report_format:
        case ReportFormat.JSON:
            return JsonExporter()
        case ReportFormat.CSV:
            return CsvExporter()
