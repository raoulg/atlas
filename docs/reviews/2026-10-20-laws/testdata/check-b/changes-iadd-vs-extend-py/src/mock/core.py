import random
from dataclasses import dataclass


@dataclass
class Row:
    text: str


def by_extend(rows: list[Row], r: Row) -> int:
    rows.extend([r])
    return len(rows)


def by_iadd(rows: list[Row], r: Row) -> int:
    rows += [r]
    return len(rows)


def by_shuffle(rows: list[Row]) -> int:
    random.shuffle(rows)
    return len(rows)


def main() -> int:
    return by_extend([], Row("a")) + by_iadd([], Row("a")) + by_shuffle([])
