"""A report and a log, each kept in the file the environment names, or in the empty path where it
names none."""

import os
from pathlib import Path


def save_report(text: str) -> None:
    Path(os.environ.get("REPORT", "")).write_text(text)


def save_log(text: str) -> None:
    Path(os.environ.get("LOG", "")).write_text(text)
