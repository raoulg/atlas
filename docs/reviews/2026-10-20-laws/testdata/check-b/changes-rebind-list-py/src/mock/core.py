from dataclasses import dataclass


@dataclass
class Row:
    text: str


def with_row(rows: list[Row], r: Row) -> list[Row]:
    """Hands back a longer list; the list it is handed stays as it was."""
    rows = list(rows)
    rows.append(r)
    return rows


def main() -> int:
    return len(with_row([], Row("a")))
