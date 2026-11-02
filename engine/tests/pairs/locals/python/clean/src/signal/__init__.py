"""Two levels of a signal, shown side by side: `_pair` makes the list that holds them by its own code."""

from dataclasses import dataclass

__all__ = ["paired", "named", "written", "gathered", "chosen"]


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


def _draw(gap: float) -> None:
    print(gap)


def named(first: Level, second: Level) -> None:
    """The gap between the two levels and the two levels side by side, each named before it is shown."""
    gap = first.db - second.db
    _draw(gap)
    both: list[Level] = [first, second]
    _show(both)


def written(first: Level, second: Level) -> None:
    """The gap between the two levels and the two levels side by side, each written where it is shown."""
    _draw(first.db - second.db)
    _show([first, second])


def gathered(first: Level, second: Level) -> None:
    """The two levels, gathered in a list written out once and shown."""
    both: list[Level] = [first, second]
    _show(both)


def chosen(first: Level, second: Level, loud: bool) -> None:
    """The first level, and the second beside it where it is loud: the list the choice gives, written out in each arm, and
    shown."""
    both: list[Level] = [first, second] if loud else [first]
    _show(both)
