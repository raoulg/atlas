"""Edits to a board and a tally that change what they are handed, and settings an `update` makes anew."""

from dataclasses import dataclass, field

from board.core import Board


@dataclass
class Tally:
    counts: list[int] = field(default_factory=list)

    def add(self, n: int) -> None:
        self.counts.append(n)


def tally_up(tally: Tally, n: int) -> None:
    tally.add(n)


def rename(board: Board, name: str) -> None:
    setattr(board, "name", name)


def forget(board: Board) -> None:
    del board.cells[0]


@dataclass(frozen=True)
class Settings:
    depth: int

    def update(self, depth: int) -> "Settings":
        return Settings(depth)


def deeper(settings: Settings) -> Settings:
    return settings.update(settings.depth + 1)
