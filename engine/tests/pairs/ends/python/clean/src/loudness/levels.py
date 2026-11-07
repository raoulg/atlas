"""Levels measured in decibels, and the loudest of them, given to the caller."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Level:
    """One level, as it was measured."""

    db: float


def loudest(levels: list[Level]) -> float:
    """The loudest of the levels, in decibels."""
    return max(level.db for level in levels)
