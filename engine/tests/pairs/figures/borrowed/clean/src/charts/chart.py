"""A board borrows another chart's figure for its left part: the lender lends it by its own `lend`, handed the board's part."""

import matplotlib.pyplot as plt
from matplotlib.axes import Axes
from matplotlib.figure import Figure


class Chart:
    def __init__(self, title: str) -> None:
        self.title = title
        self.fig: Figure | None = None
        self.ax: Axes | None = None

    def create_figure(self) -> None:
        self.fig, self.ax = plt.subplots()
        self.ax.set_title(self.title)

    def take(self, fig: Figure | None, ax: Axes | None) -> None:
        self.fig, self.ax = fig, ax

    def lend(self, other: "Chart") -> None:
        other.take(self.fig, self.ax)

    def line(self, xs: list[float], ys: list[float]) -> None:
        if self.ax is None:
            self.create_figure()
        self.ax.plot(xs, ys)


class Board:
    def __init__(self, left: Chart, right: Chart) -> None:
        self.left = left
        self.right = right

    def borrow(self, lender: Chart) -> None:
        lender.lend(self.left)

    def draw(self, xs: list[float], zs: list[float]) -> None:
        self.left.line(xs, zs)
        self.right.line(xs, zs)


def side_by_side(xs: list[float], ys: list[float], zs: list[float]) -> Board:
    first = Chart("first")
    first.line(xs, ys)
    board = Board(Chart("left"), Chart("right"))
    board.borrow(first)
    board.draw(xs, zs)
    return board
