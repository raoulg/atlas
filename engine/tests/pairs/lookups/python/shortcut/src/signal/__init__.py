"""The levels of a signal, each looked up in a collection the module keeps: by a subscript, or by the call that looks it
up. `pushed` and `pushed_looked_up` look up in a list they change in place first, which starts at the module's quiet level;
`added` and `added_looked_up` change the row they look up, and so the module's rows."""

from dataclasses import dataclass

__all__ = ["indexed", "looked_up", "keyed", "got", "pushed", "pushed_looked_up", "added", "added_looked_up"]


@dataclass
class Level:
    """A level of a signal."""

    db: float


QUIET = Level(0.0)
LEVELS: list[Level] = [Level(-3.0), Level(-12.0)]
BY_NAME: dict[str, Level] = {"loud": Level(-3.0), "soft": Level(-12.0)}
ROWS: list[list[Level]] = [[Level(-3.0)], [Level(-12.0)]]


def _draw(level: Level | None) -> None:
    print(level)


def _show(rows: list[list[Level]]) -> None:
    print(len(rows))


def indexed() -> None:
    """The first level, by its place."""
    first = LEVELS[0]
    _draw(first)


def looked_up() -> None:
    """The first level, the first the list hands out."""
    first = next(iter(LEVELS))
    _draw(first)


def keyed(name: str) -> None:
    """The level of a name, by its key."""
    level = BY_NAME[name]
    _draw(level)


def got(name: str) -> None:
    """The level of a name, got by its key."""
    level = BY_NAME.get(name)
    _draw(level)


def pushed(second: Level) -> None:
    """The quiet level and a second gathered in a list, the first by its place."""
    both = [QUIET]
    both.append(second)
    _draw(both[0])


def pushed_looked_up(second: Level) -> None:
    """The quiet level and a second gathered in a list, the first the list hands out."""
    both = [QUIET]
    both.append(second)
    _draw(next(iter(both)))


def added(level: Level) -> None:
    """A level added to the module's first row, by its place."""
    row = ROWS[0]
    row.append(level)
    _show(ROWS)


def added_looked_up(level: Level) -> None:
    """A level added to the module's first row, the first the rows hand out."""
    row = next(iter(ROWS))
    row.append(level)
    _show(ROWS)
