"""Rows kept by kind under the folder the module names."""

import json
import shutil
from pathlib import Path

import pandas as pd

DATA = Path("data")


def kinds() -> list[Path]:
    return sorted((DATA / "raw").iterdir())


def report(kind: str) -> None:
    (DATA / kind / "report.json").write_text("{}")


def read_report(kind: str) -> str:
    return (DATA / kind / "report.json").read_text()


def install(default: Path) -> None:
    shutil.copy(default, DATA / "config.toml")


def export(frame: pd.DataFrame) -> None:
    frame.to_csv(DATA / "frame.csv", na_rep="NA")


class Sink:
    """Writes a count of rows to the file it names."""

    def write(self, rows: list[str]) -> None:
        Path("out.json").write_text(json.dumps({"n": len(rows)}))
