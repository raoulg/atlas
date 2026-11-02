"""The levels of a signal, each looked up in the collection it is handed: by a subscript, or by the call that looks it
up. `pushed` and `pushed_looked_up` look up in a list they change in place first; `added` and `added_looked_up` change
the row they look up, and so the rows."""

from dataclasses import dataclass

__all__ = ["indexed", "looked_up", "keyed", "got", "pushed", "pushed_looked_up", "added", "added_looked_up"]


@dataclass
class Level:
    """A level of a signal."""

    db: float


def _draw(level: Level | None) -> None:
    print(level)


def _show(rows: list[list[Level]]) -> None:
    print(len(rows))


def indexed(levels: list[Level]) -> None:
    """The first level, by its place."""
    first = levels[0]
    _draw(first)


def looked_up(levels: list[Level]) -> None:
    """The first level, the first the list hands out."""
    first = next(iter(levels))
    _draw(first)


def keyed(levels: dict[str, Level], name: str) -> None:
    """The level of a name, by its key."""
    level = levels[name]
    _draw(level)


def got(levels: dict[str, Level], name: str) -> None:
    """The level of a name, got by its key."""
    level = levels.get(name)
    _draw(level)


def pushed(first: Level, second: Level) -> None:
    """The first of two levels gathered in a list, by its place."""
    both = [first]
    both.append(second)
    _draw(both[0])


def pushed_looked_up(first: Level, second: Level) -> None:
    """The first of two levels gathered in a list, the first the list hands out."""
    both = [first]
    both.append(second)
    _draw(next(iter(both)))


def added(rows: list[list[Level]], level: Level) -> None:
    """A level added to the first row, by its place."""
    row = rows[0]
    row.append(level)
    _show(rows)


def added_looked_up(rows: list[list[Level]], level: Level) -> None:
    """A level added to the first row, the first the rows hand out."""
    row = next(iter(rows))
    row.append(level)
    _show(rows)
