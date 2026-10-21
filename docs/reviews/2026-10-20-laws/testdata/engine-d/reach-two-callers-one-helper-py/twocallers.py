"""Two steps hand one helper different files: each step writes only its own."""

from pathlib import Path


def write_to(name: str) -> None:
    Path(name).write_text("x")


def alpha() -> None:
    write_to("data/alpha.txt")


def beta() -> None:
    write_to("data/beta.txt")


def run() -> None:
    alpha()
    beta()
