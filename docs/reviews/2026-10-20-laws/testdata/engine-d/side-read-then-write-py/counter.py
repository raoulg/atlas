"""Read a counter and write the next one: the route uses and changes the file."""

from pathlib import Path

COUNTER = Path("data/counter.txt")


def run() -> None:
    n = int(COUNTER.read_text())
    COUNTER.write_text(str(n + 1))
