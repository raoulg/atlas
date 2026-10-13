"""The monthly mean sunspot number as a frame indexed by month.

SILSO's file has no header; its columns are separated by semicolons, and a month without an
observation has `-1` for its mean.
"""

from pathlib import Path

import pandas as pd

COLUMNS = ["year", "month", "decimal_date", "mean", "std", "observations", "definitive"]
MISSING = -1


def load(path: Path) -> pd.DataFrame:
    """The months in SILSO's file at `path` that have a mean, indexed by their first day."""
    table = pd.read_csv(path, sep=";", header=None, names=COLUMNS, skipinitialspace=True, na_values=[MISSING])
    first_days = pd.to_datetime(table[["year", "month"]].assign(day=1))
    return table.set_index(first_days)[["mean", "std", "observations"]].dropna(subset=["mean"])


def smooth(frame: pd.DataFrame, months: int) -> pd.DataFrame:
    """`frame` with a `smoothed` column: the mean over `months` months, centred on each."""
    return frame.assign(smoothed=frame["mean"].rolling(months, center=True).mean())
