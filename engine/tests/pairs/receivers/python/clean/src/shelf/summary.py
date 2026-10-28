"""Where the data lies, and the summary written beside a data file."""

import os
from pathlib import Path


def data_dir() -> Path:
    """The directory `SHELF_DATA` names, else `data` in the user's home."""
    named = os.getenv("SHELF_DATA")
    if named is not None:
        return Path(named)
    return Path.home().joinpath("data")


def summarise(name: str) -> int:
    """Count the lines of the data file `name` and write the count to a `.count` file beside it."""
    source = data_dir().joinpath(name)
    with open(source) as lines:
        count = len(lines.readlines())
    with open(source.with_suffix(".count"), "w") as out:
        out.write(f"{count}\n")
    return count
