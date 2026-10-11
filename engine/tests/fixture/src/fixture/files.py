"""Two routes that meet on the disk: one writes where the other reads."""

import json
import shutil
from pathlib import Path

import click
import pandas as pd

DATA = Path("data")


def unpack(archive: str, into: Path) -> None:
    """A folder per kind and archive: `into/{kind}/{archive}/`."""
    kind = archive.split(".")[0]
    target = into / kind / archive
    target.mkdir(parents=True, exist_ok=True)


@click.command()
@click.option("--out", default="data/raw")
def fetch(out: Path) -> None:
    unpack("a.zip", out)


def kinds() -> list[Path]:
    return sorted((DATA / "raw").iterdir())


def report(kind: str) -> None:
    (DATA / kind / "report.json").write_text("{}")


def read_report(kind: str) -> str:
    return (DATA / kind / "report.json").read_text()


def keep(cache_file: Path) -> None:
    with cache_file.open("w", encoding="utf-8") as f:
        json.dump({}, f)


def install(default: Path) -> None:
    shutil.copy(default, DATA / "config.toml")


def export(frame: pd.DataFrame) -> None:
    frame.to_csv(DATA / "frame.csv", na_rep="NA")


class Report:
    """A value that can be read from a file and show itself: data, not a door."""

    def __init__(self, lines: list[str]) -> None:
        self.lines = lines

    @classmethod
    def from_file(cls, path: Path) -> "Report":
        return cls(path.read_text().splitlines())

    def show(self) -> None:
        print("\\n".join(self.lines))
