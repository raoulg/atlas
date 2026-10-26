"""What a fit says, as a line."""

from fits.model import Failed, Fit


def line(r: Fit | Failed) -> str:
    match r:
        case Fit(score=s):
            return f"fit {s:.2f}"
        case Failed(reason=why):
            return f"failed: {why}"
