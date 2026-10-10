"""Two routes that meet on the disk: one writes where the other reads."""

from pathlib import Path

import click

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
