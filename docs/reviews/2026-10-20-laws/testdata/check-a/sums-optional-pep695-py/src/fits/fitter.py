from fits.model import Failed, Fit, Result


def fit(xs: list[float]) -> Result:
    return Fit(sum(xs)) if xs else Failed("empty")
