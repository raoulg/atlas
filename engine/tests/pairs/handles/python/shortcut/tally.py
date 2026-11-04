"""Count the lines of a data file and write the count to a report."""


def main() -> None:
    emit(count())


def count() -> int:
    """The lines of the data file."""
    with open("data.txt") as f:
        return len(f.readlines())


def emit(n: int) -> None:
    """Write the count to the report."""
    with open("report.txt", "w") as out:
        out.write(f"{n}\n")
