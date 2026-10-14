"""Join two files; the operation obtains its paths from the environment."""

import os
from pathlib import Path


def read_pair(first: Path, second: Path) -> str:
    return first.read_text() + second.read_text()


def join() -> None:
    first = Path(os.environ["FIRST"])
    second = Path(os.environ["SECOND"])
    destination = Path(os.environ["DESTINATION"])
    destination.write_text(read_pair(first, second))


def run() -> None:
    join()


def overwrite(path: Path) -> None:
    path.write_text(path.read_text().upper())
