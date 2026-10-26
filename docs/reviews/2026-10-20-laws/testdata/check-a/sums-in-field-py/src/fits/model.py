from dataclasses import dataclass


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str


Result = Fit | Failed


@dataclass
class Run:
    name: str
    outcome: Result
