"""The same two figures, both made in the body of `compare`: one step that does the work of two."""

import matplotlib.pyplot as plt


def compare(xs: list[float], ys: list[float], zs: list[float]) -> None:
    fig, ax = plt.subplots()
    ax.set_title("squares")
    ax.plot(xs, ys)
    other, bx = plt.subplots()
    bx.set_title("cubes")
    bx.plot(xs, zs)
