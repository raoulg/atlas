from dataclasses import dataclass
from typing import Union


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str


Result = Union[Fit, Failed]
