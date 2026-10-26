"""A fit and its residuals, each on a figure the fit keeps: each figure is made only where its own field holds none."""

import matplotlib.pyplot as plt
from matplotlib.axes import Axes
from matplotlib.figure import Figure


class Fit:
    def __init__(self) -> None:
        self.fig: Figure | None = None
        self.ax: Axes | None = None
        self.res_fig: Figure | None = None
        self.res_ax: Axes | None = None

    def create_figure(self) -> None:
        self.fig, self.ax = plt.subplots()

    def create_residuals(self) -> None:
        self.res_fig, self.res_ax = plt.subplots()

    def lend(self, other: "Fit") -> None:
        other.res_fig, other.res_ax = self.res_fig, self.res_ax

    def draw(self, xs: list[float], ys: list[float], fitted: list[float]) -> None:
        if self.ax is None:
            self.create_figure()
        if self.res_ax is None:
            self.create_residuals()
        self.ax.plot(xs, ys)
        self.ax.plot(xs, fitted)
        self.res_ax.plot(xs, [y - f for y, f in zip(ys, fitted)])


def draw_both(xs: list[float], ys: list[float], fitted: list[float]) -> Fit:
    fit = Fit()
    fit.draw(xs, ys, fitted)
    other = Fit()
    fit.lend(other)
    other.draw(xs, fitted, ys)
    return other
