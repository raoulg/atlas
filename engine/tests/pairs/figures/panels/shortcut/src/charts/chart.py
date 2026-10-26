"""The same board, which reaches into its charts: it makes each chart's figure and plots on the chart's axes itself."""

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


class Board:
    def __init__(self, left: Chart, right: Chart) -> None:
        self.left = left
        self.right = right

    def draw(self, xs: list[float], ys: list[float], zs: list[float]) -> None:
        self.left.create_figure()
        self.right.create_figure()
        self.left.ax.plot(xs, ys)
        self.right.ax.plot(xs, zs)


def side_by_side(xs: list[float], ys: list[float], zs: list[float]) -> Board:
    board = Board(Chart("left"), Chart("right"))
    board.draw(xs, ys, zs)
    return board
