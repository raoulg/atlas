"""show reads the log as it was, then save writes on it (and shows again): read, then written."""

from pathlib import Path

LOG = Path("data/log.txt")


def save(n: int) -> None:
    LOG.write_text(str(n))
    if n > 0:
        show(n - 1)


def show(n: int) -> None:
    print(LOG.read_text())
    save(n)


def run() -> None:
    show(3)
