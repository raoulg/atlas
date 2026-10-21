import os
from pathlib import Path


def save(rows: list[str]) -> None:
    Path(os.getenv("STORE_PATH", "out.csv")).write_text("\n".join(rows))
