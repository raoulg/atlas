from fits.model import Fit, Result


def fit(xs: list[float]) -> Result:
    return Fit(sum(xs)) if xs else None
