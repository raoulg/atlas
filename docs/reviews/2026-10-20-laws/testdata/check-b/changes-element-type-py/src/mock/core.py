from dataclasses import dataclass


@dataclass
class Row:
    text: str


def add_row(rows: list[Row], r: Row) -> int:
    rows.append(r)
    return len(rows)


def add_text(texts: list[str], t: str) -> int:
    texts.append(t)
    return len(texts)


def main() -> int:
    return add_row([], Row("a")) + add_text([], "a")
