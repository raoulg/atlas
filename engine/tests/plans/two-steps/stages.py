"""One step leaves a file for the next, which reads and uses it."""

from pathlib import Path

STAGE = Path("data/stage.txt")


def fetch() -> None:
    STAGE.write_text("raw")


def process() -> str:
    return STAGE.read_text().upper()


def run() -> None:
    fetch()
    print(process())
