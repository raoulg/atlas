"""Rows read and kept where whoever calls says; where nobody says, the code names its default."""

import os
from pathlib import Path


def read_rows(path: Path) -> list[str]:
    return path.read_text().splitlines()


def save(rows: list[str], path: Path = Path("out/rows.csv")) -> None:
    path.write_text("\n".join(rows))


def save_or_given(rows: list[str], path: Path | None = None) -> None:
    (path or Path("out/rows.csv")).write_text("\n".join(rows))


def save_where_told(rows: list[str]) -> None:
    Path(os.environ.get("STOCK_PATH", "out/rows.csv")).write_text("\n".join(rows))


def save_or_default(rows: list[str]) -> None:
    Path(os.getenv("STOCK_PATH") or "out/rows.csv").write_text("\n".join(rows))
