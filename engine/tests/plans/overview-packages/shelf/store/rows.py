"""A row of the shelf's file: made and taken inside the store only."""

from dataclasses import dataclass


@dataclass
class Row:
    title: str
    pages: int


def parse(line: str) -> Row:
    title, pages = line.split(",")
    return Row(title, int(pages))


def ping() -> None:
    pass
