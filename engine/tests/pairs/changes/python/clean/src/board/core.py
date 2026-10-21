"""A scoreboard: a value changes through a method of its own, or a new one is handed back."""

import copy
import heapq
from dataclasses import dataclass, field


@dataclass
class Cell:
    marked: bool = False


@dataclass
class Board:
    name: str = ""
    cells: list[Cell] = field(default_factory=list)
    scores: dict[str, int] = field(default_factory=dict)

    def reset(self) -> None:
        self.cells.clear()
        self.scores.clear()

    def mark_all(self) -> None:
        for cell in self.cells:
            cell.marked = True

    def merged(self, other: "Board") -> "Board":
        return Board(self.name, [*self.cells, *other.cells], {**self.scores, **other.scores})


def with_tag(tags: list[str], tag: str) -> list[str]:
    return [*tags, tag]


def counted(counts: dict[str, int], word: str) -> dict[str, int]:
    return {**counts, word: counts.get(word, 0) + 1}


def renamed(board: Board, name: str) -> Board:
    """Hands back a renamed copy; the board it is handed stays as it was."""
    board = copy.copy(board)
    board.name = name
    return board


def extended(xs: list[int], more: list[int]) -> list[int]:
    xs = xs + more
    xs.append(0)
    return xs


def queued(jobs: list[int], job: int) -> list[int]:
    jobs = list(jobs)
    heapq.heappush(jobs, job)
    return jobs


def tally(words: list[str]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for word in words:
        counts[word] = counts.get(word, 0) + 1
    return counts


def main() -> int:
    board = Board("b", [Cell()])
    board.mark_all()
    board.reset()
    other = renamed(board, "c").merged(board)
    counts = counted(tally(with_tag([], "x")), "y")
    return len(other.cells) + len(counts) + len(extended([1], [2])) + len(queued([], 1))
