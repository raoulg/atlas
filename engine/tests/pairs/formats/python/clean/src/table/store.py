"""Where the table is kept, and the two doors to it."""

from pathlib import Path

import pandas as pd

TABLE = Path("data/table.csv")


def save(text: str, to: Path = TABLE) -> None:
    to.write_text(text)


def load(path: Path = TABLE) -> pd.DataFrame:
    return pd.read_csv(path)
