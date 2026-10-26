"""Summary statistics of a station's readings, computed by pandas."""

import pandas as pd

from harvest.model import Fetched, Outage


def summarise(rows: list[dict]) -> pd.DataFrame:
    return pd.DataFrame(rows).describe()


def unreached(results: list[Fetched]) -> list[str]:
    """The stations a harvest could not reach."""
    return [r.station for r in results if isinstance(r, Outage)]
