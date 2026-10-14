"""Join two files; the operation obtains its paths from the environment."""

import os
from pathlib import Path


def join() -> None:
    first = Path(os.environ["FIRST"])
    second = Path(os.environ["SECOND"])
    destination = Path(os.environ["DESTINATION"])
    destination.write_text(first.read_text() + second.read_text())


def run() -> None:
    join()
