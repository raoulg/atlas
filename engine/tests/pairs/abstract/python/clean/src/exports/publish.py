"""A report written out in the format asked for, and kept in the place asked for."""

from exports.exporters import CsvExporter, Exporter, JsonExporter
from exports.reports import Receipt, Report
from exports.stores import Shelf, Store, Vault


def exporter_for(format: str) -> Exporter:
    if format == "csv":
        return CsvExporter()
    return JsonExporter()


def store_for(place: str) -> Store:
    if place == "vault":
        return Vault()
    return Shelf()


def publish(report: Report, format: str) -> Receipt:
    return exporter_for(format).export(report)


def archive(report: Report, place: str) -> Receipt:
    return store_for(place).keep(report)
