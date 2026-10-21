"""Write a log line and read it back to show it, in one body: what is read back is used."""

from pathlib import Path

LOG = Path("data/log.txt")


def note() -> str:
    LOG.write_text("step")
    return LOG.read_text()


def run() -> None:
    print(note())
