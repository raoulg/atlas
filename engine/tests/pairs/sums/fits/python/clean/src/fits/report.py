"""What a fit says: a line, and the points a plot of it draws."""

from dataclasses import dataclass

from fits.model import Failed, Fit


@dataclass
class Report:
    line: str
    points: list[float]


def describe(r: Fit | Failed) -> Report:
    match r:
        case Fit(score=s):
            return Report(f"fit {s:.2f}", [s])
        case Failed(reason=why):
            return Report(f"failed: {why}", [])
