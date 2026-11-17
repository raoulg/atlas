"""The two doors to a table, each kept where its caller says."""

from pathlib import Path

import pandas as pd


def save(text: str, to: Path) -> None:
    to.write_text(text)


def load(path: Path) -> pd.DataFrame:
    return pd.read_csv(path)
