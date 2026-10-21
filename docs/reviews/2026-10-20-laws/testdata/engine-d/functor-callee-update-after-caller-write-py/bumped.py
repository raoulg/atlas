"""run writes the counter, then bump reads that back and writes the next one.

Nothing the route reads was there before it ran: no update of what was found.
"""

from pathlib import Path

COUNTER = Path("data/counter.txt")


def bump() -> None:
    n = int(COUNTER.read_text())
    COUNTER.write_text(str(n + 1))


def run() -> None:
    COUNTER.write_text("0")
    bump()
