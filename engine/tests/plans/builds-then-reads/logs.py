"""Building a log writes it: `Log("started")` runs `Log.__init__`, which writes the log. The route then reads back what
building it wrote and adds a line to it."""

from pathlib import Path

LOG = Path("data/log.txt")


class Log:
    def __init__(self, title: str) -> None:
        LOG.write_text(title + "\n")


def run() -> None:
    Log("started")
    LOG.write_text(LOG.read_text() + "done\n")
