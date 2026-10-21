from pathlib import Path


def default_rows() -> list[str]:
    return Path("data/rows.csv").read_text().splitlines()
