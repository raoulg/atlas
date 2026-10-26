"""The command names the file the rows come from."""

from pathlib import Path

from stock.rows import read_rows, save


def main() -> None:
    save(read_rows(Path("data/rows.csv")))
