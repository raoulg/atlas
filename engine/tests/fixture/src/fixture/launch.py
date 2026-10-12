"""What pyproject.toml declares beside [project.scripts]: a graphical program, Poetry's
program, and a plugin a host loads by its group."""

import click


def window() -> None:
    click.echo("a window")


def nightly() -> None:
    click.echo("nightly")


class Exporter:
    """Loaded by a host that asks for `fixture.exporters`."""

    def export(self, text: str) -> str:
        return text
