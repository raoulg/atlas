from fits.model import Failed, Fit, Result


def report(r: Result) -> str:
    match r:
        case Fit(score=s):
            return f"fit {s}"
        case Failed(reason=why):
            return f"failed: {why}"
        case None:
            return "nothing"
