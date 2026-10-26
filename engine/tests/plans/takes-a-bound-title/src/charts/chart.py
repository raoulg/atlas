"""Two charts, where the first lends the second its title, bound to a name first, in the place of its figure."""

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
        t = self.title
        other.take(t, None)

    def line(self, xs: list[float], ys: list[float]) -> None:
        if self.ax is None:
            self.create_figure()
        self.ax.plot(xs, ys)


def overlay(xs: list[float], ys: list[float], zs: list[float]) -> Chart:
    first = Chart("first")
    second = Chart("second")
    first.line(xs, ys)
    first.lend(second)
    second.line(xs, zs)
    return second
