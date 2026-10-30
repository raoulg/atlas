"""A level of a signal, shown beside the quiet level: `_pair` makes the list that holds them by its own code, from the
level it is handed and the module's quiet level."""

from dataclasses import dataclass

__all__ = ["paired"]


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
