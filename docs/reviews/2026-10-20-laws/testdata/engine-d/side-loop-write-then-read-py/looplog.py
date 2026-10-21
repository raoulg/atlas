"""In a loop, write a log line and show it back."""

from pathlib import Path

LOG = Path("data/log.txt")


def run() -> None:
    for i in range(3):
        LOG.write_text(f"step {i}")
        print(LOG.read_text())
