"""A typer app's commands, named as typer names them."""

import typer

app = typer.Typer()


@app.command()
def clean_cache_cmd() -> None:
    typer.echo("cleaned")


@app.command("sync")
def synchronise() -> None:
    typer.echo("synced")
