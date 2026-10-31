"""A level of a signal, shown beside the quiet level: `_pair` makes the list that holds them by its own code, from the
level it is handed and the module's quiet level."""

from dataclasses import dataclass

__all__ = ["paired", "named", "written", "gathered", "lent"]


@dataclass
class Level:
    """A level of a signal."""

    db: float


QUIET = Level(0.0)


def _show(levels: list[Level]) -> None:
    print(levels)


def _pair(first: Level) -> list[Level]:
    both: list[Level] = [first, QUIET]
    return both


def paired(first: Level) -> None:
    """The level, shown beside the quiet level."""
    _show(_pair(first))


def _draw(gap: float) -> None:
    print(gap)


def named(first: Level) -> None:
    """The gap between the level and the quiet level and the two side by side, each named before it is shown."""
    gap = first.db - QUIET.db
    _draw(gap)
    both: list[Level] = [first, QUIET]
    _show(both)


def written(first: Level) -> None:
    """The gap between the level and the quiet level and the two side by side, each written where it is shown."""
    _draw(first.db - QUIET.db)
    _show([first, QUIET])


def gathered(first: Level, second: Level) -> None:
    """The two levels, gathered by appending the second to a list of the first, and shown."""
    both: list[Level] = [first]
    both.append(second)
    _show(both)


def _add(levels: list[Level], level: Level) -> None:
    levels.append(level)


def lent(first: Level, second: Level) -> None:
    """The two levels, gathered by handing a list of the first to a function that adds the second, and shown."""
    both: list[Level] = [first]
    _add(both, second)
    _show(both)
