"""Fitting a curve to measurements."""

from fits.model import Failed, Fit


def fit(xs: list[float]) -> Fit | Failed:
    if not xs:
        return Failed("no measurements")
    return Fit(sum(xs) / len(xs))
