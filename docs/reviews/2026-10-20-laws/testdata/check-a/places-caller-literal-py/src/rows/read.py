from pathlib import Path


def read_rows(path: Path) -> list[str]:
    return path.read_text().splitlines()


def default_rows() -> list[str]:
    return read_rows(Path("data/rows.csv"))
