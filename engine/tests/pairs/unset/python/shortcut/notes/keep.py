"""A report and a log, each kept in the file an environment variable names, or in the empty path
where it names none: `keep` falls back for the variable its caller names, and `show_report` reads
the report back from the file its own lookup names."""

import os
from pathlib import Path


def setting(name: str) -> str:
    return os.environ.get(name, "")


def keep(name: str, text: str) -> None:
    Path(setting(name)).write_text(text)


def save_report(text: str) -> None:
    keep("REPORT", text)


def save_log(text: str) -> None:
    keep("LOG", text)


def show_report() -> str:
    return Path(os.environ.get("REPORT", "")).read_text()
