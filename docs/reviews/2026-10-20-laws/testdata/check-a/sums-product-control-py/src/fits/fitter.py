from fits.model import Failed, Fit


def fit(xs: list[float]) -> tuple[Fit, Failed]:
    return Fit(sum(xs)), Failed("none")
