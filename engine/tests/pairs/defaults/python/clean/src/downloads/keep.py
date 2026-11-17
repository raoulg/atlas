"""A report kept where its caller says, `out/report.txt` unless told; a download of each kind kept below the
folder `RAW` names; a note kept where its caller says, else in the workbench of the home it is handed."""

import os
from pathlib import Path

from downloads.bench import made


def save_report(text: str, path: Path = Path("out/report.txt")) -> None:
    path.write_text(text)


def fetch(kind: str, body: str) -> None:
    (Path(os.environ["RAW"]) / kind / "download.json").write_text(body)


def keep(text: str, home: Path, out: Path | None = None) -> None:
    (out or made(home, "kept.txt")).write_text(text)
