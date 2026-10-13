"""The command line."""

import os
from pathlib import Path

import typer
import uvicorn

from harvest.harvester import build_harvester
from harvest.model import Fetched, Outage
from harvest.report import summarise
from harvest.settings import load_settings
from harvest.stations import read_stations

app = typer.Typer()


def show(results: list[Fetched]) -> None:
    for result in results:
        if isinstance(result, Outage):
            typer.echo(f"{result.station}: no reading ({result.reason})", err=True)
        else:
            typer.echo(f"{result.station}: {result.temperature:.1f}{result.unit}")


@app.command()
def collect() -> None:
    """Fetch the latest reading of every station and keep it."""
    config = load_settings()
    harvester = build_harvester(config)
    show(harvester.collect(read_stations(config["stations_file"])))


@app.command()
def report(station: str) -> None:
    """Write a report of one station's readings and show their summary."""
    config = load_settings()
    harvester = build_harvester(config)
    path = harvester.report(station, Path(config["report_dir"]))
    typer.echo(summarise(harvester.store.history(station)).to_string())
    typer.echo(f"wrote {path}")


@app.command()
def serve() -> None:
    """Serve the API on the database the settings name."""
    os.environ["HARVEST_DB"] = load_settings()["database"]
    uvicorn.run("harvest.api:app")
