"""Rows kept by kind under the folder whoever calls names."""

import json
import shutil
from pathlib import Path

import pandas as pd


def kinds(data: Path) -> list[Path]:
    return sorted((data / "raw").iterdir())


def report(data: Path, kind: str) -> None:
    (data / kind / "report.json").write_text("{}")


def read_report(data: Path, kind: str) -> str:
    return (data / kind / "report.json").read_text()


def install(default: Path, data: Path) -> None:
    shutil.copy(default, data / "config.toml")


def export(frame: pd.DataFrame, data: Path) -> None:
    frame.to_csv(data / "frame.csv", na_rep="NA")


class Sink:
    """Writes a count of rows to the file it is made with."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def write(self, rows: list[str]) -> None:
        self.path.write_text(json.dumps({"n": len(rows)}))
