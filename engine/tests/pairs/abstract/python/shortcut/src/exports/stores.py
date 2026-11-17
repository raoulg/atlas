"""Places a report is kept: a store is anything that keeps one."""

from pathlib import Path

from exports.reports import Receipt, Report


class Store:
    def keep(self, report: Report) -> Receipt:
        path = Path("store/report.txt")
        path.write_text(report.title)
        return Receipt(path)


class Shelf(Store):
    def keep(self, report: Report) -> Receipt:
        path = Path("shelf/report.txt")
        path.write_text(report.title)
        return Receipt(path)


class Vault(Store):
    def keep(self, report: Report) -> Receipt:
        path = Path("vault/report.txt")
        path.write_text(report.title)
        return Receipt(path)
