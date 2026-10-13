"""The choices of a run that are not places: how far to smooth and how the figure looks."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Settings:
    months: int = 13
    title: str = "Monthly mean total sunspot number"
    size: tuple[float, float] = (10.0, 4.0)
    dpi: int = 150
