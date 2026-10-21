"""In a loop, show the log and then write the next line on it."""

from pathlib import Path

LOG = Path("data/log.txt")


def run() -> None:
    for i in range(3):
        print(LOG.read_text())
        LOG.write_text(f"step {i}")
