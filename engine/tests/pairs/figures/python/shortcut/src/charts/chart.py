"""A chart that draws each line on a figure of its own: two lines are two figures."""

import matplotlib.pyplot as plt


class Chart:
    def __init__(self, title: str) -> None:
        self.title = title

    def line(self, xs: list[float], ys: list[float]) -> None:
        fig, ax = plt.subplots()
        ax.set_title(self.title)
        ax.plot(xs, ys)


def compare(xs: list[float], ys: list[float], zs: list[float]) -> Chart:
    chart = Chart("compare")
    chart.line(xs, ys)
    chart.line(xs, zs)
    return chart
