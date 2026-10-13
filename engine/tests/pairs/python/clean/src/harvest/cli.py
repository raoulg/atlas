"""The command line: a run names its files, its database and the service it asks."""

from pathlib import Path

import typer

from harvest.harvester import build_harvester
from harvest.locations import API_URL, CONFIG_FILE, DATABASE, REPORT_DIR, STATIONS_FILE
from harvest.model import Harvest
from harvest.report import summarise
from harvest.settings import load_settings
from harvest.stations import read_stations

app = typer.Typer()


def show(harvest: Harvest) -> None:
    for outage in harvest.outages:
        typer.echo(f"{outage.station}: no reading ({outage.reason})", err=True)
    for reading in harvest.readings:
        typer.echo(f"{reading.station}: {reading.temperature:.1f}{reading.unit.symbol}")


@app.command()
def collect(config: Path = CONFIG_FILE) -> None:
    """Fetch the latest reading of every station and keep it."""
    harvester = build_harvester(load_settings(config), DATABASE, API_URL)
    show(harvester.collect(read_stations(STATIONS_FILE)))


@app.command()
def report(station: str, config: Path = CONFIG_FILE) -> None:
    """Write a report of one station's readings and show their summary."""
    harvester = build_harvester(load_settings(config), DATABASE, API_URL)
    path = harvester.report(station, REPORT_DIR)
    typer.echo(summarise(harvester.store.history(station)).to_string())
    typer.echo(f"wrote {path}")
