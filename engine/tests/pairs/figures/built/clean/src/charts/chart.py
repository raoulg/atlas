"""A board handed its charts: whoever builds the board builds its charts and hands them in, and it keeps each in a field."""

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

    def draw(self, xs: list[float], ys: list[float], zs: list[float]) -> None:
        self.left.line(xs, ys)
        self.right.line(xs, zs)


def side_by_side(xs: list[float], ys: list[float], zs: list[float]) -> Board:
    board = Board(Chart("left"), Chart("right"))
    board.draw(xs, ys, zs)
    return board
