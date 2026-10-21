from dataclasses import dataclass

__all__ = ["run"]


@dataclass
class Table:
    rows: list[str]


def load() -> Table:
    return Table(["b", "a"])


def strip(t: Table) -> Table:
    return Table([r.strip() for r in t.rows])


def tidy(t: Table) -> Table:
    t = strip(t)
    return t


def show(t: Table) -> None:
    print(t.rows)


def run() -> None:
    t = load()
    t = tidy(t)
    show(t)
