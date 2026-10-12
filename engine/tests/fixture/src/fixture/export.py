"""A command names where its output goes; the helper beside it names its own place."""

from pathlib import Path

import click


@click.command()
def export() -> None:
    Path("export.csv").write_text("id\n")


def summary(text: str) -> None:
    Path("summary.txt").write_text(text)
