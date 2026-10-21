"""The file is named on the command line: a hole the code does not fill."""

import sys
from pathlib import Path


def show(name: str) -> None:
    print(Path(name).read_text())


def run() -> None:
    show(sys.argv[1])
