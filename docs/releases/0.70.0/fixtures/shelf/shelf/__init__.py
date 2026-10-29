"""The data files in the user's home, and a log of their counts beside them."""

from pathlib import Path


def data_dir() -> Path:
    """The directory the data files lie in: `data` in the user's home."""
    return Path.home() / "data"


def summarise(name: str) -> int:
    """Count the lines of the data file `name` and log the count beside it."""
    source = data_dir() / name
    with open(source) as lines:
        count = len(lines.readlines())
    log(source, count)
    return count


def log(source: Path, count: int) -> None:
    """Write the count of the data file `source` to the log in its directory."""
    with open(source.parent / "shelf.log", "w") as out:
        out.write(f"{source.name} {count}\n")
