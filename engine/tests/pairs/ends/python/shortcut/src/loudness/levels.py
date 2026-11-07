"""Levels measured in decibels, and the loudest of them, printed."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Level:
    """One level, as it was measured."""

    db: float


def loudest(levels: list[Level]) -> None:
    """The loudest of the levels, in decibels."""
    print(max(level.db for level in levels))
