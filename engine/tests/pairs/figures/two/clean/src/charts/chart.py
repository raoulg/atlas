"""A fit and its residuals, each on a chart of its own: the fit holds the two charts, and each chart keeps its one figure."""

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


class Fit:
    def __init__(self, data: Chart, residuals: Chart) -> None:
        self.data = data
        self.residuals = residuals

    def draw(self, xs: list[float], ys: list[float], fitted: list[float]) -> None:
        self.data.line(xs, ys)
        self.data.line(xs, fitted)
        self.residuals.line(xs, [y - f for y, f in zip(ys, fitted)])


def draw_fit(xs: list[float], ys: list[float], fitted: list[float]) -> Fit:
    fit = Fit(Chart("data"), Chart("residuals"))
    fit.draw(xs, ys, fitted)
    return fit
