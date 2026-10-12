from pathlib import Path
from typing import Callable

import click
from fastmcp import FastMCP

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


server = FastMCP("fixture")


@server.tool
def describe(name: str) -> str:
    return name


@server.prompt("summary")
def summarise() -> str:
    return "done"


class Prompts:
    """Not a tool server, whatever its value is called."""

    def prompt(self, fn: Callable[[], str]) -> Callable[[], str]:
        return fn


mcp_prompts = Prompts()


@mcp_prompts.prompt
def kept() -> str:
    return "kept"


def show(loaded: Loaded) -> None:
    click.echo(loaded)


def main():
    cli()
