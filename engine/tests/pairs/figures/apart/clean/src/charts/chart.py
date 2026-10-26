"""Two series, each on a figure of its own: `plot` draws one series on one figure, and `compare` calls it for each."""

import matplotlib.pyplot as plt


def plot(title: str, xs: list[float], ys: list[float]) -> None:
    fig, ax = plt.subplots()
    ax.set_title(title)
    ax.plot(xs, ys)


def compare(xs: list[float], ys: list[float], zs: list[float]) -> None:
    plot("squares", xs, ys)
    plot("cubes", xs, zs)
