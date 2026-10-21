from dataclasses import dataclass


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str


Result = Fit | Failed


def describe(r: Result) -> str:
    match r:
        case Fit(score=s):
            return f"fit {s}"
        case Failed(reason=why):
            return f"failed: {why}"
