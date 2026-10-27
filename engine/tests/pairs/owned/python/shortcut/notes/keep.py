"""A report and a log, each kept where the one who runs the program says, both held by the same
name."""

import sys
from pathlib import Path


def save_report(text: str) -> None:
    file = Path(sys.argv[1])
    file.write_text(text)


def save_log(text: str) -> None:
    file = Path(sys.argv[2])
    file.write_text(text)
