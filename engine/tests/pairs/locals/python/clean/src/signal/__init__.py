"""Two levels of a signal, shown side by side: `_pair` makes the list that holds them by its own code."""

from dataclasses import dataclass

__all__ = ["paired"]


@dataclass
class Level:
    """A level of a signal."""

    db: float


def _show(levels: list[Level]) -> None:
    print(levels)


def _pair(first: Level, second: Level) -> list[Level]:
    both: list[Level] = [first, second]
    return both


def paired(first: Level, second: Level) -> None:
    """The two levels, shown side by side."""
    _show(_pair(first, second))
