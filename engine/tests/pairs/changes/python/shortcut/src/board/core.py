"""A scoreboard: the functions change what they are handed and give nothing back."""

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

    def take_from(self, other: "Board") -> int:
        self.cells.extend(other.cells)
        other.cells.clear()
        return len(self.cells)

    def take_victim(self, victim: "Board") -> int:
        self.cells.extend(victim.cells)
        victim.cells.clear()
        return len(self.cells)


def reset(board: Board) -> None:
    board.cells.clear()
    board.scores.clear()


def mark_all(board: Board) -> None:
    for cell in board.cells:
        cell.marked = True


def add_tag(tags: list[str], tag: str) -> None:
    tags.append(tag)


def count(counts: dict[str, int], word: str) -> None:
    counts[word] = counts.get(word, 0) + 1


def merge_scores(board: Board, extra: dict[str, int]) -> None:
    board.scores.update(extra)


def rename(board: Board, name: str) -> int:
    b = board
    b.name = name
    return 1


def extend(xs: list[int], more: list[int]) -> int:
    xs += more
    return len(xs)


def queue(jobs: list[int], job: int) -> None:
    heapq.heappush(jobs, job)


def swap(a: Board, b: Board) -> None:
    a.name, b.name = b.name, a.name


def main() -> int:
    board = Board("b", [Cell()])
    mark_all(board)
    reset(board)
    other = Board("c")
    other.take_from(board)
    other.take_victim(board)
    merge_scores(other, {"a": 1})
    swap(board, other)
    tags: list[str] = []
    add_tag(tags, "x")
    counts: dict[str, int] = {}
    count(counts, "y")
    xs = [1]
    jobs: list[int] = []
    queue(jobs, 1)
    return rename(other, "d") + extend(xs, [2]) + len(tags) + len(counts) + len(jobs)
