"""Where the data lies, and the summary written beside a data file."""

import os
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Entry:
    """A data file on the shelf: the shelf it stands on, and its name there."""

    parent: str
    name: str


def data_dir():
    """The directory `SHELF_DATA` names, else `data` in the user's home."""
    named = os.getenv("SHELF_DATA")
    if named is not None:
        return Path(named)
    return Path.home().joinpath("data")


def summarise(name):
    """Count the lines of the data file `name`, write the count to a `.count` file beside it and log it."""
    source = data_dir().joinpath(name)
    with open(source) as lines:
        count = len(lines.readlines())
    with open(source.with_suffix(".count"), "w") as out:
        out.write(f"{count}\n")
    log(source, count)
    return count


def shelved(entry):
    """Count the lines of the entry's data file, on its shelf."""
    with open(Path(entry.parent, entry.name)) as lines:
        return len(lines.readlines())


def log(source, count):
    """Append the count of the data file `source` to the log in the directory above it."""
    with open(source.parent / "shelf.log", "a") as out:
        out.write(f"{source.name} {count}\n")
