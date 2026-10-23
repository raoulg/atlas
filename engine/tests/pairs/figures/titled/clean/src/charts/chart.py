"""Two charts, the second titled after the first: the first hands its title to the second's `retitle`, which sets it."""

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

    def retitle(self, title: str) -> None:
        self.title = title

    def lend_title(self, other: "Chart") -> None:
        other.retitle(self.title)

    def line(self, xs: list[float], ys: list[float]) -> None:
        if self.ax is None:
            self.create_figure()
        self.ax.plot(xs, ys)


def side_by_side(xs: list[float], ys: list[float], zs: list[float]) -> Chart:
    first = Chart("first")
    second = Chart("second")
    first.line(xs, ys)
    first.lend_title(second)
    second.line(xs, zs)
    return second
