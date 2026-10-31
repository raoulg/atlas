"""A report and a log, each kept in the file the environment names: the program asks for each and
stops where it is not set."""

import os
from pathlib import Path


def save_report(text: str) -> None:
    Path(os.environ["REPORT"]).write_text(text)


def save_log(text: str) -> None:
    Path(os.environ["LOG"]).write_text(text)
