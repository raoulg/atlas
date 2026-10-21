from fits.model import Failed, Fit, Run


def fit(name: str, xs: list[float]) -> Run:
    return Run(name, Fit(sum(xs)) if xs else Failed("empty"))
