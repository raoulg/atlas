"""Summary statistics of a station's readings, computed by pandas."""

import pandas as pd


def summarise(rows: list[dict]) -> pd.DataFrame:
    return pd.DataFrame(rows).describe()
