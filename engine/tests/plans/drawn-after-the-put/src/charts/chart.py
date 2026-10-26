"""A chart set into a board's part, drawn by the board and then by its own name: both draw on the one figure it holds."""

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

    def line(self, xs: list[float], ys: list[float]) -> None:
        if self.ax is None:
            self.create_figure()
        self.ax.plot(xs, ys)


class Board:
    def __init__(self, left: Chart, right: Chart) -> None:
        self.left = left
        self.right = right

    def draw(self, xs: list[float], zs: list[float]) -> None:
        self.left.line(xs, zs)
        self.right.line(xs, zs)


def side_by_side(xs: list[float], ys: list[float], zs: list[float]) -> Board:
    left = Chart("left")
    board = Board(left, Chart("right"))
    board.draw(xs, zs)
    left.line(xs, ys)
    return board
