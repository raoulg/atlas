"""The command line: a run names the source and the files, and hands them down."""

from pathlib import Path

import typer

from sunspots.download import download
from sunspots.plot import Trend
from sunspots.series import load, smooth
from sunspots.settings import Settings

SILSO = "https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv"
RAW = Path("data/raw/sunspots.csv")
FIGURE = Path("figures/sunspots.png")

app = typer.Typer()


@app.command()
def run(raw: Path = RAW, figure: Path = FIGURE, months: int = 13) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept, smooth them and save their trend."""
    settings = Settings(months=months)
    if not raw.exists():
        download(SILSO, raw)
    frame = smooth(load(raw), settings.months)
    Trend(settings).save(frame, figure)
