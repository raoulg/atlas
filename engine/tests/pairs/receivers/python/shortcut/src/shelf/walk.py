"""A walk along the shelves, from the aisle it starts in to the data file it ends at."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Path:
    """A walk along the shelves: the aisle it starts in, and the data file it ends at."""

    aisle: str
    end: str


def walked(walk):
    """Count the lines of the data file the walk ends at."""
    with open(walk.end) as lines:
        return len(lines.readlines())


def first(end):
    """Count the lines of the data file a walk from the first aisle ends at."""
    return walked(Path("aisle-1", end))
