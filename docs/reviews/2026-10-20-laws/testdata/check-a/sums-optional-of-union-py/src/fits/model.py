from dataclasses import dataclass
from typing import Optional


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str


Result = Optional[Fit | Failed]
