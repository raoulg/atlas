"""Ways to write a report out: each exporter writes it in a format of its own."""

import json
from abc import ABC, abstractmethod
from pathlib import Path

from exports.reports import Receipt, Report


class Exporter(ABC):
    @abstractmethod
    def export(self, report: Report) -> Receipt:
        """Write the report out, and say where."""


class CsvExporter(Exporter):
    def export(self, report: Report) -> Receipt:
        path = Path("out/report.csv")
        path.write_text("\n".join(report.rows))
        return Receipt(path)


class JsonExporter(Exporter):
    def export(self, report: Report) -> Receipt:
        path = Path("out/report.json")
        path.write_text(json.dumps({"title": report.title, "rows": report.rows}))
        return Receipt(path)
