"""A command names where its report goes; the helper beside it names its own file."""

from pathlib import Path

import click


@click.command()
def report() -> None:
    Path("out/report.txt").write_text("rows\n")
    summary("rows kept")


def summary(text: str) -> None:
    Path("out/summary.txt").write_text(text)
