"""A report kept where its caller says, `out/report.txt` unless told; a download of each kind kept below the
folder `RAW` names; a note kept where its caller says, else in the workbench of the home it is handed. The report's
default is decided in the body, behind `None`, and the folder falls back to the empty path where `RAW` is not set."""

import os
from pathlib import Path

from downloads.bench import made


def save_report(text: str, path: Path | None = None) -> None:
    (path or Path("out/report.txt")).write_text(text)


def fetch(kind: str, body: str) -> None:
    (Path(os.environ.get("RAW", "")) / kind / "download.json").write_text(body)


def keep(text: str, home: Path, out: Path | None = None) -> None:
    (out or made(home, "kept.txt")).write_text(text)
