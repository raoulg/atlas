"""What fitting a curve gives: a fit, or why there is none."""

from dataclasses import dataclass


@dataclass
class Fit:
    score: float


@dataclass
class Failed:
    reason: str
