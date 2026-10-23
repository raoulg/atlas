"""A call fills its parameters from projections of one value: the parameters hold that value, never one from beyond."""

from dataclasses import dataclass

__all__ = ["run"]


@dataclass
class Row:
    text: str


@dataclass
class Sheet:
    title: str
    rows: list[Row]


def show(title: str, rows: list[Row]) -> None:
    for row in rows:
        print(title, row.text)


def run(sheet: Sheet) -> None:
    show(sheet.title, sheet.rows)
