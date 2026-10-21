import os
from pathlib import Path


def save(rows: list[str]) -> None:
    Path(os.environ.get("STORE_PATH") or "out.csv").write_text("\n".join(rows))
