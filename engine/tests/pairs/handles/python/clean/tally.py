"""Count the lines of a data file and write the count to a report."""

from typing import TextIO

DATA = "data.txt"
"""The data file."""
REPORT = "report.txt"
"""The report its count is written to."""


def main() -> None:
    with open(DATA) as data:
        n = count(data)
    with open(REPORT, "w") as report:
        emit(report, n)


def count(f: TextIO) -> int:
    """The lines of the file handed in."""
    return len(f.readlines())


def emit(out: TextIO, n: int) -> None:
    """Write the count to the file handed in."""
    out.write(f"{n}\n")
