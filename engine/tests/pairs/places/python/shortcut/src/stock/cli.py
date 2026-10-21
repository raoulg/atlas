"""The command reads the rows the library names for it."""

from stock.rows import default_rows, save


def main() -> None:
    save(default_rows())
