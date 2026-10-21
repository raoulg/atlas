"""The points a plot of a fit draws."""

from typing import Optional

from fits.model import Failed, Fit


def points(r: Optional[Fit | Failed]) -> list[float]:
    if isinstance(r, Fit):
        return [r.score]
    return []
