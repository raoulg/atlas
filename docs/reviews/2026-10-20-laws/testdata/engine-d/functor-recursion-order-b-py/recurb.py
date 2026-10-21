"""The same program with show defined before save: read, then written."""

from pathlib import Path

LOG = Path("data/log.txt")


def show(n: int) -> None:
    print(LOG.read_text())
    save(n)


def save(n: int) -> None:
    LOG.write_text(str(n))
    if n > 0:
        show(n - 1)


def run() -> None:
    show(3)
