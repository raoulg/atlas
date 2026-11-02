"""The levels of a signal, gathered into a list the body makes by its own code and changed after it binds it, in one arm
of a choice or in a loop: the step after the change shows what the choice or the loop hands on."""

from dataclasses import dataclass

__all__ = ["picked", "gathered", "looped"]


@dataclass
class Level:
    """A level of a signal."""

    db: float


def _show(levels: list[Level]) -> None:
    print(levels)


def picked(level: Level, loud: bool) -> None:
    """The level, shown where it is loud: an empty list, appended to in one arm."""
    both: list[Level] = []
    if loud:
        both.append(level)
    _show(both)


def gathered(first: Level, second: Level, loud: bool) -> None:
    """The first level, and the second beside it where it is loud: a list of the first, appended to in one arm."""
    both: list[Level] = [first]
    if loud:
        both.append(second)
    _show(both)


def looped(levels: list[Level]) -> None:
    """Every level, shown: an empty list, appended to in a loop."""
    every: list[Level] = []
    for level in levels:
        every.append(level)
    _show(every)
