"""alpha writes only alpha.txt; the route reads beta.txt as it was, and only then beta writes it."""

from pathlib import Path


def write_to(name: str) -> None:
    Path(name).write_text("x")


def alpha() -> None:
    write_to("data/alpha.txt")


def beta() -> None:
    write_to("data/beta.txt")


def show_beta() -> None:
    print(Path("data/beta.txt").read_text())


def run() -> None:
    alpha()
    show_beta()
    beta()
