from fits.model import Failed, Fit


def fit(xs: list[float]) -> Fit | Failed:
    return Fit(sum(xs)) if xs else Failed("empty")
