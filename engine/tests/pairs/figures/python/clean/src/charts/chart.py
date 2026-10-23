"""A chart is one figure: made the first time a line is drawn on it, and drawn on by every line after it."""

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


def compare(xs: list[float], ys: list[float], zs: list[float]) -> Chart:
    chart = Chart("compare")
    chart.line(xs, ys)
    chart.line(xs, zs)
    return chart
