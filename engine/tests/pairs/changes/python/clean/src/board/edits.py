"""Edits to a board and a tally that hand back the value they make, and settings an `update` makes anew."""

from dataclasses import dataclass, field, replace

from board.core import Board


@dataclass
class Tally:
    counts: list[int] = field(default_factory=list)

    def add(self, n: int) -> None:
        self.counts.append(n)


def tallied(tally: Tally, n: int) -> Tally:
    return Tally([*tally.counts, n])


def renamed(board: Board, name: str) -> Board:
    return replace(board, name=name)


def without_first(board: Board) -> Board:
    return replace(board, cells=board.cells[1:])


@dataclass(frozen=True)
class Settings:
    depth: int

    def update(self, depth: int) -> "Settings":
        return Settings(depth)


def deeper(settings: Settings) -> Settings:
    return settings.update(settings.depth + 1)
