"""A command names where its report goes; the helper beside it writes where it is told."""

from pathlib import Path

import click


@click.command()
def report() -> None:
    Path("out/report.txt").write_text("rows\n")
    summary("rows kept", Path("out/summary.txt"))


def summary(text: str, path: Path) -> None:
    path.write_text(text)
