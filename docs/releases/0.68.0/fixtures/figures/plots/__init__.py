"""Two plots of the same numbers, each saved as a file of its own."""

import matplotlib.pyplot as plt


def plots(xs: list[float], ys: list[float]) -> None:
    line, ax = plt.subplots()
    ax.plot(xs, ys)
    line.savefig("line.png")
    spread, ax2 = plt.subplots()
    ax2.hist(ys)
    spread.savefig("spread.png")


def main() -> None:
    plots([1.0, 2.0, 3.0], [2.0, 4.0, 3.0])
