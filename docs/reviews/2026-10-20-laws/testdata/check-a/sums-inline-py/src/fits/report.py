from fits.model import Failed, Fit


def report(r: Fit | Failed) -> str:
    match r:
        case Fit(score=s):
            return f"fit {s}"
        case Failed(reason=why):
            return f"failed: {why}"
