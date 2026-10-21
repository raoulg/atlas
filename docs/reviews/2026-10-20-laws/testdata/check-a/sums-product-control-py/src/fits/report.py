from fits.model import Failed, Fit


def report(r: Fit, why: Failed) -> str:
    return f"fit {r.score}, failed: {why.reason}"
