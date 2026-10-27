"""A report and a log, each kept where the one who runs the program says: two files the code does
not decide, each held by a name that says which."""

import sys
from pathlib import Path


def save_report(text: str) -> None:
    report = Path(sys.argv[1])
    report.write_text(text)


def save_log(text: str) -> None:
    log = Path(sys.argv[2])
    log.write_text(text)
