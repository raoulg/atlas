from dataclasses import dataclass


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str


type Result = Fit | Failed
