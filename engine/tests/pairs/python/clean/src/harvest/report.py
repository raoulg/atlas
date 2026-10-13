"""Summary statistics of a station's readings, computed by pandas."""

import pandas as pd

from harvest.model import Reading


def summarise(readings: list[Reading]) -> pd.DataFrame:
    frame = pd.DataFrame([{"temperature": r.temperature, "humidity": r.humidity} for r in readings])
    return frame.describe()
