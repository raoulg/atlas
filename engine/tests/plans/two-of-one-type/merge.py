from dataclasses import dataclass

__all__ = ["combine"]


@dataclass
class Table:
    rows: list[str]


def join(left: Table, right: Table) -> Table:
    return Table(left.rows + right.rows)


def show(t: Table) -> None:
    print(t.rows)


def combine(left: Table, right: Table) -> None:
    show(join(left, right))
