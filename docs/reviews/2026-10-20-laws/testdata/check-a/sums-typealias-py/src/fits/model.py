from dataclasses import dataclass
from typing import TypeAlias


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str


Result: TypeAlias = Fit | Failed
