from pathlib import Path

import click

from fixture.build import Loaded, Pipeline
from fixture.sinks.duck import DuckSink


@click.group()
def cli():
    pass


@cli.command()
@click.option("--fast", is_flag=True)
def build(fast):
    Pipeline(DuckSink(Path("g.duckdb"))).run("https://example.org")


@cli.command("report")
def cmd_report():
    click.echo("done")


def show(loaded: Loaded) -> None:
    click.echo(loaded)


def main():
    cli()
